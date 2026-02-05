use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Programmatic,
    SwOps,
    Mechanical,
    Adcs,
    Systems,
    ConOps,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Programmatic => "Programmatic",
            Category::SwOps => "SW / Ops",
            Category::Mechanical => "Mechanical",
            Category::Adcs => "ADCS",
            Category::Systems => "Systems",
            Category::ConOps => "ConOps",
        }
    }

    pub fn all() -> &'static [Category] {
        &[
            Category::Programmatic,
            Category::SwOps,
            Category::Mechanical,
            Category::Adcs,
            Category::Systems,
            Category::ConOps,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
        }
    }

    pub fn all() -> &'static [Priority] {
        &[Priority::High, Priority::Medium, Priority::Low]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    New,
    NotStarted,
    InProgress,
    Tbc,
    Complete,
    Blocked,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::New => "New",
            Status::NotStarted => "Not Started",
            Status::InProgress => "In Progress",
            Status::Tbc => "TBC",
            Status::Complete => "Complete",
            Status::Blocked => "Blocked",
        }
    }

    pub fn all() -> &'static [Status] {
        &[
            Status::New,
            Status::NotStarted,
            Status::InProgress,
            Status::Tbc,
            Status::Complete,
            Status::Blocked,
        ]
    }
}

// ============================================================================
// Domain Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vendor {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub next_number: i32,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorWithCounts {
    #[serde(flatten)]
    pub vendor: Vendor,
    pub open_items: i64,
    pub total_items: i64,
    pub last_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub name: String,
    pub initials: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub title: String,
    pub create_date: NaiveDate,
    pub created_by_id: i32,
    pub due_date: Option<NaiveDate>,
    pub category: Category,
    pub owner_id: i32,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemWithStatus {
    #[serde(flatten)]
    pub item: ActionItem,
    pub status: Status,
    pub status_changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: i32,
    pub action_item_id: String,
    pub note_date: NaiveDate,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusHistory {
    pub id: i32,
    pub action_item_id: String,
    pub status: Status,
    pub changed_by_id: i32,
    pub changed_at: DateTime<Utc>,
    pub comment: Option<String>,
}

// ============================================================================
// API Request Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVendor {
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVendor {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActionItem {
    pub title: String,
    pub due_date: Option<NaiveDate>,
    pub category: Category,
    pub owner_id: i32,
    pub priority: Priority,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateActionItem {
    pub title: Option<String>,
    pub due_date: Option<Option<NaiveDate>>,
    pub category: Option<Category>,
    pub owner_id: Option<i32>,
    pub priority: Option<Priority>,
    pub description: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNote {
    pub note_date: Option<NaiveDate>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeStatus {
    pub status: Status,
    pub comment: Option<String>,
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message)
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::new("VALIDATION_ERROR", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new("UNAUTHORIZED", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new("FORBIDDEN", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("CONFLICT", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}
