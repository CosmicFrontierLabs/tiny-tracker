use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use diesel::prelude::*;
use diesel::PgConnection;

mod schema {
    diesel::table! {
        users (id) {
            id -> Int4,
            email -> Varchar,
            name -> Varchar,
            initials -> Nullable<Varchar>,
            created_at -> Timestamptz,
        }
    }

    diesel::table! {
        vendors (id) {
            id -> Int4,
            prefix -> Varchar,
            name -> Varchar,
            description -> Nullable<Text>,
            next_number -> Int4,
            created_at -> Timestamptz,
            archived -> Bool,
        }
    }

    diesel::table! {
        categories (id) {
            id -> Int4,
            vendor_id -> Int4,
            #[max_length = 100]
            name -> Varchar,
            description -> Nullable<Text>,
            created_at -> Timestamptz,
        }
    }

    diesel::table! {
        action_items (id) {
            #[max_length = 20]
            id -> Varchar,
            vendor_id -> Int4,
            number -> Int4,
            #[max_length = 500]
            title -> Varchar,
            create_date -> Date,
            created_by_id -> Int4,
            due_date -> Nullable<Date>,
            owner_id -> Int4,
            #[max_length = 20]
            priority -> Varchar,
            created_at -> Timestamptz,
            updated_at -> Timestamptz,
            description -> Nullable<Text>,
            category_id -> Int4,
        }
    }

    diesel::table! {
        status_history (id) {
            id -> Int4,
            #[max_length = 20]
            action_item_id -> Varchar,
            #[max_length = 50]
            status -> Varchar,
            changed_by_id -> Int4,
            changed_at -> Timestamptz,
            comment -> Nullable<Text>,
        }
    }

    diesel::table! {
        notes (id) {
            id -> Int4,
            #[max_length = 20]
            action_item_id -> Varchar,
            note_date -> Date,
            author_id -> Int4,
            content -> Text,
            created_at -> Timestamptz,
        }
    }

    diesel::allow_tables_to_appear_in_same_query!(
        action_items,
        categories,
        notes,
        status_history,
        users,
        vendors,
    );
}

use schema::*;

// ============================================================================
// CLI
// ============================================================================

#[derive(Parser)]
#[command(name = "action-tracker-cli")]
#[command(about = "Admin CLI for Action Tracker")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new user
    CreateUser {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        initials: Option<String>,
    },
    /// List all users
    ListUsers,
    /// Create a new vendor
    CreateVendor {
        /// Unique prefix for action item IDs (2-5 uppercase letters)
        #[arg(long)]
        prefix: String,
        /// Display name for the vendor
        #[arg(long)]
        name: String,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },
    /// List all vendors
    ListVendors,
    /// Reset a vendor's next_number sequence
    ResetSequence {
        #[arg(long)]
        vendor: String,
    },
    /// Import action items from a CSV file
    ImportCsv {
        /// Path to the CSV file
        #[arg(long)]
        file: PathBuf,
        /// Vendor prefix (e.g. "AD") - derived from item IDs if not provided
        #[arg(long)]
        vendor: Option<String>,
        /// Dry run - parse and validate without writing to the database
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

// ============================================================================
// Models
// ============================================================================

#[derive(Insertable)]
#[diesel(table_name = users)]
struct NewUser {
    email: String,
    name: String,
    initials: Option<String>,
}

#[derive(Queryable)]
#[allow(dead_code)]
struct User {
    id: i32,
    email: String,
    name: String,
    initials: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Insertable)]
#[diesel(table_name = vendors)]
struct NewVendor {
    prefix: String,
    name: String,
    description: Option<String>,
}

#[derive(Queryable)]
#[allow(dead_code)]
struct Vendor {
    id: i32,
    prefix: String,
    name: String,
    description: Option<String>,
    next_number: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    archived: bool,
}

#[derive(Queryable)]
#[allow(dead_code)]
struct Category {
    id: i32,
    vendor_id: i32,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Insertable)]
