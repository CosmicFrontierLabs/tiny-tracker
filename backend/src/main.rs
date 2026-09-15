mod db;
mod mail;
mod models;
mod routes;
mod static_files;

use axum::{
    routing::{get, post},
    Router,
};
use diesel::ConnectionError;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::AsyncPgConnection;
use futures_util::FutureExt;
use rustls_platform_verifier::ConfigVerifierExt;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use routes::{
    activity, auth, categories, health, items, mail_test, notes, status, users, vendor_portal,
    vendors,
};

pub type DbPool = Pool<AsyncPgConnection>;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub config: AppConfig,
    pub mailer: Arc<dyn mail::Mailer>,
    pub mail_health: Arc<mail::MailHealthTracker>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub jwt_secret: String,
    pub dev_mode: bool,
    pub dev_user_id: Option<i32>,
    pub public_url: String,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub allowed_email_domains: Vec<String>,
    /// Lower-cased emails allowed to perform destructive admin actions.
    pub admin_emails: Vec<String>,
    pub mail: mail::MailConfig,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let dev_mode = std::env::var("DEV_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        Self {
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| {
                if dev_mode {
                    "dev-secret-do-not-use-in-production".to_string()
                } else {
                    panic!("JWT_SECRET must be set in production")
                }
            }),
            dev_mode,
            dev_user_id: std::env::var("DEV_USER_ID")
                .ok()
                .and_then(|v| v.parse().ok()),
            public_url: std::env::var("PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            google_client_id: std::env::var("GOOGLE_CLIENT_ID").ok(),
            google_client_secret: std::env::var("GOOGLE_CLIENT_SECRET").ok(),
            allowed_email_domains: std::env::var("ALLOWED_EMAIL_DOMAINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            admin_emails: std::env::var("ADMIN_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            mail: mail::MailConfig::from_env(dev_mode),
        }
    }

    /// Whether `email` may perform destructive admin actions.
    ///
    /// Checked per request rather than baked into the JWT, so dropping someone
    /// from `ADMIN_EMAILS` revokes the privilege at the next request instead of
    /// whenever their 24h token happens to expire.
    ///
    /// Dev mode grants it unconditionally: it already bypasses authentication
    /// entirely, so an allowlist there would only be theatre.
    pub fn is_admin(&self, email: &str) -> bool {
        if self.dev_mode {
            return true;
        }
        let email = email.to_lowercase();
        self.admin_emails.contains(&email)
    }
}

