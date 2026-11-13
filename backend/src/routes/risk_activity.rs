//! Notes and change history on risks.
//!
//! Mirrors the action item notes and status history routes. History rows are
//! written by the risk create and update handlers through [`record_history`];
//! this module only reads them back. Both tables are reached through
//! `ensure_risk_exists`, so a deleted risk's timeline is a 404 like the risk.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use shared::{ApiError, CreateRiskNote, RiskHistoryField, RiskHistoryResponse, RiskNoteResponse};
use std::sync::Arc;

use crate::db::schema::{risk_history, risk_notes, risks, users};
use crate::models::{NewRiskHistory, NewRiskNote, RiskHistory, RiskNote, User};
use crate::AppState;

use super::AuthUser;

const NOTE_MAX_LEN: usize = 10_000;

fn internal(msg: &'static str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::internal_error(msg)),
    )
        .into_response()
}

#[allow(clippy::result_large_err)] // see note on `get_conn` in routes/mod.rs
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
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(format!("Risk {} not found", risk_id))),
        )
            .into_response())
    }
}

/// Write one history row per changed field, all stamped with the same
/// instant so the UI can fold them into a single event.
pub(super) async fn record_history(
    conn: &mut AsyncPgConnection,
    risk_id: &str,
    user_id: i32,
    changes: &[(RiskHistoryField, Option<String>, Option<String>)],
    comment: Option<String>,
) -> Result<usize, diesel::result::Error> {
    if changes.is_empty() {
        return Ok(0);
    }
    let now = Utc::now();
    let rows: Vec<NewRiskHistory> = changes
        .iter()
        .map(|(field, old, new)| NewRiskHistory {
            risk_id: risk_id.to_string(),
            changed_by_id: user_id,
            changed_at: now,
            field: field.as_str().to_string(),
            old_value: old.clone(),
            new_value: new.clone(),
            comment: comment.clone(),
        })
        .collect();
    diesel::insert_into(risk_history::table)
        .values(&rows)
        .execute(conn)
        .await
}

pub async fn list_notes(
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

    let rows: Vec<(RiskNote, User)> = match risk_notes::table
        .inner_join(users::table.on(users::id.eq(risk_notes::author_id)))
        .filter(risk_notes::risk_id.eq(&risk_id))
        .order((risk_notes::created_at.desc(), risk_notes::id.desc()))
        .select((RiskNote::as_select(), User::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(r) => r,
        Err(_) => return internal("Failed to fetch risk notes"),
    };

    let result: Vec<RiskNoteResponse> = rows
        .into_iter()
        .map(|(n, u)| RiskNoteResponse {
            id: n.id,
            risk_id: n.risk_id,
            note_date: n.note_date,
            author_id: n.author_id,
            author_name: u.name,
            content: n.content,
            created_at: n.created_at,
        })
        .collect();

    Json(result).into_response()
}

pub async fn create_note(
    State(state): State<Arc<AppState>>,
    Path(risk_id): Path<String>,
    auth: AuthUser,
    Json(payload): Json<CreateRiskNote>,
) -> impl IntoResponse {
    let content = payload.content.trim().to_string();
    if content.is_empty() || content.chars().count() > NOTE_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error(format!(
                "Content must be 1-{NOTE_MAX_LEN} characters"
            ))),
        )
            .into_response();
    }

    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_risk_exists(&mut conn, &risk_id).await {
        return resp;
    }

    let new_note = NewRiskNote {
        risk_id: risk_id.clone(),
        note_date: payload.note_date.unwrap_or_else(|| Utc::now().date_naive()),
        author_id: auth.user_id,
        content,
    };

    let note: RiskNote = match diesel::insert_into(risk_notes::table)
        .values(&new_note)
        .returning(RiskNote::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(n) => n,
        Err(_) => return internal("Failed to create note"),
    };

    // A note is activity on the risk, so it counts as an update.
    let _ = diesel::update(risks::table.filter(risks::id.eq(&risk_id)))
        .set(risks::updated_at.eq(Utc::now()))
        .execute(&mut conn)
        .await;

    (
        StatusCode::CREATED,
        Json(RiskNoteResponse {
            id: note.id,
            risk_id: note.risk_id,
            note_date: note.note_date,
            author_id: note.author_id,
            author_name: auth.name,
            content: note.content,
            created_at: note.created_at,
        }),
    )
        .into_response()
}

pub async fn history(
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

    let rows: Vec<(RiskHistory, User)> = match risk_history::table
        .inner_join(users::table.on(users::id.eq(risk_history::changed_by_id)))
        .filter(risk_history::risk_id.eq(&risk_id))
        .order((risk_history::changed_at.desc(), risk_history::id.desc()))
        .select((RiskHistory::as_select(), User::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(r) => r,
        Err(_) => return internal("Failed to fetch risk history"),
    };

    let result: Vec<RiskHistoryResponse> = rows
        .into_iter()
        .map(|(h, u)| RiskHistoryResponse {
            id: h.id,
            risk_id: h.risk_id,
            // The CHECK constraint guarantees a known label; the fallback
            // satisfies the type system, not the data.
            field: RiskHistoryField::from_label(&h.field).unwrap_or(RiskHistoryField::Created),
            old_value: h.old_value,
            new_value: h.new_value,
            changed_by_id: h.changed_by_id,
            changed_by_name: u.name,
            changed_at: h.changed_at,
            comment: h.comment,
        })
        .collect();

    Json(result).into_response()
}
