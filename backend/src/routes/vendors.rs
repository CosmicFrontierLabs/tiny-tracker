use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use diesel::prelude::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};
use serde::Deserialize;
use shared::{ApiError, CreateVendor, UpdateVendor as UpdateVendorReq, VendorWithCounts};
use std::sync::Arc;

use crate::db::schema::{action_items, vendors};
use crate::models::Vendor as VendorRow;
use crate::models::{NewVendor, UpdateVendor, Vendor};
use crate::AppState;

use super::AuthUser;

fn to_shared_vendor(v: &Vendor) -> shared::Vendor {
    shared::Vendor {
        id: v.id,
        prefix: v.prefix.clone(),
        name: v.name.clone(),
        description: v.description.clone(),
        next_number: v.next_number,
        created_at: v.created_at,
        archived: v.archived,
    }
}

/// Prefix rules shared by create and rename: 2-5 ASCII uppercase letters.
fn validate_prefix(prefix: &str) -> Result<(), ApiError> {
    if prefix.len() < 2 || prefix.len() > 5 {
        return Err(ApiError::validation_error("Prefix must be 2-5 characters"));
    }
    if !prefix.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(ApiError::validation_error(
            "Prefix must be uppercase letters only",
        ));
    }
    Ok(())
}

/// Outcome of a vendor update, including a prefix rename if one happened.
enum UpdateOutcome {
    Updated(VendorRow),
    NotFound,
    PrefixTaken,
}

/// Apply the changeset and, if the prefix is changing, rewrite every item ID
/// for the vendor in the same transaction. The child tables (notes, status
/// history, risk links) follow via ON UPDATE CASCADE, so either the vendor,
/// its items and everything hanging off them all rename, or nothing does.
async fn apply_update(
    conn: &mut diesel_async::AsyncPgConnection,
    id: i32,
    changeset: UpdateVendor,
    new_prefix: Option<String>,
) -> Result<UpdateOutcome, diesel::result::Error> {
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        async move {
            let current: Option<VendorRow> = vendors::table
                .filter(vendors::id.eq(id))
                .first(conn)
                .await
                .optional()?;
            let Some(current) = current else {
                return Ok(UpdateOutcome::NotFound);
            };

            if let Some(new_prefix) = new_prefix.filter(|p| *p != current.prefix) {
                let taken = diesel::update(vendors::table.filter(vendors::id.eq(id)))
                    .set(vendors::prefix.eq(&new_prefix))
                    .execute(conn)
                    .await;
                match taken {
                    Ok(_) => {}
                    Err(diesel::result::Error::DatabaseError(
                        diesel::result::DatabaseErrorKind::UniqueViolation,
                        _,
                    )) => return Ok(UpdateOutcome::PrefixTaken),
                    Err(e) => return Err(e),
                }

                // Keep whatever follows the old prefix ("-001") verbatim rather
                // than re-deriving it from `number`, so imported IDs with
                // unusual padding survive the rename unchanged apart from the
                // prefix. substr() is 1-based: position len+1 is the hyphen.
                let rewritten = diesel::sql_query(
                    "UPDATE action_items SET id = $1 || substr(id, $2) WHERE vendor_id = $3",
                )
                .bind::<diesel::sql_types::Text, _>(&new_prefix)
                .bind::<diesel::sql_types::Integer, _>(current.prefix.len() as i32 + 1)
                .bind::<diesel::sql_types::Integer, _>(id)
                .execute(conn)
                .await?;

                tracing::info!(
                    vendor_id = id,
                    from = %current.prefix,
                    to = %new_prefix,
                    items = rewritten,
                    "vendor prefix renamed"
                );
            }

            // Diesel rejects an UPDATE with an empty changeset, and a
            // prefix-only request has exactly that, so only apply it when
            // there is something in it.
            let has_changes = changeset.name.is_some()
                || changeset.description.is_some()
                || changeset.archived.is_some();
            let updated: VendorRow = if has_changes {
                diesel::update(vendors::table.filter(vendors::id.eq(id)))
                    .set(&changeset)
                    .returning(VendorRow::as_returning())
                    .get_result(conn)
                    .await?
            } else {
                vendors::table
                    .filter(vendors::id.eq(id))
                    .first(conn)
                    .await?
            };
            Ok(UpdateOutcome::Updated(updated))
        }
        .scope_boxed()
    })
    .await
}

