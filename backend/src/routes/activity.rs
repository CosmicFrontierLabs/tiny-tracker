use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::sql_types::{Timestamptz, Varchar};
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use shared::{ActivityEntry, ActivityEventType, ApiError};
use std::sync::Arc;

use super::AuthUser;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub since: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, QueryableByName)]
struct RawActivityRow {
    #[diesel(sql_type = Timestamptz)]
    timestamp: DateTime<Utc>,
    #[diesel(sql_type = Varchar)]
    item_id: String,
    #[diesel(sql_type = Varchar)]
    item_title: String,
    #[diesel(sql_type = Varchar)]
    actor_name: String,
    #[diesel(sql_type = Varchar)]
    event_type: String,
    #[diesel(sql_type = Varchar)]
    detail: String,
}

pub async fn activity(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
    auth: AuthUser,
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

    let since: DateTime<Utc> = query
        .since
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap());

    let limit = query.limit.unwrap_or(50).min(200);

    let sql = r#"
        (
            SELECT
                n.created_at AS timestamp,
                n.action_item_id AS item_id,
                ai.title AS item_title,
                u.name AS actor_name,
                'note_added' AS event_type,
                LEFT(n.content, 120) AS detail
            FROM notes n
            INNER JOIN users u ON u.id = n.author_id
            INNER JOIN action_items ai ON ai.id = n.action_item_id
            WHERE n.author_id != $1
              AND n.created_at > $2
        )
        UNION ALL
        (
            SELECT
                sh.changed_at AS timestamp,
                sh.action_item_id AS item_id,
                ai.title AS item_title,
                u.name AS actor_name,
                'status_changed' AS event_type,
                sh.status AS detail
            FROM status_history sh
            INNER JOIN users u ON u.id = sh.changed_by_id
            INNER JOIN action_items ai ON ai.id = sh.action_item_id
            WHERE sh.changed_by_id != $1
              AND sh.changed_at > $2
        )
        ORDER BY timestamp DESC
        LIMIT $3
    "#;

    let rows: Vec<RawActivityRow> = match diesel::sql_query(sql)
        .bind::<diesel::sql_types::Int4, _>(auth.user_id)
        .bind::<Timestamptz, _>(since)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .load(&mut conn)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Activity query failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to load activity")),
            )
                .into_response();
        }
    };

    let entries: Vec<ActivityEntry> = rows
        .into_iter()
        .map(|row| {
            let event_type = match row.event_type.as_str() {
                "status_changed" => ActivityEventType::StatusChanged,
                _ => ActivityEventType::NoteAdded,
            };
            let detail = match &event_type {
                ActivityEventType::StatusChanged => format!("→ {}", row.detail),
                ActivityEventType::NoteAdded => {
                    if row.detail.len() >= 120 {
                        format!("{}...", row.detail)
                    } else {
                        row.detail
                    }
                }
            };
            ActivityEntry {
                timestamp: row.timestamp,
                item_id: row.item_id,
                item_title: row.item_title,
                actor_name: row.actor_name,
                event_type,
                detail,
            }
        })
        .collect();

    Json(entries).into_response()
}
