use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::Deserialize;
use shared::{ActionItemResponse, ApiError};
use std::sync::Arc;

use crate::db::schema::{action_items, categories, status_history, users, vendors};
use crate::models::{
    ActionItem, Category, NewActionItem, NewStatusHistory, StatusHistory, UpdateActionItem, User,
    Vendor,
};
use crate::AppState;

use super::AuthUser;

#[derive(Debug, Deserialize)]
pub struct ItemsQuery {
    pub vendor_id: Option<i32>,
    pub status: Option<String>,
    pub owner_id: Option<i32>,
    pub category_id: Option<i32>,
    pub priority: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateActionItemReq {
    pub title: String,
    pub due_date: Option<chrono::NaiveDate>,
    pub category_id: i32,
    pub owner_id: i32,
    pub priority: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateActionItemReq {
    pub title: Option<String>,
    pub due_date: Option<Option<chrono::NaiveDate>>,
    pub category_id: Option<i32>,
    pub owner_id: Option<i32>,
    pub priority: Option<String>,
    pub description: Option<Option<String>>,
}

pub async fn list_all(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ItemsQuery>,
    _auth: AuthUser,
) -> impl IntoResponse {
    list_items_internal(&state, None, query).await
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    Query(query): Query<ItemsQuery>,
    _auth: AuthUser,
) -> impl IntoResponse {
    list_items_internal(&state, Some(vendor_id), query).await
}

async fn list_items_internal(
    state: &Arc<AppState>,
    vendor_id: Option<i32>,
    query: ItemsQuery,
) -> Response {
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

    let mut items_query = action_items::table
        .inner_join(categories::table.on(categories::id.eq(action_items::category_id)))
        .into_boxed();

    if let Some(vid) = vendor_id.or(query.vendor_id) {
        items_query = items_query.filter(action_items::vendor_id.eq(vid));
    }

    if let Some(category_id) = query.category_id {
        items_query = items_query.filter(action_items::category_id.eq(category_id));
    }

    if let Some(owner_id) = query.owner_id {
        items_query = items_query.filter(action_items::owner_id.eq(owner_id));
    }

    if let Some(ref priority) = query.priority {
        items_query = items_query.filter(action_items::priority.eq(priority));
    }

    let items: Vec<(ActionItem, Category)> = match items_query
        .order(action_items::id.asc())
        .select((ActionItem::as_select(), Category::as_select()))
        .load(&mut conn)
        .await
    {
        Ok(items) => items,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch items")),
            )
                .into_response()
        }
    };

    // Build a map of user IDs to users for efficient lookup
    let user_ids: Vec<i32> = items
        .iter()
        .flat_map(|(item, _)| vec![item.created_by_id, item.owner_id])
        .collect();

    let users_list: Vec<User> = users::table
        .filter(users::id.eq_any(&user_ids))
        .load(&mut conn)
        .await
        .unwrap_or_default();

    let users_map: std::collections::HashMap<i32, &User> =
        users_list.iter().map(|u| (u.id, u)).collect();

    // Get current status for each item
    let mut result = Vec::new();
    for (item, category) in items {
        let status_entry: Option<StatusHistory> = status_history::table
            .filter(status_history::action_item_id.eq(&item.id))
            .order(status_history::changed_at.desc())
            .first(&mut conn)
            .await
            .ok();

        let (status, status_changed_at) = match status_entry {
            Some(sh) => (sh.status, sh.changed_at),
            None => ("New".to_string(), item.created_at),
        };

        // Filter by status if requested
        if let Some(ref query_status) = query.status {
            if &status != query_status {
                continue;
            }
        }

        let creator = users_map.get(&item.created_by_id);
        let owner = users_map.get(&item.owner_id);

        result.push(ActionItemResponse {
            id: item.id,
            vendor_id: item.vendor_id,
            number: item.number,
            title: item.title,
            description: item.description,
            create_date: item.create_date,
            created_by_id: item.created_by_id,
            created_by_name: creator
                .map(|u| u.name.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            created_by_initials: creator.and_then(|u| u.initials.clone()),
            due_date: item.due_date,
            category_id: item.category_id,
            category: category.name,
            owner_id: item.owner_id,
            owner_name: owner
                .map(|u| u.name.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            owner_initials: owner.and_then(|u| u.initials.clone()),
            priority: item.priority,
            created_at: item.created_at,
            updated_at: item.updated_at,
            status,
            status_changed_at,
        });
    }

    Json(result).into_response()
}

pub async fn get(
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

    let result: (ActionItem, Category) = match action_items::table
        .inner_join(categories::table.on(categories::id.eq(action_items::category_id)))
        .filter(action_items::id.eq(&item_id))
        .select((ActionItem::as_select(), Category::as_select()))
        .first(&mut conn)
        .await
    {
        Ok(r) => r,
        Err(diesel::NotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(format!(
                    "Action item {} not found",
                    item_id
                ))),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch item")),
            )
                .into_response()
        }
    };

