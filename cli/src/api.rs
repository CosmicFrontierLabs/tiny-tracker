//! Commands that talk to a running server over HTTP.
//!
//! Everything here goes through `/api/*` with a bearer token, so the server
//! enforces every rule exactly as it does for the web UI: status is a history
//! row, risk edits are recorded field by field, deleted rows stay hidden, and
//! every write is attributed to the token's user. Nothing in this module
//! touches the database, and it must stay that way — the whole point is that
//! there is one place the rules live.
//!
//! Request and response bodies are the `shared` structs, so a change to the
//! API that this CLI does not follow is a compile error rather than a 400 at
//! runtime.

use std::io::Read;

use anyhow::{anyhow, bail, Context};
use chrono::NaiveDate;
use clap::{Args, Subcommand};
use reqwest::blocking::{Client as HttpClient, RequestBuilder, Response};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use shared::{
    ActionItemResponse, ActivityEntry, ApiError, CategoryResponse, ChangeStatus, CreateActionItem,
    CreateCategory, CreateNote, CreateRisk, CreateRiskNote, CreateVendor, CurrentUserResponse,
    LinkItems, LinkRisk, NoteCreateResponse, NoteResponse, Priority, RiskHistoryResponse,
    RiskNoteResponse, RiskResponse, RiskStatus, RiskTrend, Status, StatusChangeResponse,
    StatusHistoryResponse, UpdateActionItem, UpdateRisk, User, Vendor, VendorWithCounts,
};

// ============================================================================
// Connection
// ============================================================================

/// Where the server is and how to prove who we are. Shared by every API
/// command; see the top-level `--url` / `--token` flags.
#[derive(Args, Debug, Clone)]
#[command(next_help_heading = "Connection")]
pub struct Connection {
    /// Base URL of the running server, e.g. https://tracker.example.org
    #[arg(long, global = true, env = "TRACKER_URL")]
    pub url: Option<String>,

    /// Bearer token from the server's API_TOKENS. Omit against a DEV_MODE server.
    #[arg(long, global = true, env = "TRACKER_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Read the bearer token from a file (one trailing newline is stripped); --token wins if both are given
    #[arg(long, global = true, env = "TRACKER_TOKEN_FILE")]
    pub token_file: Option<std::path::PathBuf>,

    /// Print responses as JSON instead of tables
    #[arg(long, global = true)]
    pub json: bool,
}

pub struct Client {
    base: String,
    token: Option<String>,
    http: HttpClient,
    json: bool,
}

impl Client {
    pub fn connect(conn: &Connection) -> anyhow::Result<Self> {
        let base = conn
            .url
            .clone()
            .ok_or_else(|| anyhow!("Set --url or TRACKER_URL to the server's base URL"))?;
        let base = base.trim_end_matches('/').to_string();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            bail!("--url must start with http:// or https://");
        }

        let token = match (&conn.token, &conn.token_file) {
            (Some(t), _) => Some(t.trim().to_string()),
            (None, Some(path)) => {
                let raw = std::fs::read_to_string(path)
                    .with_context(|| format!("reading token file {}", path.display()))?;
                let trimmed = raw.strip_suffix('\n').unwrap_or(&raw);
                let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
                if trimmed.is_empty() {
                    bail!("token file {} is empty", path.display());
                }
                Some(trimmed.to_string())
            }
            (None, None) => None,
        }
        .filter(|t| !t.is_empty());