#[diesel(table_name = categories)]
struct NewCategory {
    vendor_id: i32,
    name: String,
}

#[derive(Insertable)]
#[diesel(table_name = action_items)]
struct NewActionItem {
    id: String,
    vendor_id: i32,
    number: i32,
    title: String,
    create_date: NaiveDate,
    created_by_id: i32,
    due_date: Option<NaiveDate>,
    owner_id: i32,
    priority: String,
    description: Option<String>,
    category_id: i32,
}

#[derive(Insertable)]
#[diesel(table_name = status_history)]
struct NewStatusHistory {
    action_item_id: String,
    status: String,
    changed_by_id: i32,
    comment: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = notes)]
struct NewNote {
    action_item_id: String,
    note_date: NaiveDate,
    author_id: i32,
    content: String,
}

// ============================================================================
// CSV row
// ============================================================================

#[derive(Debug, serde::Deserialize)]
struct CsvRow {
    #[serde(rename = "Action Item #")]
    action_item_id: String,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Create Date")]
    create_date: String,
    #[serde(rename = "Created by")]
    created_by: String,
    #[serde(rename = "Due Date")]
    due_date: String,
    #[serde(rename = "Category")]
    category: String,
    #[serde(rename = "Owner")]
    owner: String,
    #[serde(rename = "Priority")]
    priority: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Status Date")]
    status_date: String,
    #[serde(rename = "Notes")]
    notes: String,
}

// ============================================================================
// Import logic
// ============================================================================

/// Parse a date string in M/D/YYYY or MM/DD/YYYY format.
fn parse_date(s: &str) -> anyhow::Result<NaiveDate> {
    let s = s.trim();
    // Try M/D/YYYY (US format)
    if let Ok(d) = NaiveDate::parse_from_str(s, "%m/%d/%Y") {
        return Ok(d);
    }
    // Try MM/DD/YYYY with leading zeros
    if let Ok(d) = NaiveDate::parse_from_str(s, "%-m/%-d/%Y") {
        return Ok(d);
    }
    anyhow::bail!("Cannot parse date: '{}'", s)
}

/// Normalize a status string to the canonical form used in the database.
fn normalize_status(s: &str) -> anyhow::Result<String> {
    match s.trim().to_lowercase().as_str() {
        "new" => Ok("New".to_string()),
        "not started" => Ok("Not Started".to_string()),
        "in progress" | "in-progress" => Ok("In Progress".to_string()),
        "tbc" => Ok("TBC".to_string()),
        "complete" | "completed" | "done" => Ok("Complete".to_string()),
        "blocked" => Ok("Blocked".to_string()),
        other => anyhow::bail!("Unknown status: '{}'", other),
    }
}

/// Normalize a priority string.
fn normalize_priority(s: &str) -> anyhow::Result<String> {
    match s.trim().to_lowercase().as_str() {
        "high" | "h" => Ok("High".to_string()),
        "medium" | "med" | "m" => Ok("Medium".to_string()),
        "low" | "l" => Ok("Low".to_string()),
        other => anyhow::bail!("Unknown priority: '{}'", other),
    }
}

/// Parse the item ID into (prefix, number). E.g. "AD-001" -> ("AD", 1)
fn parse_item_id(s: &str) -> anyhow::Result<(String, i32)> {
    let s = s.trim();
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid action item ID format: '{}' (expected PREFIX-NUMBER)", s);
    }
    let prefix = parts[0].to_string();
    let number: i32 = parts[1]
        .parse()
        .with_context(|| format!("Invalid number in item ID: '{}'", s))?;
    Ok((prefix, number))
}

