//! Read-only vendor portal: a vendor contact enters their work email, receives a
//! single-use magic link, and gets a session limited to that vendor's open items.
//!
//! Threat model notes:
//!
//! - The emailed URL carries a short-lived nonce, not the session token. Email is
//!   archived, forwarded and logged; a session JWT sitting in an inbox forever is
//!   a standing credential, a 15-minute single-use nonce is not.
//! - Only the SHA-256 of the nonce is stored, so read access to the database does
//!   not let an attacker mint portal sessions.
//! - `request_link` always returns the same response, so it cannot be used to
//!   discover which domains are onboarded.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use jsonwebtoken::{encode, EncodingKey, Header};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use shared::{
    ApiError, CreateVendorAllowedDomain, VendorAllowedDomainResponse, VendorPortalItem,
    VendorPortalLinkRequest, VendorPortalLinkResponse, VendorPortalMe,
};
use std::sync::Arc;

use crate::db::schema::{
    action_items, categories, status_history, vendor_allowed_domains, vendor_magic_links, vendors,
};
use crate::mail::Email;
use crate::models::{
    ActionItem, Category, NewVendorAllowedDomain, NewVendorMagicLink, StatusHistory, Vendor,
    VendorAllowedDomain,
};
use crate::AppState;

use super::{AuthUser, TokenScope, VendorAuth, VendorClaims, VENDOR_COOKIE_NAME};

/// How long an emailed link stays usable.
const LINK_TTL_MINUTES: i64 = 15;

/// How long the resulting portal session lasts. Shorter than the 24h staff
/// session because these land on machines we do not control.
const SESSION_TTL_HOURS: i64 = 12;

/// Links a single address may request per hour before further requests are
/// silently dropped.
const MAX_LINKS_PER_HOUR: i64 = 5;

/// Statuses that are *not* considered open. Everything else is shown to the vendor.
const CLOSED_STATUSES: [&str; 1] = ["Complete"];

// ============================================================================
// Magic link request
// ============================================================================

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 32 bytes of randomness, rendered as URL-safe alphanumerics.
fn generate_nonce() -> String {
    rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(43)
        .map(char::from)
        .collect()
}

/// Split an address into its domain, lowercased. `None` if it is not shaped like
/// an email address.
pub(super) fn email_domain(email: &str) -> Option<String> {
    let trimmed = email.trim();
    let (local, domain) = trimmed.rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        return None;
    }
    Some(domain.to_ascii_lowercase())
}

pub async fn request_link(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<VendorPortalLinkRequest>,
) -> Response {
    // Always the same reply, whatever happens below.
    let accepted = || {
        Json(VendorPortalLinkResponse {
            status: "If that address is registered, a sign-in link is on its way.".to_string(),
        })
        .into_response()
    };

    let email = payload.email.trim().to_ascii_lowercase();
    let Some(domain) = email_domain(&email) else {
        return accepted();
    };

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // Rate limit per address, independent of whether the address is known.
    let recent: i64 = vendor_magic_links::table
        .filter(vendor_magic_links::email.eq(&email))
        .filter(vendor_magic_links::created_at.gt(Utc::now() - Duration::hours(1)))
        .count()
        .get_result(&mut conn)
        .await
        .unwrap_or(0);

    if recent >= MAX_LINKS_PER_HOUR {
        tracing::warn!("Rate limited vendor portal link request for {email}");
        return accepted();
    }

    // An address may be registered against more than one vendor; send one link per
    // vendor rather than silently picking one.
    let matches: Vec<(VendorAllowedDomain, Vendor)> = vendor_allowed_domains::table
        .inner_join(vendors::table.on(vendors::id.eq(vendor_allowed_domains::vendor_id)))
        .filter(vendor_allowed_domains::domain.eq(&domain))
        .filter(vendors::archived.eq(false))
        .select((VendorAllowedDomain::as_select(), Vendor::as_select()))
        .load(&mut conn)
        .await
        .unwrap_or_default();

    if matches.is_empty() {
        tracing::info!("Vendor portal link requested for unregistered domain {domain}");
        return accepted();
    }

    let mut links = Vec::new();
    for (_, vendor) in &matches {
        let nonce = generate_nonce();
        let new_link = NewVendorMagicLink {
            vendor_id: vendor.id,
            email: email.clone(),
            token_hash: hash_token(&nonce),
            expires_at: Utc::now() + Duration::minutes(LINK_TTL_MINUTES),
        };

        if let Err(e) = diesel::insert_into(vendor_magic_links::table)
            .values(&new_link)
            .execute(&mut conn)
            .await
        {
            tracing::error!("Failed to store vendor magic link: {e}");
            continue;
        }

        links.push((
            vendor.name.clone(),
            format!("{}/vendor/verify?token={}", state.config.public_url, nonce),
        ));
    }

    if links.is_empty() {
        return accepted();
    }

    let email_message = build_link_email(&email, &links);
    let mailer = state.mailer.clone();
    let mail_health = state.mail_health.clone();
    // Send outside the request path: delivery latency should not be observable to
    // a caller probing for valid addresses.
    //
    // The outcome is recorded rather than only logged. This is the path where a
    // rejected send is otherwise invisible — the vendor is told a link is on its
    // way no matter what, so without this the failure reaches nobody.
    tokio::spawn(async move {
        match mailer.send(email_message).await {
            Ok(()) => mail_health.record_success(),
            Err(e) => {
                mail_health.record_failure(e.to_string());
                tracing::error!("Failed to send vendor portal link: {e:#}");
            }
        }
    });

    accepted()
}