        let http = HttpClient::builder()
            .user_agent(concat!("action-tracker-cli/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            base,
            token,
            http,
            json: conn.json,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn auth(&self, req: RequestBuilder) -> RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<T> {
        let req = self.auth(self.http.get(self.url(path)).query(query));
        decode(req.send().with_context(|| format!("GET {path}"))?)
    }

    fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> anyhow::Result<T> {
        let req = self.auth(self.http.post(self.url(path)).json(body));
        decode(req.send().with_context(|| format!("POST {path}"))?)
    }

    fn patch<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> anyhow::Result<T> {
        let req = self.auth(self.http.patch(self.url(path)).json(body));
        decode(req.send().with_context(|| format!("PATCH {path}"))?)
    }

    fn delete(&self, path: &str) -> anyhow::Result<()> {
        let req = self.auth(self.http.delete(self.url(path)));
        let resp = req.send().with_context(|| format!("DELETE {path}"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(api_error(resp))
        }
    }

    /// Print a response either as JSON or through the given table renderer.
    fn emit<T: Serialize>(&self, value: &T, human: impl FnOnce(&T)) -> anyhow::Result<()> {
        if self.json {
            println!("{}", serde_json::to_string_pretty(value)?);
        } else {
            human(value);
        }
        Ok(())
    }

    // ----- lookups ---------------------------------------------------------

    fn me(&self) -> anyhow::Result<CurrentUserResponse> {
        self.get("/auth/me", &[])
    }

    fn vendors(&self, include_archived: bool) -> anyhow::Result<Vec<VendorWithCounts>> {
        self.get(
            "/api/vendors",
            &[("include_archived", include_archived.to_string())],
        )
    }

    /// A vendor by numeric ID or prefix (case-insensitive).
    fn resolve_vendor(&self, key: &str) -> anyhow::Result<Vendor> {
        let key = key.trim();
        let all = self.vendors(true)?;
        let found = match key.parse::<i32>() {
            Ok(id) => all.into_iter().find(|v| v.vendor.id == id),
            Err(_) => all
                .into_iter()
                .find(|v| v.vendor.prefix.eq_ignore_ascii_case(key)),
        };
        found
            .map(|v| v.vendor)
            .ok_or_else(|| anyhow!("No vendor with id or prefix '{key}'. Try: vendors list"))
    }

    /// A user by numeric ID, email, exact name (case-insensitive), or `me`.
    fn resolve_user(&self, key: &str) -> anyhow::Result<i32> {
        let key = key.trim();
        if key.eq_ignore_ascii_case("me") {
            return Ok(self.me()?.user_id);
        }
        if let Ok(id) = key.parse::<i32>() {
            return Ok(id);
        }
        let users: Vec<User> = self.get("/api/users", &[])?;
        users
            .iter()
            .find(|u| u.email.eq_ignore_ascii_case(key))
            .or_else(|| users.iter().find(|u| u.name.eq_ignore_ascii_case(key)))
            .map(|u| u.id)
            .ok_or_else(|| anyhow!("No user with id, email or name '{key}'. Try: users list"))
    }

    /// A category by numeric ID or name within one vendor.
    fn resolve_category(&self, vendor_id: i32, key: &str) -> anyhow::Result<i32> {
        let key = key.trim();
        if let Ok(id) = key.parse::<i32>() {
            return Ok(id);
        }
        let cats: Vec<CategoryResponse> =
            self.get(&format!("/api/vendors/{vendor_id}/categories"), &[])?;
        cats.iter()
            .find(|c| c.name.eq_ignore_ascii_case(key))
            .map(|c| c.id)
            .ok_or_else(|| {
                anyhow!(
                    "No category '{key}' for this vendor. Try: categories list --vendor {vendor_id}"
                )
            })
    }

    fn item(&self, id: &str) -> anyhow::Result<ActionItemResponse> {
        self.get(&format!("/api/items/{}", id.trim()), &[])
    }
}

/// Turn a response into the typed body, or into an error carrying the
/// server's own message. Non-JSON error bodies (a proxy's 502, say) are
/// reported with their status so the cause is not lost.
fn decode<T: DeserializeOwned>(resp: Response) -> anyhow::Result<T> {
    if !resp.status().is_success() {
        return Err(api_error(resp));
    }
    let status = resp.status();
    let text = resp.text()?;
    serde_json::from_str(&text).with_context(|| {
        format!(
            "could not decode the server's {status} response; is --url pointing at tiny-tracker?"
        )
    })
}

fn api_error(resp: Response) -> anyhow::Error {
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    match serde_json::from_str::<ApiError>(&text) {
        Ok(e) => anyhow!("{} ({}, HTTP {})", e.error.message, e.error.code, status.as_u16()),
        Err(_) if status == StatusCode::UNAUTHORIZED => anyhow!(
            "HTTP 401: the server rejected the token. Check TRACKER_TOKEN against the server's API_TOKENS"
        ),
        Err(_) => {
            let snippet: String = text.chars().take(200).collect();
            anyhow!("HTTP {}: {}", status.as_u16(), snippet.trim())
        }
    }
}

/// A positional text argument, with `-` meaning "read standard input" so
/// multi-line content can be piped in rather than fought through quoting.
fn text_arg(value: &str) -> anyhow::Result<String> {
    if value == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading content from stdin")?;
        Ok(buf.trim_end().to_string())
    } else {
        Ok(value.to_string())
    }
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| format!("'{s}' is not a date of the form YYYY-MM-DD"))
}

fn parse_status(s: &str) -> Result<Status, String> {
    Status::from_label(s).ok_or_else(|| {
        format!(
            "'{s}' is not a status. One of: {}",
            Status::all()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn parse_priority(s: &str) -> Result<Priority, String> {
    Priority::from_label(s).ok_or_else(|| {
        format!(
            "'{s}' is not a priority. One of: {}",
            Priority::all()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn parse_risk_status(s: &str) -> Result<RiskStatus, String> {
    RiskStatus::from_label(s).ok_or_else(|| {
        format!(
            "'{s}' is not a risk status. One of: {}",
            RiskStatus::all()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn parse_risk_trend(s: &str) -> Result<RiskTrend, String> {
    RiskTrend::from_label(s).ok_or_else(|| {
        format!(
            "'{s}' is not a risk trend. One of: {}",
            RiskTrend::all()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn parse_scale(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .trim()
        .parse()
        .map_err(|_| format!("'{s}' is not a number"))?;
    if !v.is_finite() || !(0.0..=shared::RISK_SCALE_MAX).contains(&v) {
        return Err(format!("'{s}' is outside 0-{}", shared::RISK_SCALE_MAX));
    }
    Ok(v)
}

// ============================================================================
// Command trees
// ============================================================================

#[derive(Subcommand, Debug)]
pub enum VendorsCmd {
    /// List vendors with open and total item counts
    List {
        /// Include archived vendors
        #[arg(long)]
        archived: bool,
    },
    /// Create a vendor
    Create {
        /// Unique prefix for action item IDs (2-5 uppercase letters)
        #[arg(long)]
        prefix: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum UsersCmd {
    /// List users (owners and authors are referenced by these IDs)
    List,
}

#[derive(Subcommand, Debug)]
pub enum CategoriesCmd {
    /// List the categories of one vendor, or of all vendors
    List {
        /// Vendor prefix or ID
        #[arg(long)]
        vendor: Option<String>,
    },
    /// Create a category for a vendor
    Create {
        /// Vendor prefix or ID
        #[arg(long)]
        vendor: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ItemsCmd {
    /// List action items
    List {
        /// Vendor prefix or ID
        #[arg(long)]
        vendor: Option<String>,
        /// Status label or value, e.g. "In Progress" or in_progress
        #[arg(long, value_parser = parse_status)]
        status: Option<Status>,
        /// Owner: user ID, email, name, or "me"
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, value_parser = parse_priority)]
        priority: Option<Priority>,
    },
    /// Show one action item
    Get {
        /// Item ID, e.g. AD-001
        id: String,
    },
    /// Create an action item; the ID is assigned by the server
    Create {
        /// Vendor prefix or ID
        #[arg(long)]
        vendor: String,
        #[arg(long)]
        title: String,
        /// Category name or ID (see: categories list --vendor X)
        #[arg(long)]
        category: String,
        /// Owner: user ID, email, name, or "me"
        #[arg(long, default_value = "me")]
        owner: String,
        #[arg(long, default_value = "Medium", value_parser = parse_priority)]
        priority: Priority,
        /// Due date, YYYY-MM-DD
        #[arg(long, value_parser = parse_date)]
        due: Option<NaiveDate>,
        /// Longer description; "-" reads it from stdin
        #[arg(long)]
        description: Option<String>,
        /// The vendor's own ticket references, comma-separated
        #[arg(long)]
        tracking: Option<String>,
    },
    /// Change fields on an action item (status has its own command)
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// Category name or ID
        #[arg(long)]
        category: Option<String>,
        /// Owner: user ID, email, name, or "me"
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, value_parser = parse_priority)]
        priority: Option<Priority>,
        /// Due date, YYYY-MM-DD
        #[arg(long, value_parser = parse_date, conflicts_with = "clear_due")]
        due: Option<NaiveDate>,
        /// Remove the due date
        #[arg(long)]
        clear_due: bool,
        /// New description; "-" reads it from stdin
        #[arg(long, conflicts_with = "clear_description")]
        description: Option<String>,
        #[arg(long)]
        clear_description: bool,
        /// The vendor's own ticket references, comma-separated
        #[arg(long, conflicts_with = "clear_tracking")]
        tracking: Option<String>,
        #[arg(long)]
        clear_tracking: bool,
    },
    /// Move an item to a new status, recording who and why
    Status {
        id: String,
        /// Status label or value, e.g. "In Progress" or in_progress
        #[arg(value_parser = parse_status)]
        status: Status,
        #[arg(long)]
        comment: Option<String>,
    },
    /// Add a note to an item
    Note {
        id: String,
        /// Note text; "-" reads it from stdin
        content: String,
        /// Date the note is for, YYYY-MM-DD (default today)
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// List an item's notes, newest first
    Notes { id: String },
    /// Show an item's status history, newest first
    History { id: String },
    /// Risks this item addresses
    Risks { id: String },
    /// Link an item to a risk
    Link {
        id: String,
        /// Risk ID, e.g. RISK-034
        risk_id: String,
    },
    /// Unlink an item from a risk
    Unlink { id: String, risk_id: String },
    /// Soft-delete an item (admin only; restore with the restore-item command)
    Delete { id: String },
}

#[derive(Subcommand, Debug)]
pub enum RisksCmd {
    /// List risks, highest score first
    List {
        /// Vendor prefix or ID
        #[arg(long)]
        vendor: Option<String>,
        /// Open, "In work", Mitigated or Accepted
        #[arg(long, value_parser = parse_risk_status)]
        status: Option<RiskStatus>,
        /// Include risks whose trend is Closed (hidden by default, as in the UI)
        #[arg(long)]
        closed: bool,
    },
    /// Show one risk
    Get {
        /// Risk ID, e.g. RISK-034
        id: String,
    },
    /// Create a risk; the RISK-nnn number is assigned by the server
    Create {
        /// Vendor the risk concerns: prefix or ID
        #[arg(long)]
        vendor: String,
        /// The risk statement
        #[arg(long)]
        title: String,
        #[arg(long)]
        category: String,
        /// 0-5, half points allowed
        #[arg(long, value_parser = parse_scale)]
        severity: f64,
        /// 0-5, half points allowed
        #[arg(long, value_parser = parse_scale)]
        likelihood: f64,
        /// Module, assembly or programme area ("Item" in the sheet)
        #[arg(long)]
        component: Option<String>,
        #[arg(long)]
        cause: Option<String>,
        #[arg(long)]
        effect: Option<String>,
        #[arg(long)]
        mitigation: Option<String>,
        #[arg(long)]
        test_type: Option<String>,
        /// Which tests, by ID, retire the risk
        #[arg(long)]
        verification: Option<String>,
        /// Default Open
        #[arg(long, value_parser = parse_risk_status)]
        status: Option<RiskStatus>,
        /// Default New
        #[arg(long, value_parser = parse_risk_trend)]
        trend: Option<RiskTrend>,
        /// Date opened, YYYY-MM-DD (default today)
        #[arg(long, value_parser = parse_date)]
        opened: Option<NaiveDate>,
        /// Action items to link at creation; repeatable
        #[arg(long = "item")]
        items: Vec<String>,
    },
    /// Change fields on a risk; status, trend, severity and likelihood changes are recorded
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long, value_parser = parse_scale)]
        severity: Option<f64>,
        #[arg(long, value_parser = parse_scale)]
        likelihood: Option<f64>,
        #[arg(long, value_parser = parse_risk_status)]
        status: Option<RiskStatus>,
        #[arg(long, value_parser = parse_risk_trend)]
        trend: Option<RiskTrend>,
        #[arg(long, value_parser = parse_date)]
        opened: Option<NaiveDate>,
        #[arg(long, conflicts_with = "clear_component")]
        component: Option<String>,
        #[arg(long)]
        clear_component: bool,
        #[arg(long, conflicts_with = "clear_cause")]
        cause: Option<String>,
        #[arg(long)]
        clear_cause: bool,
        #[arg(long, conflicts_with = "clear_effect")]
        effect: Option<String>,
        #[arg(long)]
        clear_effect: bool,
        #[arg(long, conflicts_with = "clear_mitigation")]
        mitigation: Option<String>,
        #[arg(long)]
        clear_mitigation: bool,
        #[arg(long, conflicts_with = "clear_test_type")]
        test_type: Option<String>,
        #[arg(long)]
        clear_test_type: bool,
        #[arg(long, conflicts_with = "clear_verification")]
        verification: Option<String>,
        #[arg(long)]
        clear_verification: bool,
        /// Why; attached to every history row this update produces
        #[arg(long)]
        comment: Option<String>,
    },
    /// Add a note to a risk
    Note {
        id: String,
        /// Note text; "-" reads it from stdin
        content: String,
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// List a risk's notes, newest first
    Notes { id: String },
    /// Who changed status, trend, severity or likelihood, and why
    History { id: String },
    /// Live action items linked to a risk
    Items { id: String },
    /// Link one or more action items to a risk (all-or-nothing)
    Link {
        id: String,
        /// Item IDs, e.g. AD-001 AD-002
        #[arg(required = true)]
        item_ids: Vec<String>,
    },
    /// Unlink an action item from a risk
    Unlink { id: String, item_id: String },
    /// Soft-delete a risk (admin only; restore with the restore-risk command)
    Delete { id: String },
}

// ============================================================================
// Runners
// ============================================================================

pub fn whoami(client: &Client) -> anyhow::Result<()> {
    let me = client.me()?;
    client.emit(&me, |me| {
        println!(
            "{} <{}>  id={}  admin={}",
            me.name, me.email, me.user_id, me.is_admin
        );
    })
}

pub fn activity(client: &Client, since: Option<String>, limit: Option<i64>) -> anyhow::Result<()> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(s) = since {
        query.push(("since", s));
    }
    if let Some(l) = limit {
        query.push(("limit", l.to_string()));
    }
    let entries: Vec<ActivityEntry> = client.get("/api/activity", &query)?;
    client.emit(&entries, |entries| {
        if entries.is_empty() {
            println!("No activity.");
            return;
        }
        for e in entries {
            println!(
                "{}  {:<10} {:<16} {:<16} {}",
                e.timestamp.format("%Y-%m-%d %H:%M"),
                e.item_id,
                e.actor_name,
                format!("{:?}", e.event_type),
                e.detail.lines().next().unwrap_or("")
            );
        }
    })
}

pub fn vendors(client: &Client, cmd: VendorsCmd) -> anyhow::Result<()> {
    match cmd {
        VendorsCmd::List { archived } => {
            let list = client.vendors(archived)?;
            client.emit(&list, |list| {
                println!(
                    "{:<5} {:<8} {:<30} {:>6} {:>6}  Archived",
                    "ID", "Prefix", "Name", "Open", "Total"
                );
                println!("{}", "-".repeat(70));
                for v in list {
                    println!(
                        "{:<5} {:<8} {:<30} {:>6} {:>6}  {}",
                        v.vendor.id,
                        v.vendor.prefix,
                        v.vendor.name,
                        v.open_items,
                        v.total_items,
                        if v.vendor.archived { "yes" } else { "" }
                    );
                }
            })
        }
        VendorsCmd::Create {
            prefix,
            name,
            description,
        } => {
            let body = CreateVendor {
                prefix: prefix.trim().to_uppercase(),
                name,
                description,
            };
            let v: Vendor = client.post("/api/vendors", &body)?;
            client.emit(&v, |v| {
                println!("Created vendor {} ({}) id={}", v.name, v.prefix, v.id)
            })
        }
    }
}

pub fn users(client: &Client, cmd: UsersCmd) -> anyhow::Result<()> {
    match cmd {
        UsersCmd::List => {
            let list: Vec<User> = client.get("/api/users", &[])?;
            client.emit(&list, |list| {
                println!("{:<5} {:<30} {:<32} Initials", "ID", "Name", "Email");
                println!("{}", "-".repeat(80));
                for u in list {
                    println!(
                        "{:<5} {:<30} {:<32} {}",
                        u.id,
                        u.name,
                        u.email,
                        u.initials.as_deref().unwrap_or("")
                    );
                }
            })
        }
    }
}

pub fn categories(client: &Client, cmd: CategoriesCmd) -> anyhow::Result<()> {
    match cmd {
        CategoriesCmd::List { vendor } => {
            let list: Vec<CategoryResponse> = match vendor {
                Some(v) => {
                    let vendor = client.resolve_vendor(&v)?;
                    client.get(&format!("/api/vendors/{}/categories", vendor.id), &[])?
                }
                None => client.get("/api/categories", &[])?,
            };
            client.emit(&list, |list| {
                println!("{:<5} {:<8} {:<30} Description", "ID", "Vendor", "Name");
                println!("{}", "-".repeat(70));
                for c in list {
                    println!(
                        "{:<5} {:<8} {:<30} {}",
                        c.id,
                        c.vendor_id,
                        c.name,
                        c.description.as_deref().unwrap_or("")
                    );
                }
            })
        }
        CategoriesCmd::Create {
            vendor,
            name,
            description,
        } => {
            let vendor = client.resolve_vendor(&vendor)?;
            let body = CreateCategory { name, description };
            let c: CategoryResponse =
                client.post(&format!("/api/vendors/{}/categories", vendor.id), &body)?;
            client.emit(&c, |c| {
                println!(
                    "Created category {} id={} for {}",
                    c.name, c.id, vendor.prefix
                )
            })
        }
    }
}

fn print_item_table(items: &[ActionItemResponse]) {
    if items.is_empty() {
        println!("No items.");
        return;
    }
    println!(
        "{:<10} {:<12} {:<8} {:<16} {:<10} Title",
        "ID", "Status", "Priority", "Owner", "Due"
    );
    println!("{}", "-".repeat(90));
    for i in items {
        println!(
            "{:<10} {:<12} {:<8} {:<16} {:<10} {}",
            i.id,
            i.status,
            i.priority,
            truncate(&i.owner_name, 16),
            i.due_date.map(|d| d.to_string()).unwrap_or_default(),
            i.title
        );
    }
}

fn print_item(i: &ActionItemResponse) {
    println!("{}  {}", i.id, i.title);
    println!(
        "  Status:    {} (since {})",
        i.status,
        i.status_changed_at.format("%Y-%m-%d %H:%M")
    );
    println!("  Priority:  {}", i.priority);
    println!("  Category:  {} (id {})", i.category, i.category_id);
    println!("  Owner:     {} (id {})", i.owner_name, i.owner_id);
    println!("  Created:   {} by {}", i.create_date, i.created_by_name);
    if let Some(d) = i.due_date {
        println!("  Due:       {d}");
    }
    if let Some(t) = &i.vendor_tracking_ids {
        println!("  Tracking:  {t}");
    }
    if let Some(d) = &i.description {
        println!("  Description:");
        for line in d.lines() {
            println!("    {line}");
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n - 1).collect();
        format!("{cut}…")
    }
}

pub fn items(client: &Client, cmd: ItemsCmd) -> anyhow::Result<()> {
    match cmd {
        ItemsCmd::List {
            vendor,
            status,
            owner,
            priority,
        } => {
            let mut query: Vec<(&str, String)> = Vec::new();
            if let Some(v) = vendor {
                query.push(("vendor_id", client.resolve_vendor(&v)?.id.to_string()));
            }
            if let Some(s) = status {
                query.push(("status", s.as_str().to_string()));
            }
            if let Some(o) = owner {
                query.push(("owner_id", client.resolve_user(&o)?.to_string()));
            }
            if let Some(p) = priority {
                query.push(("priority", p.as_str().to_string()));
            }
            let list: Vec<ActionItemResponse> = client.get("/api/items", &query)?;
            client.emit(&list, |l| print_item_table(l))
        }

        ItemsCmd::Get { id } => {
            let item = client.item(&id)?;
            client.emit(&item, print_item)
        }

        ItemsCmd::Create {
            vendor,
            title,
            category,
            owner,
            priority,
            due,
            description,
            tracking,
        } => {
            let vendor = client.resolve_vendor(&vendor)?;
            let body = CreateActionItem {
                title,
                due_date: due,
                category_id: client.resolve_category(vendor.id, &category)?,
                owner_id: client.resolve_user(&owner)?,
                priority: priority.as_str().to_string(),
                description: description.as_deref().map(text_arg).transpose()?,
                vendor_tracking_ids: tracking,
            };
            let item: ActionItemResponse =
                client.post(&format!("/api/vendors/{}/items", vendor.id), &body)?;
            client.emit(&item, |i| println!("Created {}  {}", i.id, i.title))
        }

        ItemsCmd::Update {
            id,
            title,
            category,
            owner,
            priority,
            due,
            clear_due,
            description,
            clear_description,
            tracking,
            clear_tracking,
        } => {
            // Only resolve the category when asked to, and against the item's
            // own vendor, since category names are per vendor.
            let category_id = match category {
                Some(c) => {
                    let item = client.item(&id)?;
                    Some(client.resolve_category(item.vendor_id, &c)?)
                }
                None => None,
            };
            let body = UpdateActionItem {
                title,
                due_date: if clear_due { Some(None) } else { due.map(Some) },
                category_id,
                owner_id: owner.map(|o| client.resolve_user(&o)).transpose()?,
                priority: priority.map(|p| p.as_str().to_string()),
                description: if clear_description {
                    Some(None)
                } else {
                    description.as_deref().map(text_arg).transpose()?.map(Some)
                },
                vendor_tracking_ids: if clear_tracking {
                    Some(None)
                } else {
                    tracking.map(Some)
                },
            };
            if body == UpdateActionItem::default() {
                bail!("Nothing to update: pass at least one field flag (see --help)");
            }
            let item: ActionItemResponse =
                client.patch(&format!("/api/items/{}", id.trim()), &body)?;
            client.emit(&item, print_item)
        }

        ItemsCmd::Status {
            id,
            status,
            comment,
        } => {
            let body = ChangeStatus { status, comment };
            let r: StatusChangeResponse =
                client.post(&format!("/api/items/{}/status", id.trim()), &body)?;
            client.emit(&r, |r| {
                println!(
                    "{} is now {} (changed {})",
                    r.action_item_id,
                    r.status,
                    r.changed_at.format("%Y-%m-%d %H:%M")
                )
            })
        }

        ItemsCmd::Note { id, content, date } => {
            let body = CreateNote {
                note_date: date,
                content: text_arg(&content)?,
            };
            let n: NoteCreateResponse =
                client.post(&format!("/api/items/{}/notes", id.trim()), &body)?;
            client.emit(&n, |n| {
                println!(
                    "Added note {} to {} dated {}",
                    n.id, n.action_item_id, n.note_date
                )
            })
        }

        ItemsCmd::Notes { id } => {
            let notes: Vec<NoteResponse> =
                client.get(&format!("/api/items/{}/notes", id.trim()), &[])?;
            client.emit(&notes, |notes| {
                if notes.is_empty() {
                    println!("No notes.");
                }
                for n in notes {
                    println!("[{}] {} (note {}):", n.date, n.author_name, n.id);
                    for line in n.content.lines() {
                        println!("    {line}");
                    }
                }
            })
        }

        ItemsCmd::History { id } => {
            let rows: Vec<StatusHistoryResponse> =
                client.get(&format!("/api/items/{}/history", id.trim()), &[])?;
            client.emit(&rows, |rows| {
                for h in rows {
                    println!(
                        "{}  {:<12} {:<16} {}",
                        h.changed_at.format("%Y-%m-%d %H:%M"),
                        h.status,
                        truncate(&h.changed_by_name, 16),
                        h.comment.as_deref().unwrap_or("")
                    );
                }
            })
        }

        ItemsCmd::Risks { id } => {
            let risks: Vec<RiskResponse> =
                client.get(&format!("/api/items/{}/risks", id.trim()), &[])?;
            client.emit(&risks, |r| print_risk_table(r))
        }

        ItemsCmd::Link { id, risk_id } => {
            let body = LinkRisk {
                risk_id: risk_id.trim().to_uppercase(),
            };
            let r: RiskResponse = client.post(&format!("/api/items/{}/risks", id.trim()), &body)?;
            client.emit(&r, |r| {
                println!(
                    "Linked {} to {} ({} items now)",
                    id.trim(),
                    r.id,
                    r.linked_item_count
                )
            })
        }

        ItemsCmd::Unlink { id, risk_id } => {
            client.delete(&format!(
                "/api/items/{}/risks/{}",
                id.trim(),
                risk_id.trim().to_uppercase()
            ))?;
            println!(
                "Unlinked {} from {}",
                id.trim(),
                risk_id.trim().to_uppercase()
            );
            Ok(())
        }

        ItemsCmd::Delete { id } => {
            client.delete(&format!("/api/items/{}", id.trim()))?;
            println!(
                "Deleted {}. It can be restored with: action-tracker-cli restore-item --id {}",
                id.trim(),
                id.trim()
            );
            Ok(())
        }
    }
}

fn print_risk_table(risks: &[RiskResponse]) {
    if risks.is_empty() {
        println!("No risks.");
        return;
    }
    println!(
        "{:<10} {:<6} {:>5} {:<6} {:<10} {:<10} {:<14} Risk",
        "ID", "Vendor", "Score", "Level", "Status", "Trend", "Item"
    );
    println!("{}", "-".repeat(100));
    for r in risks {
        println!(
            "{:<10} {:<6} {:>5} {:<6} {:<10} {:<10} {:<14} {}",
            r.id,
            r.vendor_prefix,
            shared::format_scale(r.score),
            r.level.as_str(),
            r.status.as_str(),
            r.trend.as_str(),
            truncate(r.component.as_deref().unwrap_or(""), 14),
            truncate(&r.title, 50)
        );
    }
}

fn print_risk(r: &RiskResponse) {
    println!("{}  {}", r.id, r.title);
    println!("  Vendor:      {}", r.vendor_prefix);
    println!(
        "  Score:       {} = severity {} x likelihood {}  ({})",
        shared::format_scale(r.score),
        shared::format_scale(r.severity),
        shared::format_scale(r.likelihood),
        r.level.as_str()
    );
    println!(
        "  Status:      {}   Trend: {}",
        r.status.as_str(),
        r.trend.as_str()
    );
    println!("  Category:    {}", r.category);
    if let Some(c) = &r.component {
        println!("  Item:        {c}");
    }
    println!("  Opened:      {} by {}", r.opened_on, r.created_by_name);
    println!("  Linked items: {}", r.linked_item_count);
    for (label, value) in [
        ("Cause", &r.cause),
        ("Effect", &r.effect),
        ("Mitigation", &r.mitigation),
        ("Test type", &r.test_type),
        ("Verification", &r.verification),
    ] {
        if let Some(v) = value {
            println!("  {label}:");
            for line in v.lines() {
                println!("    {line}");
            }
        }
    }
}

pub fn risks(client: &Client, cmd: RisksCmd) -> anyhow::Result<()> {
    match cmd {
        RisksCmd::List {
            vendor,
            status,
            closed,
        } => {
            let mut query: Vec<(&str, String)> = Vec::new();
            if let Some(v) = vendor {
                query.push(("vendor_id", client.resolve_vendor(&v)?.id.to_string()));
            }
            if let Some(s) = status {
                query.push(("status", s.as_str().to_string()));
            }
            let mut list: Vec<RiskResponse> = client.get("/api/risks", &query)?;
            if !closed {
                list.retain(|r| r.trend != RiskTrend::Closed);
            }
            client.emit(&list, |l| print_risk_table(l))
        }

        RisksCmd::Get { id } => {
            let r: RiskResponse =
                client.get(&format!("/api/risks/{}", id.trim().to_uppercase()), &[])?;
            client.emit(&r, print_risk)
        }

        RisksCmd::Create {
            vendor,
            title,
            category,
            severity,
            likelihood,
            component,
            cause,
            effect,
            mitigation,
            test_type,
            verification,
            status,
            trend,
            opened,
            items,
        } => {
            let vendor = client.resolve_vendor(&vendor)?;
            let body = CreateRisk {
                component,
                category,
                title,
                cause,
                effect,
                severity,
                likelihood,
                mitigation,
                test_type,
                verification,
                status,
                trend,
                opened_on: opened,
                item_ids: items.iter().map(|i| i.trim().to_uppercase()).collect(),
            };
            let r: RiskResponse =
                client.post(&format!("/api/vendors/{}/risks", vendor.id), &body)?;
            client.emit(&r, |r| {
                println!(
                    "Created {}  score {} ({})  {}",
                    r.id,
                    shared::format_scale(r.score),
                    r.level.as_str(),
                    r.title
                )
            })
        }

        RisksCmd::Update {
            id,
            title,
            category,
            severity,
            likelihood,
            status,
            trend,
            opened,
            component,
            clear_component,
            cause,
            clear_cause,
            effect,
            clear_effect,
            mitigation,
            clear_mitigation,
            test_type,
            clear_test_type,
            verification,
            clear_verification,
            comment,
        } => {
            fn nullable(value: Option<String>, clear: bool) -> Option<Option<String>> {
                if clear {
                    Some(None)
                } else {
                    value.map(Some)
                }
            }
            let body = UpdateRisk {
                component: nullable(component, clear_component),
                category,
                title,
                cause: nullable(cause, clear_cause),
                effect: nullable(effect, clear_effect),
                severity,
                likelihood,
                mitigation: nullable(mitigation, clear_mitigation),
                test_type: nullable(test_type, clear_test_type),
                verification: nullable(verification, clear_verification),
                status,
                trend,
                opened_on: opened,
                comment,
            };
            let mut without_comment = body.clone();
            without_comment.comment = None;
            if without_comment == UpdateRisk::default() {
                bail!("Nothing to update: pass at least one field flag (see --help)");
            }
            let r: RiskResponse =
                client.patch(&format!("/api/risks/{}", id.trim().to_uppercase()), &body)?;
            client.emit(&r, print_risk)
        }

        RisksCmd::Note { id, content, date } => {
            let body = CreateRiskNote {
                note_date: date,
                content: text_arg(&content)?,
            };
            let n: RiskNoteResponse = client.post(
                &format!("/api/risks/{}/notes", id.trim().to_uppercase()),
                &body,
            )?;
            client.emit(&n, |n| {
                println!("Added note {} to {} dated {}", n.id, n.risk_id, n.note_date)
            })
        }

        RisksCmd::Notes { id } => {
            let notes: Vec<RiskNoteResponse> = client.get(
                &format!("/api/risks/{}/notes", id.trim().to_uppercase()),
                &[],
            )?;
            client.emit(&notes, |notes| {
                if notes.is_empty() {
                    println!("No notes.");
                }
                for n in notes {
                    println!("[{}] {} (note {}):", n.note_date, n.author_name, n.id);
                    for line in n.content.lines() {
                        println!("    {line}");
                    }
                }
            })
        }

        RisksCmd::History { id } => {
            let rows: Vec<RiskHistoryResponse> = client.get(
                &format!("/api/risks/{}/history", id.trim().to_uppercase()),
                &[],
            )?;
            client.emit(&rows, |rows| {
                for h in rows {
                    let change = match (&h.old_value, &h.new_value) {
                        (Some(o), Some(n)) => format!("{o} -> {n}"),
                        (None, Some(n)) => n.clone(),
                        _ => String::new(),
                    };
                    println!(
                        "{}  {:<11} {:<24} {:<16} {}",
                        h.changed_at.format("%Y-%m-%d %H:%M"),
                        h.field.as_str(),
                        truncate(&change, 24),
                        truncate(&h.changed_by_name, 16),
                        h.comment.as_deref().unwrap_or("")
                    );
                }
            })
        }

        RisksCmd::Items { id } => {
            let items: Vec<ActionItemResponse> = client.get(
                &format!("/api/risks/{}/items", id.trim().to_uppercase()),
                &[],
            )?;
            client.emit(&items, |i| print_item_table(i))
        }

        RisksCmd::Link { id, item_ids } => {
            let body = LinkItems {
                item_ids: item_ids.iter().map(|i| i.trim().to_uppercase()).collect(),
            };
            let r: RiskResponse = client.post(
                &format!("/api/risks/{}/items", id.trim().to_uppercase()),
                &body,
            )?;
            client.emit(&r, |r| {
                println!("{} now has {} linked item(s)", r.id, r.linked_item_count)
            })
        }

        RisksCmd::Unlink { id, item_id } => {
            let risk_id = id.trim().to_uppercase();
            let item_id = item_id.trim().to_uppercase();
            client.delete(&format!("/api/items/{item_id}/risks/{risk_id}"))?;
            println!("Unlinked {item_id} from {risk_id}");
            Ok(())
        }

        RisksCmd::Delete { id } => {
            let risk_id = id.trim().to_uppercase();
            client.delete(&format!("/api/risks/{risk_id}"))?;
            println!("Deleted {risk_id}. It can be restored with: action-tracker-cli restore-risk --id {risk_id}");
            Ok(())
        }
    }
}