    let (item, category) = result;

    // Fetch creator and owner
    let creator: Option<User> = users::table
        .filter(users::id.eq(item.created_by_id))
        .first(&mut conn)
        .await
        .ok();

    let owner: Option<User> = users::table
        .filter(users::id.eq(item.owner_id))
        .first(&mut conn)
        .await
        .ok();

    let status_entry: Option<StatusHistory> = status_history::table
        .filter(status_history::action_item_id.eq(&item.id))
        .order(status_history::changed_at.desc())
        .first(&mut conn)
        .await
        .ok();

    let (status, status_changed_at) = match status_entry {
        Some(sh) => (sh.status, sh.changed_at),
        None => ("New".to_string(), item.created_at),
    };

    Json(ActionItemResponse {
        id: item.id,
        vendor_id: item.vendor_id,
        number: item.number,
        title: item.title,
        description: item.description,
        create_date: item.create_date,
        created_by_id: item.created_by_id,
        created_by_name: creator
            .as_ref()
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        created_by_initials: creator.as_ref().and_then(|u| u.initials.clone()),
        due_date: item.due_date,
        category_id: item.category_id,
        category: category.name,
        owner_id: item.owner_id,
        owner_name: owner
            .as_ref()
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        owner_initials: owner.as_ref().and_then(|u| u.initials.clone()),
        priority: item.priority,
        created_at: item.created_at,
        updated_at: item.updated_at,
        status,
        status_changed_at,
    })
    .into_response()
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Path(vendor_id): Path<i32>,
    auth: AuthUser,
    Json(payload): Json<CreateActionItemReq>,
) -> impl IntoResponse {
    // Validate title
    if payload.title.is_empty() || payload.title.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error("Title must be 1-500 characters")),
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

    // Get vendor and increment next_number
    let vendor: Vendor = match vendors::table
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
                Json(ApiError::internal_error("Failed to fetch vendor")),
            )
                .into_response()
        }
    };

    // Verify category exists
    let category: Category = match categories::table
        .filter(categories::id.eq(payload.category_id))
        .first(&mut conn)
        .await
    {
        Ok(c) => c,
        Err(diesel::NotFound) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::validation_error("Invalid category")),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to verify category")),
            )
                .into_response()
        }
    };

    let number = vendor.next_number;
    let item_id = format!("{}-{:03}", vendor.prefix, number);

    // Update vendor's next_number
    if diesel::update(vendors::table.filter(vendors::id.eq(vendor_id)))
        .set(vendors::next_number.eq(number + 1))
        .execute(&mut conn)
        .await
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error("Failed to update vendor")),
        )
            .into_response();
    }

    let now = Utc::now();
    let new_item = NewActionItem {
        id: item_id.clone(),
        vendor_id,
        number,
        title: payload.title,
        create_date: now.date_naive(),
        created_by_id: auth.user_id,
        due_date: payload.due_date,
        owner_id: payload.owner_id,
        priority: payload.priority,
        description: payload.description,
        category_id: payload.category_id,
    };

    let item: ActionItem = match diesel::insert_into(action_items::table)
        .values(&new_item)
        .returning(ActionItem::as_returning())
        .get_result(&mut conn)
        .await
    {
        Ok(i) => i,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to create item")),
            )
                .into_response()
        }
    };

    // Create initial status entry
    let initial_status = NewStatusHistory {
        action_item_id: item.id.clone(),
        status: "New".to_string(),
        changed_by_id: auth.user_id,
        comment: Some("Item created".to_string()),
    };

    let _ = diesel::insert_into(status_history::table)
        .values(&initial_status)
        .execute(&mut conn)
        .await;

    // Fetch creator name for response
    let creator: Option<User> = users::table
        .filter(users::id.eq(item.created_by_id))
        .first(&mut conn)
        .await
        .ok();

    // Fetch owner name for response
    let owner: Option<User> = users::table
        .filter(users::id.eq(item.owner_id))
        .first(&mut conn)
        .await
        .ok();

    (
        StatusCode::CREATED,
        Json(ActionItemResponse {
            id: item.id,
            vendor_id: item.vendor_id,
            number: item.number,
            title: item.title,
            description: item.description,
            create_date: item.create_date,
            created_by_id: item.created_by_id,
            created_by_name: creator
                .as_ref()
                .map(|u| u.name.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            created_by_initials: creator.as_ref().and_then(|u| u.initials.clone()),
            due_date: item.due_date,
            category_id: item.category_id,
            category: category.name,
            owner_id: item.owner_id,
            owner_name: owner
                .as_ref()
                .map(|u| u.name.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            owner_initials: owner.as_ref().and_then(|u| u.initials.clone()),
            priority: item.priority,
            created_at: item.created_at,
            updated_at: item.updated_at,
            status: "New".to_string(),
            status_changed_at: item.created_at,
        }),
    )
        .into_response()
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
    _auth: AuthUser,
    Json(payload): Json<UpdateActionItemReq>,
) -> impl IntoResponse {
    // Validate title if provided
    if let Some(ref title) = payload.title {
        if title.is_empty() || title.len() > 500 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::validation_error("Title must be 1-500 characters")),
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

    let changeset = UpdateActionItem {
        title: payload.title,
        due_date: payload.due_date,
        category_id: payload.category_id,
        owner_id: payload.owner_id,
        priority: payload.priority,
        description: payload.description,
        updated_at: Some(Utc::now()),
    };

    let item: ActionItem =
        match diesel::update(action_items::table.filter(action_items::id.eq(&item_id)))
            .set(&changeset)
            .returning(ActionItem::as_returning())
            .get_result(&mut conn)
            .await
        {
            Ok(i) => i,
            Err(diesel::NotFound) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError::not_found(format!(
                        "Action item {} not found",
                        item_id
                    ))),
                )
                    .into_response()
            }
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::internal_error("Failed to update item")),
                )
                    .into_response()
            }
        };

    // Get category name
    let category: Category = match categories::table
        .filter(categories::id.eq(item.category_id))
        .first(&mut conn)
        .await
    {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Failed to fetch category")),
            )
                .into_response()
        }
    };

    let status_entry: Option<StatusHistory> = status_history::table
        .filter(status_history::action_item_id.eq(&item.id))
        .order(status_history::changed_at.desc())
        .first(&mut conn)
        .await
        .ok();

    let (status, status_changed_at) = match status_entry {
        Some(sh) => (sh.status, sh.changed_at),
        None => ("New".to_string(), item.created_at),
    };

    // Fetch creator and owner names
    let creator: Option<User> = users::table
        .filter(users::id.eq(item.created_by_id))
        .first(&mut conn)
        .await
        .ok();

    let owner: Option<User> = users::table
        .filter(users::id.eq(item.owner_id))
        .first(&mut conn)
        .await
        .ok();

    Json(ActionItemResponse {
        id: item.id,
        vendor_id: item.vendor_id,
        number: item.number,
        title: item.title,
        description: item.description,
        create_date: item.create_date,
        created_by_id: item.created_by_id,
        created_by_name: creator
            .as_ref()
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        created_by_initials: creator.as_ref().and_then(|u| u.initials.clone()),
        due_date: item.due_date,
        category_id: item.category_id,
        category: category.name,
        owner_id: item.owner_id,
        owner_name: owner
            .as_ref()
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        owner_initials: owner.as_ref().and_then(|u| u.initials.clone()),
        priority: item.priority,
        created_at: item.created_at,
        updated_at: item.updated_at,
        status,
        status_changed_at,
    })
    .into_response()
}

pub async fn go_redirect(Path(item_id): Path<String>) -> Redirect {
    Redirect::to(&format!("/items/{}", item_id))
}
