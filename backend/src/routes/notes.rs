use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use shared::{ApiError, CreateNote};
use std::sync::Arc;

use crate::db::schema::{action_items, notes, users};
use crate::models::{NewNote, Note, UpdateActionItem, User};
use crate::AppState;

use super::AuthUser;

pub async fn list(
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

    // Join with users to get author name
    let updates_result: Vec<(Note, User)> = match notes::table
        .inner_join(users::table.on(users::id.eq(notes::author_id)))
        .filter(notes::action_item_id.eq(&item_id))
        .order((notes::note_date.desc(), notes::created_at.desc()))
        .select((Note::as_select(), User::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(n) => n,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch updates")),
            )
                .into_response()
        }
    };

    let result: Vec<_> = updates_result
        .into_iter()
        .map(|(n, u)| {
            serde_json::json!({
                "id": n.id,
                "action_item_id": n.action_item_id,
                "date": n.note_date,
                "author_id": n.author_id,
                "author_name": u.name,
                "content": n.content,
                "created_at": n.created_at,
            })
        })
        .collect();

    Json(result).into_response()
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<CreateNote>,
) -> impl IntoResponse {
    // Validate content
    if payload.content.is_empty() || payload.content.len() > 10000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error(
                "Content must be 1-10000 characters",
            )),
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

    let note_date = payload.note_date.unwrap_or_else(|| Utc::now().date_naive());

    let new_note = NewNote {
        action_item_id: item_id,
        note_date,
        author_id: auth.user_id,
        content: payload.content,
    };

    let note: Note = match diesel::insert_into(notes::table)
        .values(&new_note)
        .returning(Note::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(n) => n,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to create note")),
            )
                .into_response()
        }
    };

    // Update the action item's updated_at timestamp
    let update_changeset = UpdateActionItem {
        title: None,
        due_date: None,
        category_id: None,
        owner_id: None,
        priority: None,
        description: None,
        updated_at: Some(Utc::now()),
    };
    let _ = diesel::update(action_items::table.filter(action_items::id.eq(&note.action_item_id)))
        .set(&update_changeset)
        .execute(&mut conn)
        .await;

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": note.id,
            "action_item_id": note.action_item_id,
            "note_date": note.note_date,
            "author_id": note.author_id,
            "content": note.content,
            "created_at": note.created_at,
        })),
    )
        .into_response()
}