#[derive(Deserialize)]
pub struct ListVendorsParams {
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListVendorsParams>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let mut query = vendors::table.order(vendors::prefix.asc()).into_boxed();
    if !params.include_archived {
        query = query.filter(vendors::archived.eq(false));
    }
    let vendors_result: Result<Vec<Vendor>, _> = query.load(&mut conn).await;

    let all_vendors = match vendors_result {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch vendors")),
            )
                .into_response()
        }
    };

    // Build response with counts
    let mut result = Vec::new();
    for vendor in &all_vendors {
        // Get total items count
        let total: i64 = action_items::table
            .filter(action_items::vendor_id.eq(vendor.id))
            .filter(action_items::deleted_at.is_null())
            .count()
            .get_result(&mut conn)
            .await
            .unwrap_or(0);

        // Get open items count - simplified for now
        let open = total; // TODO: Implement proper status filtering

        result.push(VendorWithCounts {
            vendor: to_shared_vendor(vendor),
            open_items: open,
            total_items: total,
            last_updated: None,
        });
    }

    Json(result).into_response()
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let vendor: Result<Vendor, _> = vendors::table
        .filter(vendors::id.eq(id))
        .first(&mut conn)
        .await;

    match vendor {
        Ok(v) => Json(to_shared_vendor(&v)).into_response(),
        Err(diesel::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Vendor {} not found", id))),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Failed to fetch vendor")),
        )
            .into_response(),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(payload): Json<CreateVendor>,
) -> impl IntoResponse {
    if let Err(e) = validate_prefix(&payload.prefix) {
        return (StatusCode::BAD_REQUEST, Json(e)).into_response();
    }

    // Validate name
    if payload.name.is_empty() || payload.name.len() > 255 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error("Name must be 1-255 characters")),
        )
            .into_response();
    }

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let new_vendor = NewVendor {
        prefix: payload.prefix.clone(),
        name: payload.name,
        description: payload.description,
    };

    let result: Result<Vendor, _> = diesel::insert_into(vendors::table)
        .values(&new_vendor)
        .returning(Vendor::as_returning())
        .get_result(&mut conn)
        .await;

    match result {
        Ok(v) => (StatusCode::CREATED, Json(to_shared_vendor(&v))).into_response(),
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        )) => (
            StatusCode::CONFLICT,
            Json(ApiError::conflict(format!(
                "Vendor with prefix '{}' already exists",
                payload.prefix
            ))),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Failed to create vendor")),
        )
            .into_response(),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    auth: AuthUser,
    Json(payload): Json<UpdateVendorReq>,
) -> impl IntoResponse {
    // Validate name if provided
    if let Some(ref name) = payload.name {
        if name.is_empty() || name.len() > 255 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::validation_error("Name must be 1-255 characters")),
            )
                .into_response();
        }
    }

    let new_prefix = payload.prefix.map(|p| p.trim().to_uppercase());
    if let Some(ref prefix) = new_prefix {
        if let Err(e) = validate_prefix(prefix) {
            return (StatusCode::BAD_REQUEST, Json(e)).into_response();
        }
    }

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let changeset = UpdateVendor {
        name: payload.name,
        description: payload.description,
        archived: payload.archived,
    };

    match apply_update(&mut conn, id, changeset, new_prefix.clone()).await {
        Ok(UpdateOutcome::Updated(v)) => {
            if new_prefix.is_some() {
                tracing::info!(vendor_id = id, user = %auth.email, "vendor updated with prefix change");
            }
            Json(to_shared_vendor(&v)).into_response()
        }
        Ok(UpdateOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Vendor {} not found", id))),
        )
            .into_response(),
        Ok(UpdateOutcome::PrefixTaken) => (
            StatusCode::CONFLICT,
            Json(ApiError::conflict(format!(
                "Vendor with prefix '{}' already exists",
                new_prefix.unwrap_or_default()
            ))),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Failed to update vendor")),
        )
            .into_response(),
    }
}