fn establish_connection(
    config: &str,
) -> futures_util::future::BoxFuture<'_, diesel::ConnectionResult<AsyncPgConnection>> {
    // Strip channel_binding=require — PgBouncer (NeonDB pooler) doesn't support it
    let config = config.replace("&channel_binding=require", "");
    let fut = async move {
        let rustls_config = rustls::ClientConfig::with_platform_verifier()
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
        let (client, conn) = tokio_postgres::connect(&config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        AsyncPgConnection::try_from_client_and_connection(client, conn).await
    };
    fut.boxed()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--check-assets") {
        static_files::verify_assets_embedded();
        println!("Frontend assets OK");
        return Ok(());
    }

    // Load .env if present
    dotenvy::dotenv().ok();

    // Set up tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "action_tracker=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    static_files::verify_assets_embedded();

    // Pin the rustls crypto provider explicitly. Both the database TLS stack and
    // the SMTP mailer use rustls, and if a future dependency pulls in a second
    // provider rustls panics at first use rather than choosing. Failing here, at
    // startup, is better than failing on the first outbound connection.
    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
    {
        tracing::debug!("rustls crypto provider was already installed");
    }

    let config = AppConfig::from_env();

    if config.dev_mode {
        tracing::warn!("Running in DEV MODE - authentication is bypassed!");
    }

    // Database connection with TLS (required for NeonDB)
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let mut manager_config = ManagerConfig::default();
    manager_config.custom_setup = Box::new(establish_connection);

    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(
        database_url,
        manager_config,
    );
    let pool = Pool::builder(manager)
        .max_size(10)
        .build()
        .expect("Failed to create pool");

    // Verify database connectivity at startup
    {
        use diesel_async::RunQueryDsl;
        let mut conn = pool
            .get()
            .await
            .expect("Failed to connect to database at startup");
        diesel::sql_query("SELECT 1")
            .execute(&mut conn)
            .await
            .expect("Database health check failed");
        tracing::info!("Database connection verified");
    }

    let mailer = mail::build(&config.mail)?;

    let state = AppState {
        pool,
        config: config.clone(),
        mailer,
        mail_health: Arc::new(mail::MailHealthTracker::default()),
    };

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health::health_check))
        // Auth routes
        .route("/auth/login", get(auth::login))
        .route("/auth/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        // Vendor portal (read-only, magic-link authenticated)
        .route("/vendor/request-link", post(vendor_portal::request_link))
        .route("/vendor/verify", get(vendor_portal::verify))
        .route("/vendor/logout", post(vendor_portal::logout))
        .route("/vendor/api/me", get(vendor_portal::me))
        .route("/vendor/api/items", get(vendor_portal::list_items))
        // Vendor routes
        .route("/api/vendors", get(vendors::list).post(vendors::create))
        .route("/api/vendors/:id", get(vendors::get).patch(vendors::update))
        .route(
            "/api/vendors/:id/allowed-domains",
            get(vendor_portal::list_domains).post(vendor_portal::add_domain),
        )
        .route(
            "/api/vendors/:id/allowed-domains/:domain_id",
            axum::routing::delete(vendor_portal::delete_domain),
        )
        // Item routes
        .route("/api/items", get(items::list_all))
        .route(
            "/api/vendors/:id/items",
            get(items::list).post(items::create),
        )
        .route(
            "/api/items/:item_id",
            get(items::get).patch(items::update).delete(items::delete),
        )
        // Note routes
        .route(
            "/api/items/:item_id/notes",
            get(notes::list).post(notes::create),
        )
        // Status routes
        .route("/api/items/:item_id/history", get(status::history))
        .route("/api/items/:item_id/status", post(status::change))
        // User routes
        .route("/api/users", get(users::list))
        // Mail diagnostics
        .route("/api/mail/test", post(mail_test::send))
        .route("/api/mail/status", get(mail_test::status))
        // Category routes
        .route("/api/categories", get(categories::list_all))
        .route(
            "/api/vendors/:id/categories",
            get(categories::list_by_vendor).post(categories::create),
        )
        // Activity feed
        .route("/api/activity", get(activity::activity))
        // Deep link redirect
        .route("/go/:item_id", get(items::go_redirect))
        // Static files (frontend) - fallback for everything else
        .fallback(static_files::static_handler)
        .layer({
            let origin = config
                .public_url
                .parse::<axum::http::HeaderValue>()
                .expect("PUBLIC_URL must be a valid header value");
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PATCH,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([axum::http::header::CONTENT_TYPE])
        })
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod config_tests {
    use super::AppConfig;

    fn config(dev_mode: bool, admin_emails: &[&str]) -> AppConfig {
        AppConfig {
            jwt_secret: "test".to_string(),
            dev_mode,
            dev_user_id: None,
            public_url: "http://localhost:8080".to_string(),
            google_client_id: None,
            google_client_secret: None,
            allowed_email_domains: Vec::new(),
            admin_emails: admin_emails.iter().map(|e| e.to_string()).collect(),
            mail: crate::mail::MailConfig::from_env(true),
        }
    }

    #[test]
    fn an_empty_allowlist_admits_nobody() {
        // The default for a deployment that has not opted in. Getting this wrong
        // in the permissive direction would hand item deletion to every user.
        let config = config(false, &[]);
        assert!(!config.is_admin("matt@cosmicfrontier.org"));
    }

    #[test]
    fn allowlist_matching_ignores_address_case() {
        // Google returns the address in whatever case the profile carries, which
        // need not match how it was typed into the env var.
        let config = config(false, &["matt@cosmicfrontier.org"]);
        assert!(config.is_admin("Matt@CosmicFrontier.org"));
        assert!(!config.is_admin("someone@cosmicfrontier.org"));
    }

    #[test]
    fn dev_mode_is_admin_because_it_already_bypasses_auth() {
        let config = config(true, &[]);
        assert!(config.is_admin("anyone@localhost"));
    }
}
