use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::{ApiError, ChangeStatus};
use std::sync::Arc;

use crate::db::schema::{action_items, status_history, users};
use crate::models::{NewStatusHistory, StatusHistory, User};
use crate::AppState;

use super::AuthUser;

const VALID_STATUSES: &[&str] = &[
    "New",
    "Not Started",
    "In Progress",
    "TBC",
    "Complete",
    "Blocked",
];

pub async fn history(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
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

    // Verify item exists
    let item_exists: bool = action_items::table
        .filter(action_items::id.eq(&item_id))
        .count()
        .get_result::<i64>(&mut conn)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

    if !item_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Action item {} not found",
                item_id
            ))),
        )
            .into_response();
    }

    let history: Vec<(StatusHistory, User)> = match status_history::table
        .inner_join(users::table.on(users::id.eq(status_history::changed_by_id)))
        .filter(status_history::action_item_id.eq(&item_id))
        .order(status_history::changed_at.desc())
        .select((StatusHistory::as_select(), User::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(h) => h,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch status history")),
            )
                .into_response()
        }
    };

    let result: Vec<_> = history
        .into_iter()
        .map(|(h, u)| {
            serde_json::json!({
                "id": h.id,
                "action_item_id": h.action_item_id,
                "status": h.status,
                "changed_by_id": h.changed_by_id,
                "changed_by_name": u.name,
                "changed_at": h.changed_at,
                "comment": h.comment,
            })
        })
        .collect();

    Json(result).into_response()
}

pub async fn change(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<ChangeStatus>,
) -> impl IntoResponse {
    let status_str = payload.status.as_str();

    // Validate status
    if !VALID_STATUSES.contains(&status_str) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error(format!(
                "Invalid status. Must be one of: {}",
                VALID_STATUSES.join(", ")
            ))),
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

    // Verify item exists
    let item_exists: bool = action_items::table
        .filter(action_items::id.eq(&item_id))
        .count()
        .get_result::<i64>(&mut conn)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

    if !item_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!(
                "Action item {} not found",
                item_id
            ))),
        )
            .into_response();
    }

    let new_status = NewStatusHistory {
        action_item_id: item_id,
        status: status_str.to_string(),
        changed_by_id: auth.user_id,
        comment: payload.comment,
    };

    let entry: StatusHistory = match diesel::insert_into(status_history::table)
        .values(&new_status)
        .returning(StatusHistory::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(e) => e,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to update status")),
            )
                .into_response()
        }
    };

    // Update the action item's updated_at timestamp
    let _ = diesel::update(action_items::table.filter(action_items::id.eq(&entry.action_item_id)))
        .set(action_items::updated_at.eq(Utc::now()))
        .execute(&mut conn)
        .await;

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": entry.id,
            "action_item_id": entry.action_item_id,
            "status": entry.status,
            "changed_by_id": entry.changed_by_id,
            "changed_at": entry.changed_at,
            "comment": entry.comment,
        })),
    )
        .into_response()
}