fn build_link_email(to: &str, links: &[(String, String)]) -> Email {
    let mut text = String::from("You asked for a link to view your open action items.\n\n");
    let mut html = String::from("<p>You asked for a link to view your open action items.</p>");

    for (vendor_name, url) in links {
        text.push_str(&format!("{vendor_name}:\n{url}\n\n"));
        html.push_str(&format!(
            "<p><strong>{}</strong><br><a href=\"{}\">View open items</a></p>",
            html_escape(vendor_name),
            html_escape(url)
        ));
    }

    let footer = format!(
        "This link expires in {LINK_TTL_MINUTES} minutes and can only be used once. \
         If you did not request it, you can ignore this email."
    );
    text.push_str(&footer);
    html.push_str(&format!("<p style=\"color:#666\">{footer}</p>"));

    Email {
        to: to.to_string(),
        subject: "Your action item portal link".to_string(),
        html,
        text,
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// Magic link verification
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

pub async fn verify(
    State(state): State<Arc<AppState>>,
    Query(query): Query<VerifyQuery>,
) -> Response {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let token_hash = hash_token(&query.token);
    let now = Utc::now();

    // Claim the link and mark it consumed in one statement, so two concurrent
    // requests with the same nonce cannot both succeed.
    let claimed: Option<(i32, String)> = diesel::update(
        vendor_magic_links::table
            .filter(vendor_magic_links::token_hash.eq(&token_hash))
            .filter(vendor_magic_links::consumed_at.is_null())
            .filter(vendor_magic_links::expires_at.gt(now)),
    )
    .set(vendor_magic_links::consumed_at.eq(now))
    .returning((vendor_magic_links::vendor_id, vendor_magic_links::email))
    .get_result(&mut conn)
    .await
    .optional()
    .unwrap_or(None);

    let Some((link_vendor_id, link_email)) = claimed else {
        return portal_redirect("/vendor?error=invalid_link");
    };

    // The vendor may have been archived between request and click.
    let vendor: Option<Vendor> = vendors::table
        .filter(vendors::id.eq(link_vendor_id))
        .filter(vendors::archived.eq(false))
        .first(&mut conn)
        .await
        .optional()
        .unwrap_or(None);

    if vendor.is_none() {
        return portal_redirect("/vendor?error=invalid_link");
    }

    let expires_at = now + Duration::hours(SESSION_TTL_HOURS);
    let claims = VendorClaims {
        sub: link_email,
        vendor_id: link_vendor_id,
        scope: TokenScope::VendorPortal,
        iat: now.timestamp() as usize,
        exp: expires_at.timestamp() as usize,
    };

    let secret = super::vendor_jwt_secret(&state.config.jwt_secret);
    let token = match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to mint vendor portal token: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to create session")),
            )
                .into_response();
        }
    };

    let secure = if state.config.public_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={}; Path=/vendor; HttpOnly; SameSite=Lax; Max-Age={}{}",
        VENDOR_COOKIE_NAME,
        token,
        SESSION_TTL_HOURS * 3600,
        secure
    );

    (
        StatusCode::FOUND,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/vendor".to_string()),
        ],
    )
        .into_response()
}

