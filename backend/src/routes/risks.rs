//! Risk register: vendor-scoped risks and their links to action items.
//!
//! Score and level are never stored. They are computed from severity and
//! likelihood by the `shared` crate on the way out, so the frontend, the API
//! and the CLI importer all agree on the arithmetic.
//!
//! Risks are soft-deleted exactly like action items, so every read path here
//! filters on `deleted_at IS NULL` — including the link-count subquery, which
//! must also ignore deleted *items*, or a deleted item would keep propping
//! up a risk's count.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use serde::Deserialize;
use shared::{
    risk_level, risk_score, ApiError, CreateRisk, LinkItems, LinkRisk, RiskResponse, RiskStatus,
    RiskTrend, UpdateRisk as UpdateRiskReq, RISK_SCALE_MAX,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::schema::{action_item_risks, action_items, categories, risks, users, vendors};
use crate::models::{
    ActionItem, Category, NewActionItemRisk, NewRisk, Risk, UpdateRisk, User, Vendor,
};
use crate::AppState;

use super::{AdminUser, AuthUser};

const TITLE_MAX_LEN: usize = 500;
const CATEGORY_MAX_LEN: usize = 100;
const COMPONENT_MAX_LEN: usize = 100;
const TEST_TYPE_MAX_LEN: usize = 50;
/// Long-text fields. Generous, but bounded so a pasted document cannot be
/// stored as a risk's cause.
const TEXT_MAX_LEN: usize = 10_000;

#[derive(Debug, Deserialize)]
pub struct RisksQuery {
    pub vendor_id: Option<i32>,
    /// Display label, e.g. `In work`. Case-insensitive.
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Trim an optional text field, folding whitespace-only input to `None` so
/// clearing a field in the UI stores NULL rather than an empty string.
fn normalize_optional(
    raw: Option<String>,
    field: &str,
    max_len: usize,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw else { return Ok(None) };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_len {
        return Err(ApiError::validation_error(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

/// Trim a required text field, rejecting empty input.
fn normalize_required(raw: &str, field: &str, max_len: usize) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::validation_error(format!("{field} is required")));
    }
    if trimmed.chars().count() > max_len {
        return Err(ApiError::validation_error(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Severity and likelihood share a 0-5 scale. NaN and infinities are
/// rejected explicitly: JSON cannot carry them, but a `f64` can, and a NaN
/// score would silently sort to the wrong end of the register.
fn validate_scale(value: f64, field: &str) -> Result<(), ApiError> {
    if !value.is_finite() || !(0.0..=RISK_SCALE_MAX).contains(&value) {
        return Err(ApiError::validation_error(format!(
            "{field} must be between 0 and {RISK_SCALE_MAX}"
        )));
    }
    Ok(())
}

fn bad_request(e: ApiError) -> Response {
    (StatusCode::BAD_REQUEST, Json(e)).into_response()
}

fn internal(msg: &'static str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::internal_error(msg)),
    )
        .into_response()
}

fn risk_not_found(risk_id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError::not_found(format!("Risk {} not found", risk_id))),
    )
        .into_response()
}

#[allow(clippy::result_large_err)] // see note on `get_conn` in routes/mod.rs
/// Ensure a risk exists and has not been deleted, returning a 404 otherwise.
async fn ensure_risk_exists(conn: &mut AsyncPgConnection, risk_id: &str) -> Result<(), Response> {
    let exists: bool = risks::table
        .filter(risks::id.eq(risk_id))
        .filter(risks::deleted_at.is_null())
        .count()
        .get_result::<i64>(conn)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

    if exists {
        Ok(())
    } else {
        Err(risk_not_found(risk_id))
    }
}

/// Trim, drop blanks and de-duplicate a list of item IDs, keeping order.
fn dedup_ids(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.trim();
        if !id.is_empty() && !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// Which of `item_ids` are not live action items. Deleted items count as
/// missing: a link to one would be invisible everywhere and only confuse.
async fn missing_live_items(
    conn: &mut AsyncPgConnection,
    item_ids: &[String],
) -> Result<Vec<String>, diesel::result::Error> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let found: Vec<String> = action_items::table
        .filter(action_items::id.eq_any(item_ids))
        .filter(action_items::deleted_at.is_null())
        .select(action_items::id)
        .load(conn)
        .await?;
    Ok(item_ids
        .iter()
        .filter(|id| !found.contains(id))
        .cloned()
        .collect())
}

/// Link every item to the risk, skipping pairs that already exist.
async fn insert_links(
    conn: &mut AsyncPgConnection,
    risk_id: &str,
    item_ids: &[String],
    user_id: i32,
) -> Result<usize, diesel::result::Error> {
    let rows: Vec<NewActionItemRisk> = item_ids
        .iter()
        .map(|item_id| NewActionItemRisk {
            action_item_id: item_id.clone(),
            risk_id: risk_id.to_string(),
            linked_by_id: user_id,
        })
        .collect();
    if rows.is_empty() {
        return Ok(0);
    }
    diesel::insert_into(action_item_risks::table)
        .values(&rows)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
}

// ---------------------------------------------------------------------------
// Response assembly
// ---------------------------------------------------------------------------

fn build_risk_response(
    risk: Risk,
    vendor_prefix: String,
    creator_name: String,
    linked_item_count: i64,
) -> RiskResponse {
    let score = risk_score(risk.severity, risk.likelihood);
    RiskResponse {
        id: risk.id,
        vendor_id: risk.vendor_id,
        vendor_prefix,
        number: risk.number,
        component: risk.component,
        category: risk.category,
        title: risk.title,
        cause: risk.cause,
        effect: risk.effect,
        severity: risk.severity,
        likelihood: risk.likelihood,
        score,
        level: risk_level(score),
        mitigation: risk.mitigation,
        test_type: risk.test_type,
        verification: risk.verification,
        // The CHECK constraints make an unparseable label impossible; the
        // fallbacks are for the type system, not for data.
        status: RiskStatus::from_label(&risk.status).unwrap_or(RiskStatus::Open),
        trend: RiskTrend::from_label(&risk.trend).unwrap_or(RiskTrend::New),
        opened_on: risk.opened_on,
        created_by_id: risk.created_by_id,
        created_by_name: creator_name,
        created_at: risk.created_at,
        updated_at: risk.updated_at,
        linked_item_count,
    }
}

/// Count live action items per risk, for a set of risk IDs, in one query.
async fn linked_item_counts(
    conn: &mut AsyncPgConnection,
    risk_ids: &[String],
) -> Result<HashMap<String, i64>, diesel::result::Error> {
    if risk_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, i64)> = action_item_risks::table
        .inner_join(action_items::table)
        .filter(action_item_risks::risk_id.eq_any(risk_ids))
        .filter(action_items::deleted_at.is_null())
        .group_by(action_item_risks::risk_id)
        .select((action_item_risks::risk_id, diesel::dsl::count_star()))
        .load(conn)
        .await?;
    Ok(rows.into_iter().collect())
}

/// Turn loaded rows into responses, resolving creators and link counts with
/// one query each rather than one per risk.
async fn build_risk_responses(
    conn: &mut AsyncPgConnection,
    rows: Vec<(Risk, Vendor)>,
) -> Result<Vec<RiskResponse>, diesel::result::Error> {
    let creator_ids: Vec<i32> = rows.iter().map(|(r, _)| r.created_by_id).collect();
    let creators: HashMap<i32, String> = users::table
        .filter(users::id.eq_any(&creator_ids))
        .select((users::id, users::name))
        .load::<(i32, String)>(conn)
        .await?
        .into_iter()
        .collect();

    let risk_ids: Vec<String> = rows.iter().map(|(r, _)| r.id.clone()).collect();
    let counts = linked_item_counts(conn, &risk_ids).await?;

    Ok(rows
        .into_iter()
        .map(|(risk, vendor)| {
            let creator = creators
                .get(&risk.created_by_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            let count = counts.get(&risk.id).copied().unwrap_or(0);
            build_risk_response(risk, vendor.prefix, creator, count)
        })
        .collect())
}

/// Register order: highest score first, then by ID so ties are stable.
fn sort_by_score_desc(risks: &mut [RiskResponse]) {
    risks.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Load one live risk with its vendor, as a full response.
async fn load_risk_response(
    conn: &mut AsyncPgConnection,
    risk_id: &str,
) -> Result<Option<RiskResponse>, diesel::result::Error> {
    let row: Option<(Risk, Vendor)> = risks::table
        .inner_join(vendors::table)
        .filter(risks::id.eq(risk_id))
        .filter(risks::deleted_at.is_null())
        .select((Risk::as_select(), Vendor::as_select()))
        .first(conn)
        .await
        .optional()?;
    match row {
        Some(row) => Ok(build_risk_responses(conn, vec![row]).await?.pop()),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Handlers: risks
// ---------------------------------------------------------------------------

pub async fn list_all(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RisksQuery>,
    _auth: AuthUser,
) -> impl IntoResponse {
    list_internal(&state, None, query).await
}

pub async fn list_by_vendor(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    Query(query): Query<RisksQuery>,
    _auth: AuthUser,
) -> impl IntoResponse {
    list_internal(&state, Some(vendor_id), query).await
}

async fn list_internal(
    state: &Arc<AppState>,
    vendor_id: Option<i32>,
    query: RisksQuery,
) -> Response {
    let status_filter = match query.status.as_deref() {
        None => None,
        Some(label) => match RiskStatus::from_label(label) {
            Some(s) => Some(s.as_str()),
            None => {
                return bad_request(ApiError::validation_error(format!(
                    "Unknown risk status '{label}'"
                )))
            }
        },
    };

    let mut conn = match super::get_conn(state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let mut q = risks::table
        .inner_join(vendors::table)
        .filter(risks::deleted_at.is_null())
        .into_boxed();

    if let Some(vid) = vendor_id.or(query.vendor_id) {
        q = q.filter(risks::vendor_id.eq(vid));
    }
    if let Some(status) = status_filter {
        q = q.filter(risks::status.eq(status));
    }

    let rows: Vec<(Risk, Vendor)> = match q
        .order((risks::vendor_id.asc(), risks::number.asc()))
        .select((Risk::as_select(), Vendor::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(rows) => rows,
        Err(_) => return internal("Failed to fetch risks"),
    };

    match build_risk_responses(&mut conn, rows).await {
        Ok(mut result) => {
            sort_by_score_desc(&mut result);
            Json(result).into_response()
        }
        Err(_) => internal("Failed to fetch risks"),
    }
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    match load_risk_response(&mut conn, &risk_id).await {
        Ok(Some(risk)) => Json(risk).into_response(),
        Ok(None) => risk_not_found(&risk_id),
        Err(_) => internal("Failed to fetch risk"),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    auth: AuthUser,
    Json(payload): Json<CreateRisk>,
) -> impl IntoResponse {
    let item_ids = dedup_ids(&payload.item_ids);

    let validated = (|| -> Result<NewRiskFields, ApiError> {
        validate_scale(payload.severity, "Severity")?;
        validate_scale(payload.likelihood, "Likelihood")?;
        Ok(NewRiskFields {
            component: normalize_optional(payload.component, "Component", COMPONENT_MAX_LEN)?,
            category: normalize_required(&payload.category, "Category", CATEGORY_MAX_LEN)?,
            title: normalize_required(&payload.title, "Risk", TITLE_MAX_LEN)?,
            cause: normalize_optional(payload.cause, "Cause", TEXT_MAX_LEN)?,
            effect: normalize_optional(payload.effect, "Effect", TEXT_MAX_LEN)?,
            mitigation: normalize_optional(payload.mitigation, "Mitigation", TEXT_MAX_LEN)?,
            test_type: normalize_optional(payload.test_type, "Test type", TEST_TYPE_MAX_LEN)?,
            verification: normalize_optional(payload.verification, "Verification", TEXT_MAX_LEN)?,
        })
    })();
    let fields = match validated {
        Ok(f) => f,
        Err(e) => return bad_request(e),
    };

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // Checked before anything is written, so a bad item ID rejects the whole
    // request rather than leaving a risk behind with half its links.
    match missing_live_items(&mut conn, &item_ids).await {
        Ok(missing) if missing.is_empty() => {}
        Ok(missing) => {
            return bad_request(ApiError::validation_error(format!(
                "Unknown action items: {}",
                missing.join(", ")
            )))
        }
        Err(_) => return internal("Failed to verify action items"),
    }

    let vendor: Vendor = match vendors::table
        .filter(vendors::id.eq(vendor_id))
        .first(&mut conn)
        .await
    {
        Ok(v) => v,
        Err(diesel::NotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(format!(
                    "Vendor {} not found",
                    vendor_id
                ))),
            )
                .into_response()
        }
        Err(_) => return internal("Failed to fetch vendor"),
    };

    // One programme-wide sequence: nextval is atomic, so two concurrent
    // creates cannot be handed the same number. If the insert below then
    // fails, the number is simply skipped, which is harmless.
    let number: i64 = match diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
        "nextval('risk_number_seq')",
    ))
    .get_result(&mut conn)
    .await
    {
        Ok(n) => n,
        Err(_) => return internal("Failed to allocate risk number"),
    };
    let number = number as i32;
    let risk_id = format!("RISK-{:03}", number);

    let new_risk = NewRisk {
        id: risk_id,
        vendor_id,
        number,
        component: fields.component,
        category: fields.category,
        title: fields.title,
        cause: fields.cause,
        effect: fields.effect,
        severity: payload.severity,
        likelihood: payload.likelihood,
        mitigation: fields.mitigation,
        test_type: fields.test_type,
        verification: fields.verification,
        status: payload
            .status
            .unwrap_or(RiskStatus::Open)
            .as_str()
            .to_string(),
        trend: payload.trend.unwrap_or(RiskTrend::New).as_str().to_string(),
        opened_on: payload.opened_on.unwrap_or_else(|| Utc::now().date_naive()),
        created_by_id: auth.user_id,
    };

    let risk: Risk = match diesel::insert_into(risks::table)
        .values(&new_risk)
        .returning(Risk::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(r) => r,
        Err(_) => return internal("Failed to create risk"),
    };

    if insert_links(&mut conn, &risk.id, &item_ids, auth.user_id)
        .await
        .is_err()
    {
        return internal("Risk was created but linking its action items failed");
    }

    tracing::info!(
        risk_id = %risk.id,
        user = %auth.email,
        linked_items = item_ids.len(),
        "risk created"
    );

    (
        StatusCode::CREATED,
        Json(build_risk_response(
            risk,
            vendor.prefix,
            auth.name,
            item_ids.len() as i64,
        )),
    )
        .into_response()
}

/// The validated, normalised text fields of a create request.
struct NewRiskFields {
    component: Option<String>,
    category: String,
    title: String,
    cause: Option<String>,
    effect: Option<String>,
    mitigation: Option<String>,
    test_type: Option<String>,
    verification: Option<String>,
}

/// Normalise an optional-text update, keeping the "sent vs not sent"
/// distinction of the outer `Option`.
fn normalize_update(
    raw: Option<Option<String>>,
    field: &str,
    max_len: usize,
) -> Result<Option<Option<String>>, ApiError> {
    match raw {
        None => Ok(None),
        Some(inner) => Ok(Some(normalize_optional(inner, field, max_len)?)),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<UpdateRiskReq>,
) -> impl IntoResponse {
    let validated = (|| -> Result<UpdateRisk, ApiError> {
        if let Some(s) = payload.severity {
            validate_scale(s, "Severity")?;
        }
        if let Some(l) = payload.likelihood {
            validate_scale(l, "Likelihood")?;
        }
        Ok(UpdateRisk {
            component: normalize_update(payload.component, "Component", COMPONENT_MAX_LEN)?,
            category: payload
                .category
                .as_deref()
                .map(|c| normalize_required(c, "Category", CATEGORY_MAX_LEN))
                .transpose()?,
            title: payload
                .title
                .as_deref()
                .map(|t| normalize_required(t, "Risk", TITLE_MAX_LEN))
                .transpose()?,
            cause: normalize_update(payload.cause, "Cause", TEXT_MAX_LEN)?,
            effect: normalize_update(payload.effect, "Effect", TEXT_MAX_LEN)?,
            severity: payload.severity,
            likelihood: payload.likelihood,
            mitigation: normalize_update(payload.mitigation, "Mitigation", TEXT_MAX_LEN)?,
            test_type: normalize_update(payload.test_type, "Test type", TEST_TYPE_MAX_LEN)?,
            verification: normalize_update(payload.verification, "Verification", TEXT_MAX_LEN)?,
            status: payload.status.map(|s| s.as_str().to_string()),
            trend: payload.trend.map(|t| t.as_str().to_string()),
            opened_on: payload.opened_on,
            updated_at: Some(Utc::now()),
        })
    })();
    let changeset = match validated {
        Ok(c) => c,
        Err(e) => return bad_request(e),
    };

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // Filtering on deleted_at is what stops an edit resurrecting a deleted
    // risk from a stale tab.
    let updated = diesel::update(
        risks::table
            .filter(risks::id.eq(&risk_id))
            .filter(risks::deleted_at.is_null()),
    )
    .set(&changeset)
    .execute(&mut conn)
    .await;

    match updated {
        Ok(0) => return risk_not_found(&risk_id),
        Ok(_) => {}
        Err(_) => return internal("Failed to update risk"),
    }

    tracing::info!(risk_id = %risk_id, user = %auth.email, "risk updated");

    match load_risk_response(&mut conn, &risk_id).await {
        Ok(Some(risk)) => Json(risk).into_response(),
        Ok(None) => risk_not_found(&risk_id),
        Err(_) => internal("Failed to fetch risk"),
    }
}

/// Soft-delete a risk. Admin-only, same posture as deleting an action item:
/// the row and its item links stay, the ID stays claimed, and an admin can
/// restore it from the CLI.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    admin: AdminUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let updated = diesel::update(
        risks::table
            .filter(risks::id.eq(&risk_id))
            .filter(risks::deleted_at.is_null()),
    )
    .set((
        risks::deleted_at.eq(Utc::now()),
        risks::deleted_by_id.eq(admin.0.user_id),
    ))
    .execute(&mut conn)
    .await;

    match updated {
        Ok(0) => risk_not_found(&risk_id),
        Ok(_) => {
            tracing::info!(risk_id = %risk_id, admin = %admin.0.email, "risk deleted");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => internal("Failed to delete risk"),
    }
}

// ---------------------------------------------------------------------------
// Handlers: links between items and risks
// ---------------------------------------------------------------------------

/// Link several action items to one risk in a single request. Idempotent:
/// already-linked pairs are skipped, and every ID is checked before any row
/// is written so the request is all-or-nothing.
pub async fn link_items(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<LinkItems>,
) -> impl IntoResponse {
    let item_ids = dedup_ids(&payload.item_ids);
    if item_ids.is_empty() {
        return bad_request(ApiError::validation_error("item_ids must not be empty"));
    }

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_risk_exists(&mut conn, &risk_id).await {
        return resp;
    }

    match missing_live_items(&mut conn, &item_ids).await {
        Ok(missing) if missing.is_empty() => {}
        Ok(missing) => {
            return bad_request(ApiError::validation_error(format!(
                "Unknown action items: {}",
                missing.join(", ")
            )))
        }
        Err(_) => return internal("Failed to verify action items"),
    }

    if insert_links(&mut conn, &risk_id, &item_ids, auth.user_id)
        .await
        .is_err()
    {
        return internal("Failed to link action items");
    }

    tracing::info!(
        risk_id = %risk_id,
        user = %auth.email,
        items = ?item_ids,
        "items linked to risk"
    );

    match load_risk_response(&mut conn, &risk_id).await {
        Ok(Some(risk)) => (StatusCode::CREATED, Json(risk)).into_response(),
        Ok(None) => risk_not_found(&risk_id),
        Err(_) => internal("Failed to fetch risk"),
    }
}

/// Live action items linked to a risk, in the same shape as the item list.
pub async fn items_for_risk(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_risk_exists(&mut conn, &risk_id).await {
        return resp;
    }

    let rows: Vec<(ActionItem, Category)> = match action_items::table
        .inner_join(action_item_risks::table)
        .inner_join(categories::table.on(categories::id.eq(action_items::category_id)))
        .filter(action_item_risks::risk_id.eq(&risk_id))
        .filter(action_items::deleted_at.is_null())
        .order(action_items::id.asc())
        .select((ActionItem::as_select(), Category::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(rows) => rows,
        Err(_) => return internal("Failed to fetch linked items"),
    };

    let user_ids: Vec<i32> = rows
        .iter()
        .flat_map(|(item, _)| [item.created_by_id, item.owner_id])
        .collect();
    let users_list: Vec<User> = users::table
        .filter(users::id.eq_any(&user_ids))
        .load(&mut conn)
        .await
        .unwrap_or_default();
    let users_map: HashMap<i32, &User> = users_list.iter().map(|u| (u.id, u)).collect();

    let mut result = Vec::with_capacity(rows.len());
    for (item, category) in rows {
        let (status, status_changed_at) = super::items::current_status(&mut conn, &item).await;
        let creator = users_map.get(&item.created_by_id).copied();
        let owner = users_map.get(&item.owner_id).copied();
        result.push(super::items::build_item_response(
            item,
            category.name,
            creator,
            owner,
            status,
            status_changed_at,
        ));
    }

    Json(result).into_response()
}

/// Live risks linked to an action item.
pub async fn risks_for_item(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = super::ensure_item_exists(&mut conn, &item_id).await {
        return resp;
    }

    let rows: Vec<(Risk, Vendor)> = match risks::table
        .inner_join(action_item_risks::table)
        .inner_join(vendors::table)
        .filter(action_item_risks::action_item_id.eq(&item_id))
        .filter(risks::deleted_at.is_null())
        .select((Risk::as_select(), Vendor::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(rows) => rows,
        Err(_) => return internal("Failed to fetch linked risks"),
    };

    match build_risk_responses(&mut conn, rows).await {
        Ok(mut result) => {
            sort_by_score_desc(&mut result);
            Json(result).into_response()
        }
        Err(_) => internal("Failed to fetch linked risks"),
    }
}

/// Link an action item to a risk. Idempotent: linking twice is not an error,
/// so two people doing it at once both see success.
pub async fn link(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<LinkRisk>,
) -> impl IntoResponse {
    let risk_id = payload.risk_id.trim().to_string();
    if risk_id.is_empty() {
        return bad_request(ApiError::validation_error("risk_id is required"));
    }

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = super::ensure_item_exists(&mut conn, &item_id).await {
        return resp;
    }
    if let Err(resp) = ensure_risk_exists(&mut conn, &risk_id).await {
        return resp;
    }

    let new_link = NewActionItemRisk {
        action_item_id: item_id.clone(),
        risk_id: risk_id.clone(),
        linked_by_id: auth.user_id,
    };

    if diesel::insert_into(action_item_risks::table)
        .values(&new_link)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .is_err()
    {
        return internal("Failed to link risk");
    }

    tracing::info!(item_id = %item_id, risk_id = %risk_id, user = %auth.email, "risk linked");

    match load_risk_response(&mut conn, &risk_id).await {
        Ok(Some(risk)) => (StatusCode::CREATED, Json(risk)).into_response(),
        Ok(None) => risk_not_found(&risk_id),
        Err(_) => internal("Failed to fetch risk"),
    }
}

pub async fn unlink(
    State(state): State<Arc<AppState>>,
    Path((item_id, risk_id)): Path<(String, String)>,
    auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = super::ensure_item_exists(&mut conn, &item_id).await {
        return resp;
    }

    let deleted = diesel::delete(
        action_item_risks::table
            .filter(action_item_risks::action_item_id.eq(&item_id))
            .filter(action_item_risks::risk_id.eq(&risk_id)),
    )
    .execute(&mut conn)
    .await;

    match deleted {
        Ok(0) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Item {item_id} is not linked to risk {risk_id}"
            ))),
        )
            .into_response(),
        Ok(_) => {
            tracing::info!(item_id = %item_id, risk_id = %risk_id, user = %auth.email, "risk unlinked");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => internal("Failed to unlink risk"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_text_folds_blank_to_none() {
        assert_eq!(
            normalize_optional(Some("  ".into()), "Cause", 10).unwrap(),
            None
        );
        assert_eq!(normalize_optional(None, "Cause", 10).unwrap(), None);
        assert_eq!(
            normalize_optional(Some("  x ".into()), "Cause", 10).unwrap(),
            Some("x".into())
        );
        assert!(normalize_optional(Some("x".repeat(11)), "Cause", 10).is_err());
    }

    #[test]
    fn update_keeps_clear_distinct_from_untouched() {
        // None = not sent, Some(None) = clear, Some(Some) = set.
        assert_eq!(normalize_update(None, "Cause", 10).unwrap(), None);
        assert_eq!(
            normalize_update(Some(None), "Cause", 10).unwrap(),
            Some(None)
        );
        assert_eq!(
            normalize_update(Some(Some("".into())), "Cause", 10).unwrap(),
            Some(None)
        );
        assert_eq!(
            normalize_update(Some(Some("v".into())), "Cause", 10).unwrap(),
            Some(Some("v".into()))
        );
    }

    #[test]
    fn dedup_ids_trims_drops_blanks_and_keeps_first_order() {
        let ids = vec![
            " AD-001 ".to_string(),
            "".to_string(),
            "AD-002".to_string(),
            "AD-001".to_string(),
        ];
        assert_eq!(dedup_ids(&ids), vec!["AD-001", "AD-002"]);
    }

    #[test]
    fn scale_rejects_out_of_range_and_non_finite() {
        assert!(validate_scale(0.0, "Severity").is_ok());
        assert!(validate_scale(2.5, "Severity").is_ok());
        assert!(validate_scale(5.0, "Severity").is_ok());
        assert!(validate_scale(5.01, "Severity").is_err());
        assert!(validate_scale(-0.1, "Severity").is_err());
        assert!(validate_scale(f64::NAN, "Severity").is_err());
        assert!(validate_scale(f64::INFINITY, "Severity").is_err());
    }
}
