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
    pub vendor_tracking_ids: Option<String>,
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
    pub vendor_tracking_ids: Option<String>,
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
    pub vendor_tracking_ids: Option<Option<String>>,
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

// ============================================================================
// VendorAllowedDomain
// ============================================================================

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = vendor_allowed_domains)]
pub struct VendorAllowedDomain {
    pub id: i32,
    pub vendor_id: i32,
    pub domain: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = vendor_allowed_domains)]
pub struct NewVendorAllowedDomain {
    pub vendor_id: i32,
    pub domain: String,
}

// ============================================================================
// VendorMagicLink
// ============================================================================

#[derive(Debug, Insertable)]
#[diesel(table_name = vendor_magic_links)]
pub struct NewVendorMagicLink {
    pub vendor_id: i32,
    pub email: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

// ============================================================================
// Risk
// ============================================================================

/// Loaded only via `as_select()` / `as_returning()`, so the soft-delete
/// columns are deliberately absent here, as they are on `ActionItem`.
#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = risks)]
pub struct Risk {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub component: Option<String>,
    pub category: String,
    pub title: String,
    pub cause: Option<String>,
    pub effect: Option<String>,
    pub severity: f64,
    pub likelihood: f64,
    pub mitigation: Option<String>,
    pub test_type: Option<String>,
    pub verification: Option<String>,
    pub status: String,
    pub trend: String,
    pub opened_on: NaiveDate,
    pub created_by_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = risks)]
pub struct NewRisk {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub component: Option<String>,
    pub category: String,
    pub title: String,
    pub cause: Option<String>,
    pub effect: Option<String>,
    pub severity: f64,
    pub likelihood: f64,
    pub mitigation: Option<String>,
    pub test_type: Option<String>,
    pub verification: Option<String>,
    pub status: String,
    pub trend: String,
    pub opened_on: NaiveDate,
    pub created_by_id: i32,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = risks)]
pub struct UpdateRisk {
    pub component: Option<Option<String>>,
    pub category: Option<String>,
    pub title: Option<String>,
    pub cause: Option<Option<String>>,
    pub effect: Option<Option<String>>,
    pub severity: Option<f64>,
    pub likelihood: Option<f64>,
    pub mitigation: Option<Option<String>>,
    pub test_type: Option<Option<String>>,
    pub verification: Option<Option<String>>,
    pub status: Option<String>,
    pub trend: Option<String>,
    pub opened_on: Option<NaiveDate>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ============================================================================
// ActionItemRisk (link table)
// ============================================================================

#[derive(Debug, Insertable)]
#[diesel(table_name = action_item_risks)]
pub struct NewActionItemRisk {
    pub action_item_id: String,
    pub risk_id: String,
    pub linked_by_id: i32,
}
