use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::ApiError;
use std::sync::Arc;

use crate::db::schema::users;
use crate::models::User;
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

    let all_users: Vec<User> = match users::table.order(users::name.asc()).load(&mut conn).await {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch users")),
            )
                .into_response()
        }
    };

    let result: Vec<_> = all_users
        .into_iter()
        .map(|u| {
            serde_json::json!({
                "id": u.id,
                "email": u.email,
                "name": u.name,
                "initials": u.initials,
                "created_at": u.created_at,
            })
        })
        .collect();

    Json(result).into_response()
}