/// Resolve a user name (like "M. Fitzgerald") to a user ID.
/// Matches against the `name` column using case-insensitive prefix/contains matching.
fn resolve_user<'a>(
    name: &str,
    users: &'a [User],
    user_cache: &mut HashMap<String, i32>,
) -> anyhow::Result<i32> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("Empty user name");
    }

    if let Some(&id) = user_cache.get(name) {
        return Ok(id);
    }

    let lower = name.to_lowercase();

    // Exact match first
    if let Some(u) = users.iter().find(|u| u.name.to_lowercase() == lower) {
        user_cache.insert(name.to_string(), u.id);
        return Ok(u.id);
    }

    // Try matching "F. Last" pattern against "First Last" in the DB.
    // E.g. "M. Fitzgerald" matches "Mike Fitzgerald"
    if let Some(dot_pos) = name.find('.') {
        let initial = &name[..dot_pos];
        let last_name = name[dot_pos + 1..].trim();
        if let Some(u) = users.iter().find(|u| {
            let u_lower = u.name.to_lowercase();
            u_lower.starts_with(&initial.to_lowercase())
                && u_lower.ends_with(&last_name.to_lowercase())
        }) {
            user_cache.insert(name.to_string(), u.id);
            return Ok(u.id);
        }
    }

    // Try initials match
    if let Some(u) = users.iter().find(|u| {
        u.initials
            .as_deref()
            .map(|i| i.to_lowercase() == lower)
            .unwrap_or(false)
    }) {
        user_cache.insert(name.to_string(), u.id);
        return Ok(u.id);
    }

    // Contains match (last resort)
    if let Some(u) = users
        .iter()
        .find(|u| u.name.to_lowercase().contains(&lower))
    {
        user_cache.insert(name.to_string(), u.id);
        return Ok(u.id);
    }

    anyhow::bail!(
        "Cannot resolve user '{}'. Known users: {}",
        name,
        users
            .iter()
            .map(|u| u.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Parse multi-line notes into individual (date, content) entries.
/// Format: "MM/DD/YYYY INITIALS: content\nMM/DD/YYYY INITIALS: content"
fn parse_notes(raw: &str) -> Vec<(Option<NaiveDate>, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let mut entries: Vec<(Option<NaiveDate>, String)> = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to detect if this line starts a new note entry (date prefix)
        let has_date_prefix = try_parse_note_date(line).is_some();

        if has_date_prefix {
            let (date, rest) = try_parse_note_date(line).unwrap();
            entries.push((Some(date), rest));
        } else if let Some(last) = entries.last_mut() {
            // Continuation of previous note
            last.1.push('\n');
            last.1.push_str(line);
        } else {
            // First entry with no date
            entries.push((None, line.to_string()));
        }
    }

    entries
}

/// Try to extract a date from the beginning of a note line.
/// Formats: "M/D/YYYY text..." or "MM/DD/YYYY text..."
fn try_parse_note_date(line: &str) -> Option<(NaiveDate, String)> {
    // Look for a date-like pattern at the start: digits/digits/digits
    let bytes = line.as_bytes();
    let mut i = 0;

    // Skip digits for month
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i > 2 || i >= bytes.len() || bytes[i] != b'/' {
        return None;
    }
    i += 1; // skip '/'

    // Skip digits for day
    let day_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == day_start || i - day_start > 2 || i >= bytes.len() || bytes[i] != b'/' {
        return None;
    }
    i += 1; // skip '/'

    // Skip digits for year
    let year_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i - year_start != 4 {
        return None;
    }

    let date_str = &line[..i];
    let rest = line[i..].trim().to_string();

    let date = parse_date(date_str).ok()?;
    Some((date, rest))
}

fn import_csv(file: PathBuf, vendor_prefix: Option<String>, dry_run: bool) -> anyhow::Result<()> {
    // Read and parse CSV, skipping the first two header/info rows
    let contents = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    // The CSV has 2 junk rows before the real header. Skip them.
    let lines: Vec<&str> = contents.lines().collect();
    if lines.len() < 3 {
        anyhow::bail!("CSV file too short - expected header rows + data");
    }

    // Find the header row (contains "Action Item #")
    let header_idx = lines
        .iter()
        .position(|l| l.contains("Action Item #"))
        .context("Could not find header row containing 'Action Item #'")?;

    let csv_body = lines[header_idx..].join("\n");
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_body.as_bytes());

    let mut rows: Vec<CsvRow> = Vec::new();
    for result in rdr.deserialize() {
        let row: CsvRow = result?;
        // Skip empty rows (just an ID with no title)
        if row.title.trim().is_empty() {
            continue;
        }
        rows.push(row);
    }

    if rows.is_empty() {
        println!("No action items found in CSV.");
        return Ok(());
    }

    // Determine vendor prefix from item IDs if not provided
    let prefix = if let Some(p) = vendor_prefix {
        p
    } else {
        let (p, _) = parse_item_id(&rows[0].action_item_id)?;
        p
    };

    println!("Parsed {} action items for vendor '{}'", rows.len(), prefix);

    // Collect unique values for validation reporting
    let unique_users: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        for row in &rows {
            if !row.created_by.trim().is_empty() {
                set.insert(row.created_by.trim().to_string());
            }
            if !row.owner.trim().is_empty() {
                set.insert(row.owner.trim().to_string());
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };

    let unique_categories: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        for row in &rows {
            if !row.category.trim().is_empty() {
                set.insert(row.category.trim().to_string());
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };

    println!("\nUsers referenced in CSV:");
    for u in &unique_users {
        println!("  - {}", u);
    }
    println!("\nCategories referenced in CSV:");
    for c in &unique_categories {
        println!("  - {}", c);
    }

    // Validate all rows parse correctly
    let mut max_number: i32 = 0;
    let mut errors: Vec<String> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let line = i + 1;

        match parse_item_id(&row.action_item_id) {
            Ok((p, n)) => {
                if p != prefix {
                    errors.push(format!(
                        "Row {}: Item '{}' has prefix '{}', expected '{}'",
                        line, row.action_item_id, p, prefix
                    ));
                }
                max_number = max_number.max(n);
            }
            Err(e) => errors.push(format!("Row {}: {}", line, e)),
        }

        if let Err(e) = parse_date(&row.create_date) {
            errors.push(format!("Row {}: create_date: {}", line, e));
        }

        if !row.due_date.trim().is_empty()
            && row.due_date.trim().to_uppercase() != "TBD"
            && row.due_date.trim().to_uppercase() != "PDR"
        {
            if let Err(e) = parse_date(&row.due_date) {
                errors.push(format!("Row {}: due_date: {}", line, e));
            }
        }

        if let Err(e) = normalize_priority(&row.priority) {
            errors.push(format!("Row {}: {}", line, e));
        }

        if let Err(e) = normalize_status(&row.status) {
            errors.push(format!("Row {}: {}", line, e));
        }

        if !row.status_date.trim().is_empty() {
            if let Err(e) = parse_date(&row.status_date) {
                errors.push(format!("Row {}: status_date: {}", line, e));
            }
        }
    }

    if !errors.is_empty() {
        println!("\nValidation errors:");
        for e in &errors {
            println!("  ERROR: {}", e);
        }
        anyhow::bail!("{} validation error(s) found", errors.len());
    }

    println!("\nAll rows validated successfully.");

    if dry_run {
        println!("\n[DRY RUN] Would import {} action items.", rows.len());
        for row in &rows {
            let note_entries = parse_notes(&row.notes);
            println!(
                "  {} - {} (status: {}, {} notes)",
                row.action_item_id,
                row.title,
                row.status,
                note_entries.len()
            );
        }
        return Ok(());
    }

    // Connect to database
    let mut conn = establish_connection();

    // Look up vendor
    let vendor: Vendor = vendors::table
        .filter(vendors::prefix.eq(&prefix))
        .first(&mut conn)
        .with_context(|| format!("Vendor with prefix '{}' not found. Create it first.", prefix))?;

    // Load all users for name resolution
    let all_users: Vec<User> = users::table
        .order(users::name.asc())
        .load(&mut conn)?;

    if all_users.is_empty() {
        anyhow::bail!("No users in database. Create users first.");
    }

    let mut user_cache: HashMap<String, i32> = HashMap::new();

    // Verify all user references resolve before inserting anything
    for row in &rows {
        if !row.created_by.trim().is_empty() {
            resolve_user(&row.created_by, &all_users, &mut user_cache)
                .with_context(|| format!("Item {}: created_by", row.action_item_id))?;
        }
        if !row.owner.trim().is_empty() {
            resolve_user(&row.owner, &all_users, &mut user_cache)
                .with_context(|| format!("Item {}: owner", row.action_item_id))?;
        }
    }

    println!("\nUser resolution:");
    for (csv_name, user_id) in &user_cache {
        let user = all_users.iter().find(|u| u.id == *user_id).unwrap();
        println!("  '{}' -> {} (id={})", csv_name, user.name, user.id);
    }

    // Ensure categories exist
    let mut category_cache: HashMap<String, i32> = HashMap::new();
    let existing_categories: Vec<Category> = categories::table
        .filter(categories::vendor_id.eq(vendor.id))
        .load(&mut conn)?;

    for cat in &existing_categories {
        category_cache.insert(cat.name.clone(), cat.id);
    }

    for cat_name in &unique_categories {
        if !category_cache.contains_key(cat_name) {
            let new_cat = NewCategory {
                vendor_id: vendor.id,
                name: cat_name.clone(),
            };
            let created: Category = diesel::insert_into(categories::table)
                .values(&new_cat)
                .get_result(&mut conn)?;
            println!("  Created category: '{}' (id={})", cat_name, created.id);
            category_cache.insert(cat_name.clone(), created.id);
        }
    }

    // Import each row inside a transaction
    conn.transaction::<_, anyhow::Error, _>(|conn| {
        let mut imported = 0;
        let mut skipped = 0;

        // Use the first user as a fallback for notes/status author
        let fallback_user_id = all_users[0].id;

        for row in &rows {
            let (_, number) = parse_item_id(&row.action_item_id)?;

            // Check if item already exists
            let exists: bool = diesel::select(diesel::dsl::exists(
                action_items::table.filter(action_items::id.eq(&row.action_item_id)),
            ))
            .get_result(conn)?;

            if exists {
                println!("  SKIP {} (already exists)", row.action_item_id);
                skipped += 1;
                continue;
            }

            let created_by_id = if row.created_by.trim().is_empty() {
                fallback_user_id
            } else {
                *user_cache.get(row.created_by.trim()).unwrap()
            };

            let owner_id = if row.owner.trim().is_empty() {
                created_by_id
            } else {
                *user_cache.get(row.owner.trim()).unwrap()
            };

            let category_id = *category_cache
                .get(row.category.trim())
                .context("Category not found")?;

            let create_date = parse_date(&row.create_date)?;
            let due_date = {
                let d = row.due_date.trim();
                if d.is_empty() || d.eq_ignore_ascii_case("TBD") || d.eq_ignore_ascii_case("PDR") {
                    None
                } else {
                    Some(parse_date(d)?)
                }
            };

            let priority = normalize_priority(&row.priority)?;
            let status = normalize_status(&row.status)?;

            let new_item = NewActionItem {
                id: row.action_item_id.trim().to_string(),
                vendor_id: vendor.id,
                number,
                title: row.title.trim().to_string(),
                create_date,
                created_by_id,
                due_date,
                owner_id,
                priority,
                description: None,
                category_id,
            };

            diesel::insert_into(action_items::table)
                .values(&new_item)
                .execute(conn)?;

            // Insert initial status history
            let status_entry = NewStatusHistory {
                action_item_id: row.action_item_id.trim().to_string(),
                status,
                changed_by_id: created_by_id,
                comment: Some("Imported from CSV".to_string()),
            };
            diesel::insert_into(status_history::table)
                .values(&status_entry)
                .execute(conn)?;

            // Parse and insert notes
            let note_entries = parse_notes(&row.notes);
            for (note_date, content) in &note_entries {
                let new_note = NewNote {
                    action_item_id: row.action_item_id.trim().to_string(),
                    note_date: note_date.unwrap_or(create_date),
                    author_id: created_by_id,
                    content: content.clone(),
                };
                diesel::insert_into(notes::table)
                    .values(&new_note)
                    .execute(conn)?;
            }

            println!(
                "  IMPORTED {} - {} ({} notes)",
                row.action_item_id,
                row.title,
                note_entries.len()
            );
            imported += 1;
        }

        // Update vendor's next_number to be past the highest imported number
        let new_next = max_number + 1;
        if new_next > vendor.next_number {
            diesel::update(vendors::table.filter(vendors::id.eq(vendor.id)))
                .set(vendors::next_number.eq(new_next))
                .execute(conn)?;
            println!(
                "\nUpdated vendor '{}' next_number: {} -> {}",
                prefix, vendor.next_number, new_next
            );
        }

        println!(
            "\nImport complete: {} imported, {} skipped",
            imported, skipped
        );

        Ok(())
    })?;

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn establish_connection() -> PgConnection {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::CreateUser {
            email,
            name,
            initials,
        } => {
            let mut conn = establish_connection();

            let new_user = NewUser {
                email: email.clone(),
                name: name.clone(),
                initials,
            };

            diesel::insert_into(users::table)
                .values(&new_user)
                .execute(&mut conn)?;

            println!("Created user: {} <{}>", name, email);
        }

        Commands::ListUsers => {
            let mut conn = establish_connection();

            let results: Vec<User> = users::table.order(users::name.asc()).load(&mut conn)?;

            println!(
                "{:<5} {:<30} {:<30} {:<10}",
                "ID", "Name", "Email", "Initials"
            );
            println!("{}", "-".repeat(80));
            for user in results {
                println!(
                    "{:<5} {:<30} {:<30} {:<10}",
                    user.id,
                    user.name,
                    user.email,
                    user.initials.unwrap_or_default()
                );
            }
        }

        Commands::CreateVendor {
            prefix,
            name,
            description,
        } => {
            // Validate prefix
            if prefix.len() < 2 || prefix.len() > 5 {
                anyhow::bail!("Prefix must be 2-5 characters");
            }
            if !prefix.chars().all(|c| c.is_ascii_uppercase()) {
                anyhow::bail!("Prefix must be uppercase letters only");
            }

            let mut conn = establish_connection();

            let new_vendor = NewVendor {
                prefix: prefix.clone(),
                name: name.clone(),
                description,
            };

            diesel::insert_into(vendors::table)
                .values(&new_vendor)
                .execute(&mut conn)?;

            println!("Created vendor: {} ({})", name, prefix);
        }

        Commands::ListVendors => {
            let mut conn = establish_connection();

            let results: Vec<Vendor> = vendors::table
                .order(vendors::prefix.asc())
                .load(&mut conn)?;

            println!(
                "{:<5} {:<10} {:<30} {:<10}",
                "ID", "Prefix", "Name", "Next #"
            );
            println!("{}", "-".repeat(60));
            for vendor in results {
                println!(
                    "{:<5} {:<10} {:<30} {:<10}",
                    vendor.id, vendor.prefix, vendor.name, vendor.next_number
                );
            }
        }

        Commands::ResetSequence { vendor } => {
            let mut conn = establish_connection();

            let vendor_record: Vendor = vendors::table
                .filter(vendors::prefix.eq(&vendor))
                .first(&mut conn)?;

            println!(
                "Vendor {} ({}) - current next_number: {}",
                vendor_record.prefix, vendor_record.name, vendor_record.next_number
            );
            println!("To reset, manually update the vendors table.");
        }

        Commands::ImportCsv {
            file,
            vendor,
            dry_run,
        } => {
            import_csv(file, vendor, dry_run)?;
        }
    }

    Ok(())
}
