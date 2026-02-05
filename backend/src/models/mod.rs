use chrono::{DateTime, NaiveDate, Utc};
use diesel::prelude::*;

use crate::db::schema::*;

// ============================================================================
// Category
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = categories)]
pub struct Category {
    pub id: i32,
    pub vendor_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = categories)]
pub struct NewCategory {
    pub vendor_id: i32,
    pub name: String,
    pub description: Option<String>,
}

// ============================================================================
// Vendor
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = vendors)]
pub struct Vendor {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub next_number: i32,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = vendors)]
pub struct NewVendor {
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = vendors)]
pub struct UpdateVendor {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

// ============================================================================
// User
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub name: String,
    pub initials: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub email: String,
    pub name: String,
    pub initials: Option<String>,
}

// ============================================================================
// ActionItem
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = action_items)]
pub struct ActionItem {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub title: String,
    pub create_date: NaiveDate,
    pub created_by_id: i32,
    pub due_date: Option<NaiveDate>,
    pub owner_id: i32,
    pub priority: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub description: Option<String>,
    pub category_id: i32,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = action_items)]
pub struct NewActionItem {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub title: String,
    pub create_date: NaiveDate,
    pub created_by_id: i32,
    pub due_date: Option<NaiveDate>,
    pub owner_id: i32,
    pub priority: String,
    pub description: Option<String>,
    pub category_id: i32,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = action_items)]
pub struct UpdateActionItem {
    pub title: Option<String>,
    pub due_date: Option<Option<NaiveDate>>,
    pub category_id: Option<i32>,
    pub owner_id: Option<i32>,
    pub priority: Option<String>,
    pub description: Option<Option<String>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ============================================================================
// StatusHistory
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = status_history)]
pub struct StatusHistory {
    pub id: i32,
    pub action_item_id: String,
    pub status: String,
    pub changed_by_id: i32,
    pub changed_at: DateTime<Utc>,
    pub comment: Option<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = status_history)]
pub struct NewStatusHistory {
    pub action_item_id: String,
    pub status: String,
    pub changed_by_id: i32,
    pub comment: Option<String>,
}

// ============================================================================
// Note
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = notes)]
pub struct Note {
    pub id: i32,
    pub action_item_id: String,
    pub note_date: NaiveDate,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = notes)]
pub struct NewNote {
    pub action_item_id: String,
    pub note_date: NaiveDate,
    pub author_id: i32,
    pub content: String,
}
