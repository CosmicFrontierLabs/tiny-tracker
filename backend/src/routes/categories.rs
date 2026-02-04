use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use shared::ApiError;
use std::sync::Arc;

use crate::db::schema::{categories, vendors};
use crate::models::{Category, NewCategory, Vendor};
use crate::AppState;

use super::AuthUser;

#[derive(Debug, Deserialize)]
pub struct CreateCategoryReq {
    pub name: String,
    pub description: Option<String>,
}

pub async fn list_all(State(state): State<Arc<AppState>>, _auth: AuthUser) -> impl IntoResponse {
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

    let cats: Vec<Category> = match categories::table
        .order((categories::vendor_id.asc(), categories::name.asc()))
        .load(&mut conn)
        .await
    {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch categories")),
            )
                .into_response()
        }
    };

    let result: Vec<_> = cats
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "vendor_id": c.vendor_id,
                "name": c.name,
                "description": c.description,
                "created_at": c.created_at,
            })
        })
        .collect();

    Json(result).into_response()
}

pub async fn list_by_vendor(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
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

    let cats: Vec<Category> = match categories::table
        .filter(categories::vendor_id.eq(vendor_id))
        .order(categories::name.asc())
        .load(&mut conn)
        .await
    {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch categories")),
            )
                .into_response()
        }
    };

    let result: Vec<_> = cats
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "vendor_id": c.vendor_id,
                "name": c.name,
                "description": c.description,
                "created_at": c.created_at,
            })
        })
        .collect();

    Json(result).into_response()
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    _auth: AuthUser,
    Json(payload): Json<CreateCategoryReq>,
) -> impl IntoResponse {
    // Validate name
    if payload.name.is_empty() || payload.name.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error("Name must be 1-100 characters")),
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

    // Verify vendor exists
    let _vendor: Vendor = match vendors::table
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
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to verify vendor")),
            )
                .into_response()
        }
    };

    let new_category = NewCategory {
        vendor_id,
        name: payload.name,
        description: payload.description,
    };

    let category: Category = match diesel::insert_into(categories::table)
        .values(&new_category)
        .returning(Category::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(c) => c,
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        )) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError::conflict(
                    "Category with this name already exists for this vendor",
                )),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to create category")),
            )
                .into_response()
        }
    };

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": category.id,
            "vendor_id": category.vendor_id,
            "name": category.name,
            "description": category.description,
            "created_at": category.created_at,
        })),
    )
        .into_response()
}
