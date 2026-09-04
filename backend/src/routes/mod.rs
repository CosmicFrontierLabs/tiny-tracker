pub mod activity;
pub mod auth;
pub mod categories;
pub mod health;
pub mod items;
pub mod notes;
pub mod status;
pub mod users;
pub mod vendor_portal;
pub mod vendors;

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Object;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use shared::ApiError;
use std::sync::Arc;

use crate::db::schema::action_items;
use crate::AppState;

pub(super) const CLEAR_TOKEN_COOKIE: &str = "token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

/// Vendor portal cookies are scoped to `/vendor` so the browser never attaches
/// them to a staff `/api/*` request, and vice versa.
pub(super) const VENDOR_COOKIE_NAME: &str = "vendor_token";
pub(super) const CLEAR_VENDOR_COOKIE: &str =
    "vendor_token=; Path=/vendor; HttpOnly; SameSite=Lax; Max-Age=0";

// These two helpers return an axum `Response` as their error, which is the
// idiomatic way to short-circuit a handler with a ready-made HTTP reply. Since
// Rust 1.98 clippy flags that as a large `Err` variant. Boxing it would ripple
// `*resp` derefs through every route module for no measurable gain on a path
// that only runs when a request is already failing.
#[allow(clippy::result_large_err)]
/// Acquire a pooled database connection, mapping pool failures to a 500 response.
pub(super) async fn get_conn(state: &AppState) -> Result<Object<AsyncPgConnection>, Response> {
    state.pool.get().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Database connection failed")),
        )
            .into_response()
    })
}

#[allow(clippy::result_large_err)] // see note on `get_conn`
/// Ensure an action item exists, returning a 404 response if it does not.
pub(super) async fn ensure_item_exists(
    conn: &mut AsyncPgConnection,
    item_id: &str,
) -> Result<(), Response> {
    let exists: bool = action_items::table
        .filter(action_items::id.eq(item_id))
        .count()
        .get_result::<i64>(conn)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

    if exists {
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Action item {} not found",
                item_id
            ))),
        )
            .into_response())
    }
}

/// Distinguishes a full staff session from a read-only vendor portal session.
///
/// Staff and vendor tokens are already signed with different secrets, so a vendor
/// token cannot validate as a staff token even without this field. It is checked
/// as well so that a future signing-key change cannot silently collapse the two.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// Full access to `/api/*`. The default so that staff cookies issued before
    /// the vendor portal existed keep working until they expire.
    #[default]
    Staff,
    /// Read-only access to a single vendor's open items via `/vendor/api/*`.
    VendorPortal,
}

/// Derive the vendor portal signing key from the staff secret.
///
/// Deriving rather than adding a second env var keeps deployment unchanged, while
/// still guaranteeing the two token families have disjoint signatures.
pub fn vendor_jwt_secret(jwt_secret: &str) -> String {
    format!("{jwt_secret}|vendor-portal-v1")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // email
    pub name: String,
    pub user_id: i32,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub scope: TokenScope,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VendorClaims {
    pub sub: String, // vendor contact email
    pub vendor_id: i32,
    pub scope: TokenScope,
    pub exp: usize,
    pub iat: usize,
}

/// A verified read-only vendor portal session.
///
/// Carries the vendor row itself: the extractor has to load it anyway to confirm
/// the vendor is still active, and holding it here means no portal route can
/// forget that check.
pub struct VendorAuth {
    pub vendor: crate::models::Vendor,
    pub email: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct AuthUser {
    pub user_id: i32,
    pub email: String,
    pub name: String,
}

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Dev mode bypass
        if state.config.dev_mode {
            if let Some(dev_user_id) = state.config.dev_user_id {
                return Ok(AuthUser {
                    user_id: dev_user_id,
                    email: "dev@localhost".to_string(),
                    name: "Dev User".to_string(),
                });
            }
            // Default dev user
            return Ok(AuthUser {
                user_id: 1,
                email: "dev@localhost".to_string(),
                name: "Dev User".to_string(),
            });
        }

        // Try to get token from cookie
        let cookie_header = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let token = cookie_header
            .split(';')
            .find_map(|cookie| {
                let cookie = cookie.trim();
                if cookie.starts_with("token=") {
                    Some(cookie.trim_start_matches("token="))
                } else {
                    None
                }
            })
            .or_else(|| {
                // Fallback to Authorization header
                parts
                    .headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
            });

        let token = match token {
            Some(t) => t,
            None => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ApiError::unauthorized("Missing authentication token")),
                )
                    .into_response())
            }
        };

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                [(header::SET_COOKIE, CLEAR_TOKEN_COOKIE)],
                Json(ApiError::unauthorized("Invalid or expired token")),
            )
                .into_response()
        })?;

        // A vendor portal token must never satisfy a staff route, even if it
        // somehow validates against the staff key.
        if token_data.claims.scope != TokenScope::Staff {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError::unauthorized("Token is not valid for this API")),
            )
                .into_response());
        }

        Ok(AuthUser {
            user_id: token_data.claims.user_id,
            email: token_data.claims.sub,
            name: token_data.claims.name,
        })
    }
}

#[axum::async_trait]
impl FromRequestParts<Arc<AppState>> for VendorAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // Deliberately no dev-mode bypass: the portal has no notion of a "dev
        // vendor", and in dev the magic link is printed by the `log` mailer.
        let unauthorized = || {
            (
                StatusCode::UNAUTHORIZED,
                [(header::SET_COOKIE, CLEAR_VENDOR_COOKIE)],
                Json(ApiError::unauthorized(
                    "Vendor portal session is invalid or expired",
                )),
            )
                .into_response()
        };

        let cookie_header = parts
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let prefix = format!("{VENDOR_COOKIE_NAME}=");
        let token = cookie_header
            .split(';')
            .find_map(|cookie| cookie.trim().strip_prefix(&prefix))
            .ok_or_else(unauthorized)?;

        let secret = vendor_jwt_secret(&state.config.jwt_secret);
        let token_data = decode::<VendorClaims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| unauthorized())?;

        if token_data.claims.scope != TokenScope::VendorPortal {
            return Err(unauthorized());
        }

        let expires_at = chrono::DateTime::from_timestamp(token_data.claims.exp as i64, 0)
            .ok_or_else(unauthorized)?;

        // Archiving a vendor is how a relationship is ended, so it must also end
        // any portal session already in flight rather than letting it run out its
        // remaining hours.
        let mut conn = get_conn(state).await?;
        let vendor: crate::models::Vendor = crate::db::schema::vendors::table
            .filter(crate::db::schema::vendors::id.eq(token_data.claims.vendor_id))
            .filter(crate::db::schema::vendors::archived.eq(false))
            .first(&mut conn)
            .await
            .map_err(|_| unauthorized())?;

        Ok(VendorAuth {
            vendor,
            email: token_data.claims.sub,
            expires_at,
        })
    }
}
