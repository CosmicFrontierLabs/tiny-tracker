use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::{ApiError, CreateVendor, UpdateVendor as UpdateVendorReq};
use std::sync::Arc;

use crate::db::schema::{action_items, vendors};
use crate::models::{NewVendor, UpdateVendor, Vendor};
use crate::AppState;

use super::AuthUser;

pub async fn list(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
    let mut conn = match state.pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Database connection failed")),
            )
                .into_response()
        }
    };

    let vendors_result: Result<Vec<Vendor>, _> = vendors::table
        .order(vendors::prefix.asc())
        .load(&mut conn)
        .await;

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
    for vendor in all_vendors {
        // Get total items count
        let total: i64 = action_items::table
            .filter(action_items::vendor_id.eq(vendor.id))
            .count()
            .get_result(&mut conn)
            .await
            .unwrap_or(0);

        // Get open items count - simplified for now
        let open = total; // TODO: Implement proper status filtering

        result.push(serde_json::json!({
            "id": vendor.id,
            "prefix": vendor.prefix,
            "name": vendor.name,
            "description": vendor.description,
            "next_number": vendor.next_number,
            "created_at": vendor.created_at,
            "open_items": open,
            "total_items": total,
        }));
    }

    Json(result).into_response()
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match state.pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Database connection failed")),
            )
                .into_response()
        }
    };

    let vendor: Result<Vendor, _> = vendors::table
        .filter(vendors::id.eq(id))
        .first(&mut conn)
        .await;

    match vendor {
        Ok(v) => Json(serde_json::json!({
            "id": v.id,
            "prefix": v.prefix,
            "name": v.name,
            "description": v.description,
            "next_number": v.next_number,
            "created_at": v.created_at,
        }))
        .into_response(),
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
    // Validate prefix
    if payload.prefix.len() < 2 || payload.prefix.len() > 5 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error("Prefix must be 2-5 characters")),
        )
            .into_response();
    }
    if !payload.prefix.chars().all(|c| c.is_ascii_uppercase()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error(
                "Prefix must be uppercase letters only",
            )),
        )
            .into_response();
    }

    // Validate name
    if payload.name.is_empty() || payload.name.len() > 255 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error("Name must be 1-255 characters")),
        )
            .into_response();
    }

    let mut conn = match state.pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Database connection failed")),
            )
                .into_response()
        }
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
        Ok(v) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": v.id,
                "prefix": v.prefix,
                "name": v.name,
                "description": v.description,
                "next_number": v.next_number,
                "created_at": v.created_at,
            })),
        )
            .into_response(),
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
    _auth: AuthUser,
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

    let mut conn = match state.pool.get().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Database connection failed")),
            )
                .into_response()
        }
    };

    let changeset = UpdateVendor {
        name: payload.name,
        description: payload.description,
    };

    let result: Result<Vendor, _> = diesel::update(vendors::table.filter(vendors::id.eq(id)))
        .set(&changeset)
        .returning(Vendor::as_returning())
        .get_result(&mut conn)
        .await;

    match result {
        Ok(v) => Json(serde_json::json!({
            "id": v.id,
            "prefix": v.prefix,
            "name": v.name,
            "description": v.description,
            "next_number": v.next_number,
            "created_at": v.created_at,
        }))
        .into_response(),
        Err(diesel::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Vendor {} not found", id))),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Failed to update vendor")),
        )
            .into_response(),
    }
}