fn portal_redirect(location: &str) -> Response {
    (
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response()
}

pub async fn logout() -> Response {
    (
        StatusCode::OK,
        [(header::SET_COOKIE, super::CLEAR_VENDOR_COOKIE)],
        Json(shared::LogoutResponse {
            status: "logged out".to_string(),
        }),
    )
        .into_response()
}

// ============================================================================
// Portal API (read-only, scoped to one vendor)
// ============================================================================

pub async fn me(auth: VendorAuth) -> Response {
    Json(VendorPortalMe {
        vendor_id: auth.vendor.id,
        vendor_prefix: auth.vendor.prefix,
        vendor_name: auth.vendor.name,
        email: auth.email,
        session_expires_at: auth.expires_at,
    })
    .into_response()
}

/// Current status of every item belonging to a vendor, in one query.
///
/// Mirrors `items::current_status` but batched: the portal renders a whole list
/// and per-item status lookups would be N+1.
async fn statuses_for_vendor(
    conn: &mut AsyncPgConnection,
    vendor_id: i32,
) -> std::collections::HashMap<String, (String, DateTime<Utc>)> {
    let rows: Vec<StatusHistory> = status_history::table
        .inner_join(action_items::table.on(action_items::id.eq(status_history::action_item_id)))
        .filter(action_items::vendor_id.eq(vendor_id))
        .order(status_history::changed_at.asc())
        .select(StatusHistory::as_select())
        .load(conn)
        .await
        .unwrap_or_default();

    // Ascending order means the last write for each item wins.
    let mut latest = std::collections::HashMap::new();
    for row in rows {
        latest.insert(row.action_item_id, (row.status, row.changed_at));
    }
    latest
}

pub async fn list_items(State(state): State<Arc<AppState>>, auth: VendorAuth) -> Response {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let items: Vec<(ActionItem, Category)> = match action_items::table
        .inner_join(categories::table.on(categories::id.eq(action_items::category_id)))
        .filter(action_items::vendor_id.eq(auth.vendor.id))
        .order(action_items::id.asc())
        .select((ActionItem::as_select(), Category::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to fetch vendor portal items: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch items")),
            )
                .into_response();
        }
    };

    let statuses = statuses_for_vendor(&mut conn, auth.vendor.id).await;

    let result: Vec<VendorPortalItem> = items
        .into_iter()
        .filter_map(|(item, category)| {
            let (status, status_changed_at) = statuses
                .get(&item.id)
                .cloned()
                .unwrap_or_else(|| ("New".to_string(), item.created_at));

            if CLOSED_STATUSES.contains(&status.as_str()) {
                return None;
            }

            Some(VendorPortalItem {
                id: item.id,
                title: item.title,
                description: item.description,
                category: category.name,
                priority: item.priority,
                status,
                create_date: item.create_date,
                due_date: item.due_date,
                status_changed_at,
            })
        })
        .collect();

    Json(result).into_response()
}

// ============================================================================
// Staff-side configuration of allowed domains
// ============================================================================

fn to_shared_domain(d: &VendorAllowedDomain) -> VendorAllowedDomainResponse {
    VendorAllowedDomainResponse {
        id: d.id,
        vendor_id: d.vendor_id,
        domain: d.domain.clone(),
        created_at: d.created_at,
    }
}

pub async fn list_domains(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    _auth: AuthUser,
) -> Response {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let domains: Vec<VendorAllowedDomain> = vendor_allowed_domains::table
        .filter(vendor_allowed_domains::vendor_id.eq(vendor_id))
        .order(vendor_allowed_domains::domain.asc())
        .select(VendorAllowedDomain::as_select())
        .load(&mut conn)
        .await
        .unwrap_or_default();

    Json(domains.iter().map(to_shared_domain).collect::<Vec<_>>()).into_response()
}

