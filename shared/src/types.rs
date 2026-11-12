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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vendor {
    pub id: i32,
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub next_number: i32,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorWithCounts {
    #[serde(flatten)]
    pub vendor: Vendor,
    pub open_items: i64,
    pub total_items: i64,
    pub last_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub name: String,
    pub initials: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionItemWithStatus {
    #[serde(flatten)]
    pub item: ActionItem,
    pub status: Status,
    pub status_changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: i32,
    pub action_item_id: String,
    pub note_date: NaiveDate,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateVendor {
    pub prefix: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateVendor {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateActionItem {
    pub title: String,
    pub due_date: Option<NaiveDate>,
    pub category: Category,
    pub owner_id: i32,
    pub priority: Priority,
    pub description: Option<String>,
    pub vendor_tracking_ids: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateActionItem {
    pub title: Option<String>,
    pub due_date: Option<Option<NaiveDate>>,
    pub category: Option<Category>,
    pub owner_id: Option<i32>,
    pub priority: Option<Priority>,
    pub description: Option<Option<String>>,
    pub vendor_tracking_ids: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateNote {
    pub note_date: Option<NaiveDate>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeStatus {
    pub status: Status,
    pub comment: Option<String>,
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    pub user_id: i32,
    pub email: String,
    pub name: String,
    /// Whether this session may perform destructive admin actions (item deletion).
    ///
    /// Derived from the server's `ADMIN_EMAILS` allowlist on every request, not
    /// stored in the token, so the UI reflects a change to the list immediately.
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogoutResponse {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryResponse {
    pub id: i32,
    pub vendor_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionItemResponse {
    pub id: String,
    pub vendor_id: i32,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    /// Free-form references into the vendor's own ticketing system: keys or
    /// URLs, comma- or newline-separated. Stored as typed; the UI splits it.
    pub vendor_tracking_ids: Option<String>,
    pub create_date: NaiveDate,
    pub created_by_id: i32,
    pub created_by_name: String,
    pub created_by_initials: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub category_id: i32,
    pub category: String,
    pub owner_id: i32,
    pub owner_name: String,
    pub owner_initials: Option<String>,
    pub priority: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: String,
    pub status_changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteResponse {
    pub id: i32,
    pub action_item_id: String,
    pub date: NaiveDate,
    pub author_id: i32,
    pub author_name: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteCreateResponse {
    pub id: i32,
    pub action_item_id: String,
    pub note_date: NaiveDate,
    pub author_id: i32,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusHistoryResponse {
    pub id: i32,
    pub action_item_id: String,
    pub status: String,
    pub changed_by_id: i32,
    pub changed_by_name: String,
    pub changed_at: DateTime<Utc>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEventType {
    NoteAdded,
    StatusChanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub item_id: String,
    pub item_title: String,
    pub actor_name: String,
    pub event_type: ActivityEventType,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusChangeResponse {
    pub id: i32,
    pub action_item_id: String,
    pub status: String,
    pub changed_by_id: i32,
    pub changed_at: DateTime<Utc>,
    pub comment: Option<String>,
}

// ============================================================================
// Vendor Portal (read-only, magic-link authenticated)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorPortalLinkRequest {
    pub email: String,
}

/// Deliberately uninformative: the same response is returned whether or not the
/// address matched a configured vendor domain, so the endpoint cannot be used to
/// enumerate which domains are onboarded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorPortalLinkResponse {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorPortalMe {
    pub vendor_id: i32,
    pub vendor_prefix: String,
    pub vendor_name: String,
    pub email: String,
    pub session_expires_at: DateTime<Utc>,
}

/// A single open item as shown to an external vendor contact.
///
/// Intentionally narrower than `ActionItemResponse`: it omits internal user IDs,
/// the creating staff member, and note history. Only the assigned owner's display
/// name is exposed, so the vendor knows who to chase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorPortalItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    /// The vendor's own ticket references, so they can match our item to theirs.
    pub vendor_tracking_ids: Option<String>,
    pub category: String,
    pub priority: String,
    pub status: String,
    pub create_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub status_changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorAllowedDomainResponse {
    pub id: i32,
    pub vendor_id: i32,
    pub domain: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateVendorAllowedDomain {
    pub domain: String,
}

// ============================================================================
// Mail diagnostics
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendTestEmail {
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestEmailResponse {
    pub recipient: String,
    /// Configured transport id: `smtp`, `resend` or `log`.
    pub backend: String,
    /// False for the `log` backend, which accepts the message without sending
    /// it. The UI must say so, or a green result is a lie.
    pub delivered: bool,
}

/// Operator-facing health of the mail transport.
///
/// Deliberately four states, not two. A freshly deployed portal that has never
/// attempted a send is not healthy, it is *untested* — and an unverified sending
/// domain boots cleanly and then rejects every message, so "no evidence" must
/// never be presented as "evidence of success".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailHealth {
    /// The `log` backend: accepts messages and writes them to the log unsent.
    NotDelivering,
    /// A real transport, but nothing has been sent yet.
    Untested,
    /// The most recent attempt failed.
    Failing,
    /// The most recent attempt succeeded.
    Ok,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MailStatus {
    /// Transport id: `smtp`, `resend` or `log`.
    pub backend: String,
    pub health: MailHealth,
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// Verbatim transport error from the last failure, for diagnosis.
    pub last_error: Option<String>,
}

// ============================================================================
// Risk register
// ============================================================================

/// Where a risk is in its life. Stored as the display label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Open,
    InWork,
    Mitigated,
    Accepted,
}

impl RiskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskStatus::Open => "Open",
            RiskStatus::InWork => "In work",
            RiskStatus::Mitigated => "Mitigated",
            RiskStatus::Accepted => "Accepted",
        }
    }

    pub fn all() -> &'static [RiskStatus] {
        &[
            RiskStatus::Open,
            RiskStatus::InWork,
            RiskStatus::Mitigated,
            RiskStatus::Accepted,
        ]
    }

    /// Parse the display label as stored in the database (case-insensitive).
    pub fn from_label(label: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|s| s.as_str().eq_ignore_ascii_case(label.trim()))
    }
}

/// Which way a risk is moving, as judged at review time. `Closed` is a trend
/// rather than a status because that is how the register has always been
/// kept: a mitigated risk stays `Mitigated`, and its trend records that it is
/// no longer being watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTrend {
    New,
    Improving,
    Stable,
    Worsening,
    Closed,
}

impl RiskTrend {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskTrend::New => "New",
            RiskTrend::Improving => "Improving",
            RiskTrend::Stable => "Stable",
            RiskTrend::Worsening => "Worsening",
            RiskTrend::Closed => "Closed",
        }
    }

    pub fn all() -> &'static [RiskTrend] {
        &[
            RiskTrend::New,
            RiskTrend::Improving,
            RiskTrend::Stable,
            RiskTrend::Worsening,
            RiskTrend::Closed,
        ]
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|t| t.as_str().eq_ignore_ascii_case(label.trim()))
    }
}

/// Derived from the score, never stored. See [`risk_level`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    High,
    Medium,
    Low,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::High => "High",
            RiskLevel::Medium => "Medium",
            RiskLevel::Low => "Low",
        }
    }
}

/// Upper bound of the severity and likelihood scales.
pub const RISK_SCALE_MAX: f64 = 5.0;

/// A risk's score is severity times likelihood, each on a 0-5 scale, so the
/// score runs 0-25.
pub fn risk_score(severity: f64, likelihood: f64) -> f64 {
    severity * likelihood
}

/// The register's banding: 15 and above is High, 8 and above is Medium,
/// anything else is Low. The thresholds are the ones the team has always
/// used, so a score of exactly 8 or 15 lands in the upper band.
pub fn risk_level(score: f64) -> RiskLevel {
    if score >= 15.0 {
        RiskLevel::High
    } else if score >= 8.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskResponse {
    /// `RISK-{NNN}`, e.g. `RISK-034`. One programme-wide series, not per
    /// vendor; `vendor_id` says which vendor the risk is about.
    pub id: String,
    pub vendor_id: i32,
    pub vendor_prefix: String,
    pub number: i32,
    /// Module, assembly or programme area the risk is against.
    pub component: Option<String>,
    pub category: String,
    /// The risk statement itself.
    pub title: String,
    pub cause: Option<String>,
    pub effect: Option<String>,
    pub severity: f64,
    pub likelihood: f64,
    /// `severity * likelihood`. Derived, see [`risk_score`].
    pub score: f64,
    /// Derived from `score`, see [`risk_level`].
    pub level: RiskLevel,
    pub mitigation: Option<String>,
    pub test_type: Option<String>,
    /// Which tests, by ID, retire the risk.
    pub verification: Option<String>,
    pub status: RiskStatus,
    pub trend: RiskTrend,
    pub opened_on: NaiveDate,
    pub created_by_id: i32,
    pub created_by_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Live (non-deleted) action items linked to this risk.
    pub linked_item_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateRisk {
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
    /// Defaults to `Open`.
    pub status: Option<RiskStatus>,
    /// Defaults to `New`.
    pub trend: Option<RiskTrend>,
    /// Defaults to today.
    pub opened_on: Option<NaiveDate>,
    /// Action items to link at creation. Every ID must be a live item or the
    /// whole create is rejected, so a typo cannot leave a half-linked risk.
    #[serde(default)]
    pub item_ids: Vec<String>,
}

/// Deserialize a nullable, optional field so that an absent key is `None` and
/// an explicit `null` is `Some(None)`. A plain `Option<Option<T>>` collapses
/// both to `None`, which would make clearing a field indistinguishable from
/// not mentioning it.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

/// Every field optional. For nullable text, the outer `Option` is "was this
/// field sent" and the inner one is the new value, so `Some(None)` clears it
/// and `None` leaves it alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UpdateRisk {
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub component: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub cause: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub effect: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likelihood: Option<f64>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub mitigation: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub test_type: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub verification: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<RiskStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend: Option<RiskTrend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_on: Option<NaiveDate>,
}

/// Body of `POST /api/items/:id/risks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkRisk {
    pub risk_id: String,
}

/// Body of `POST /api/risks/:id/items`: link several action items at once.
/// Idempotent; items already linked are left alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkItems {
    pub item_ids: Vec<String>,
}

#[cfg(test)]
mod risk_tests {
    use super::*;

    #[test]
    fn level_bands_match_the_register_thresholds() {
        // The spreadsheet formula is IF(score>=15,"High",IF(score>=8,"Medium","Low")).
        assert_eq!(risk_level(risk_score(4.0, 4.0)), RiskLevel::High); // 16
        assert_eq!(risk_level(risk_score(5.0, 3.0)), RiskLevel::High); // 15, inclusive
        assert_eq!(risk_level(risk_score(4.0, 3.0)), RiskLevel::Medium); // 12
        assert_eq!(risk_level(risk_score(2.0, 4.0)), RiskLevel::Medium); // 8, inclusive
        assert_eq!(risk_level(risk_score(2.5, 3.0)), RiskLevel::Low); // 7.5
        assert_eq!(risk_level(risk_score(3.0, 0.0)), RiskLevel::Low); // 0
    }

    #[test]
    fn labels_round_trip_case_insensitively() {
        for s in RiskStatus::all() {
            assert_eq!(RiskStatus::from_label(s.as_str()), Some(*s));
            assert_eq!(RiskStatus::from_label(&s.as_str().to_uppercase()), Some(*s));
        }
        for t in RiskTrend::all() {
            assert_eq!(RiskTrend::from_label(t.as_str()), Some(*t));
        }
        assert_eq!(RiskStatus::from_label("in-work"), None);
        assert_eq!(
            RiskStatus::from_label(" in work "),
            Some(RiskStatus::InWork)
        );
    }

    #[test]
    fn update_risk_distinguishes_clear_from_untouched() {
        let cleared: UpdateRisk = serde_json::from_str(r#"{"cause": null}"#).unwrap();
        assert_eq!(cleared.cause, Some(None));
        assert_eq!(cleared.effect, None);

        let untouched = UpdateRisk::default();
        assert_eq!(serde_json::to_string(&untouched).unwrap(), "{}");
    }
}
