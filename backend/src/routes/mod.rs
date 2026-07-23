pub mod activity;
pub mod auth;
pub mod categories;
pub mod health;
pub mod items;
pub mod notes;
pub mod status;
pub mod users;
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

const CLEAR_TOKEN_COOKIE: &str = "token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // email
    pub name: String,
    pub user_id: i32,
    pub exp: usize,
    pub iat: usize,
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

        Ok(AuthUser {
            user_id: token_data.claims.user_id,
            email: token_data.claims.sub,
            name: token_data.claims.name,
        })
    }
}