/// Accept only a bare hostname: no scheme, no '@', at least one dot, and nothing
/// that would let an operator paste a full address and silently widen access.
fn normalise_domain(input: &str) -> Result<String, &'static str> {
    let domain = input.trim().trim_start_matches('@').to_ascii_lowercase();

    if domain.is_empty() || domain.len() > 255 {
        return Err("Domain must be 1-255 characters");
    }
    if domain.contains('@') || domain.contains('/') || domain.contains(' ') {
        return Err("Enter a bare domain, e.g. acme.com");
    }
    if !domain.contains('.') {
        return Err("Domain must include a dot, e.g. acme.com");
    }
    if !domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err("Domain may only contain letters, digits, dots and hyphens");
    }
    if domain.starts_with('.') || domain.ends_with('.') || domain.starts_with('-') {
        return Err("Domain is not well formed");
    }

    Ok(domain)
}

pub async fn add_domain(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    _auth: AuthUser,
    Json(payload): Json<CreateVendorAllowedDomain>,
) -> Response {
    let domain = match normalise_domain(&payload.domain) {
        Ok(d) => d,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::validation_error(message)),
            )
                .into_response()
        }
    };

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let new_domain = NewVendorAllowedDomain { vendor_id, domain };

    match diesel::insert_into(vendor_allowed_domains::table)
        .values(&new_domain)
        .returning(VendorAllowedDomain::as_returning())
        .get_result::<VendorAllowedDomain>(&mut conn)
        .await
    {
        Ok(d) => (StatusCode::CREATED, Json(to_shared_domain(&d))).into_response(),
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        )) => (
            StatusCode::CONFLICT,
            Json(ApiError::conflict("That domain is already allowed")),
        )
            .into_response(),
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::ForeignKeyViolation,
            _,
        )) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Vendor {vendor_id} not found"))),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to add allowed domain: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to add domain")),
            )
                .into_response()
        }
    }
}

pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Path((vendor_id, domain_id)): Path<(i32, i32)>,
    _auth: AuthUser,
) -> Response {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let deleted = diesel::delete(
        vendor_allowed_domains::table
            .filter(vendor_allowed_domains::id.eq(domain_id))
            .filter(vendor_allowed_domains::vendor_id.eq(vendor_id)),
    )
    .execute(&mut conn)
    .await
    .unwrap_or(0);

    if deleted == 0 {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found("Domain not found for this vendor")),
        )
            .into_response();
    }

    // Revoking a domain must also kill links already in flight for it.
    let _ = diesel::delete(
        vendor_magic_links::table
            .filter(vendor_magic_links::vendor_id.eq(vendor_id))
            .filter(vendor_magic_links::consumed_at.is_null()),
    )
    .execute(&mut conn)
    .await;

    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_domain_extracts_and_lowercases() {
        assert_eq!(
            email_domain("Person@Acme.COM"),
            Some("acme.com".to_string())
        );
        assert_eq!(
            email_domain("  person@mail.acme.com "),
            Some("mail.acme.com".to_string())
        );
    }

    #[test]
    fn email_domain_rejects_malformed_addresses() {
        assert_eq!(email_domain("no-at-sign"), None);
        assert_eq!(email_domain("@acme.com"), None);
        assert_eq!(email_domain("person@"), None);
        assert_eq!(email_domain("person@localhost"), None);
    }

    #[test]
    fn subdomains_do_not_match_parent_domain() {
        // The whole point of exact matching: acme.com must not admit
        // someone@evil.acme.com unless that subdomain is registered too.
        assert_ne!(
            email_domain("attacker@evil.acme.com"),
            email_domain("staff@acme.com")
        );
    }

    #[test]
    fn normalise_domain_accepts_bare_hostnames() {
        assert_eq!(normalise_domain("Acme.com").unwrap(), "acme.com");
        assert_eq!(normalise_domain("@acme.com").unwrap(), "acme.com");
        assert_eq!(
            normalise_domain(" mail.acme.com ").unwrap(),
            "mail.acme.com"
        );
    }

    #[test]
    fn normalise_domain_rejects_addresses_and_urls() {
        assert!(normalise_domain("person@acme.com").is_err());
        assert!(normalise_domain("https://acme.com").is_err());
        assert!(normalise_domain("acme").is_err());
        assert!(normalise_domain("").is_err());
        assert!(normalise_domain(".acme.com").is_err());
    }

    #[test]
    fn hash_token_is_stable_and_hex() {
        let hash = hash_token("abc");
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_token("abc"));
        assert_ne!(hash, hash_token("abd"));
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn nonces_are_unique_and_long() {
        let a = generate_nonce();
        let b = generate_nonce();
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
    }
}
