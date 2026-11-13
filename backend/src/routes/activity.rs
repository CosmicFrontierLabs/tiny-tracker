// clippy 1.99 flags redundant field names inside diesel's QueryableByName
// derive expansion; the lint lands on the generated impl, so a struct-level
// allow does not reach it.
#![allow(clippy::redundant_field_names)]

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
use shared::{ActivityEntry, ActivityEventType, ActivityTarget, ApiError};
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
    #[diesel(sql_type = Varchar)]
    target: String,
}

pub async fn activity(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
    auth: AuthUser,
) -> impl IntoResponse {
    let mut conn = match super::get_conn(&state).await {
        Ok(c) => c,
        Err(resp) => return resp,
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
                LEFT(n.content, 120) AS detail,
                'item' AS target
            FROM notes n
            INNER JOIN users u ON u.id = n.author_id
            INNER JOIN action_items ai ON ai.id = n.action_item_id
            WHERE n.author_id != $1
              AND n.created_at > $2
              AND ai.deleted_at IS NULL
        )
        UNION ALL
        (
            SELECT
                sh.changed_at AS timestamp,
                sh.action_item_id AS item_id,
                ai.title AS item_title,
                u.name AS actor_name,
                'status_changed' AS event_type,
                sh.status AS detail,
                'item' AS target
            FROM status_history sh
            INNER JOIN users u ON u.id = sh.changed_by_id
            INNER JOIN action_items ai ON ai.id = sh.action_item_id
            WHERE sh.changed_by_id != $1
              AND sh.changed_at > $2
              AND ai.deleted_at IS NULL
        )
        UNION ALL
        (
            SELECT
                rn.created_at AS timestamp,
                rn.risk_id AS item_id,
                r.title AS item_title,
                u.name AS actor_name,
                'risk_note_added' AS event_type,
                LEFT(rn.content, 120) AS detail,
                'risk' AS target
            FROM risk_notes rn
            INNER JOIN users u ON u.id = rn.author_id
            INNER JOIN risks r ON r.id = rn.risk_id
            WHERE rn.author_id != $1
              AND rn.created_at > $2
              AND r.deleted_at IS NULL
        )
        UNION ALL
        (
            SELECT
                rh.changed_at AS timestamp,
                rh.risk_id AS item_id,
                r.title AS item_title,
                u.name AS actor_name,
                'risk_changed' AS event_type,
                rh.field || '|' || COALESCE(rh.old_value, '') || '|' || COALESCE(rh.new_value, '') AS detail,
                'risk' AS target
            FROM risk_history rh
            INNER JOIN users u ON u.id = rh.changed_by_id
            INNER JOIN risks r ON r.id = rh.risk_id
            WHERE rh.changed_by_id != $1
              AND rh.changed_at > $2
              AND r.deleted_at IS NULL
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
                "risk_note_added" => ActivityEventType::RiskNoteAdded,
                "risk_changed" => ActivityEventType::RiskChanged,
                _ => ActivityEventType::NoteAdded,
            };
            let target = match row.target.as_str() {
                "risk" => ActivityTarget::Risk,
                _ => ActivityTarget::Item,
            };
            let detail = match &event_type {
                ActivityEventType::StatusChanged => format!("→ {}", row.detail),
                ActivityEventType::RiskChanged => describe_risk_change(&row.detail),
                ActivityEventType::NoteAdded | ActivityEventType::RiskNoteAdded => {
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
                target,
            }
        })
        .collect();

    Json(entries).into_response()
}

/// Turn the `field|old|new` encoding from the risk_history branch of the
/// UNION into a one-line description: "severity 4 → 2.5", or "opened".
fn describe_risk_change(encoded: &str) -> String {
    let mut parts = encoded.splitn(3, '|');
    let field = parts.next().unwrap_or("");
    let old = parts.next().unwrap_or("");
    let new = parts.next().unwrap_or("");
    match field {
        "created" => "opened".to_string(),
        _ if old.is_empty() => format!("{field} → {new}"),
        _ => format!("{field} {old} → {new}"),
    }
}

#[cfg(test)]
mod tests {
    use super::describe_risk_change;

    #[test]
    fn risk_changes_read_as_one_line() {
        assert_eq!(describe_risk_change("severity|4|2.5"), "severity 4 → 2.5");
        assert_eq!(
            describe_risk_change("status|Open|In work"),
            "status Open → In work"
        );
        assert_eq!(describe_risk_change("created||"), "opened");
        // A value containing the separator is cut, never panicked on.
        assert_eq!(describe_risk_change("trend|a|b|c"), "trend a → b|c");
    }
}
