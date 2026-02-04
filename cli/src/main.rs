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
        }
    }
}

use schema::*;

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
}

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
}

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
    }

    Ok(())
}
