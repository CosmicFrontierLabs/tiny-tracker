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
    activity, auth, categories, health, items, mail_test, notes, risk_activity, risks, status,
    users, vendor_portal, vendors,
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
            jwt_secret: resolve_jwt_secret(
                std::env::var("JWT_SECRET").ok(),
                std::env::var_os("JWT_SECRET_FILE").map(std::path::PathBuf::from),
                dev_mode,
            )
            .unwrap_or_else(|e| panic!("{e}")),
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

const DEV_JWT_SECRET: &str = "dev-secret-do-not-use-in-production";

/// Resolve the JWT signing key from `JWT_SECRET` or `JWT_SECRET_FILE`.
///
/// The file form exists so the key can be bind-mounted read-only instead of
/// living in the container environment (where `docker inspect` shows it and
/// an auto-updater copies it on every recreate). Exactly one trailing `\n` or
/// `\r\n` is stripped, so a file written with `echo` holds the same key as
/// the env var did: the result must be byte-identical, or every staff session
/// and outstanding vendor magic link is invalidated at cutover.
///
/// A configured file that cannot be used is fatal in every mode, including dev
/// mode: silently falling back to the dev key would be worse than not starting.
///
/// Errors never contain the key or its length.
fn resolve_jwt_secret(
    env_secret: Option<String>,
    secret_file: Option<std::path::PathBuf>,
    dev_mode: bool,
) -> Result<String, String> {
    match (env_secret, secret_file) {
        (Some(_), Some(_)) => {
            Err("JWT_SECRET and JWT_SECRET_FILE are both set; set exactly one".to_string())
        }
        (Some(secret), None) => Ok(secret),
        (None, Some(path)) => read_jwt_secret_file(&path),
        (None, None) if dev_mode => Ok(DEV_JWT_SECRET.to_string()),
        (None, None) => Err("JWT_SECRET must be set in production".to_string()),
    }
}

fn read_jwt_secret_file(path: &std::path::Path) -> Result<String, String> {
    let shown = path.display();
    let bytes = std::fs::read(path)
        .map_err(|e| format!("JWT_SECRET_FILE {shown} could not be read: {e}"))?;
    let mut secret = String::from_utf8(bytes)
        .map_err(|_| format!("JWT_SECRET_FILE {shown} is not valid UTF-8"))?;
    if secret.ends_with("\r\n") {
        secret.truncate(secret.len() - 2);
    } else if secret.ends_with('\n') {
        secret.truncate(secret.len() - 1);
    }
    if secret.is_empty() {
        return Err(format!("JWT_SECRET_FILE {shown} is empty"));
    }
    Ok(secret)
}

fn establish_connection(
    config: &str,
) -> futures_util::future::BoxFuture<'_, diesel::ConnectionResult<AsyncPgConnection>> {
    // Some connection poolers reject channel_binding=require; drop it so the
    // same URL works with or without one in front of the database.
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

    // Database connection with TLS support. Managed Postgres requires it; a
    // plain local Postgres without TLS falls back to cleartext (sslmode prefer).
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
        // Risk register
        .route("/api/risks", get(risks::list_all))
        .route(
            "/api/vendors/:id/risks",
            get(risks::list_by_vendor).post(risks::create),
        )
        .route(
            "/api/risks/:risk_id",
            get(risks::get).patch(risks::update).delete(risks::delete),
        )
        .route(
            "/api/risks/:risk_id/items",
            get(risks::items_for_risk).post(risks::link_items),
        )
        .route(
            "/api/risks/:risk_id/notes",
            get(risk_activity::list_notes).post(risk_activity::create_note),
        )
        .route("/api/risks/:risk_id/history", get(risk_activity::history))
        .route(
            "/api/items/:item_id/risks",
            get(risks::risks_for_item).post(risks::link),
        )
        .route(
            "/api/items/:item_id/risks/:risk_id",
            axum::routing::delete(risks::unlink),
        )
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
        assert!(!config.is_admin("admin@example.org"));
    }

    #[test]
    fn allowlist_matching_ignores_address_case() {
        // Google returns the address in whatever case the profile carries, which
        // need not match how it was typed into the env var.
        let config = config(false, &["admin@example.org"]);
        assert!(config.is_admin("Admin@Example.org"));
        assert!(!config.is_admin("someone@example.org"));
    }

    #[test]
    fn dev_mode_is_admin_because_it_already_bypasses_auth() {
        let config = config(true, &[]);
        assert!(config.is_admin("anyone@localhost"));
    }
}

#[cfg(test)]
mod jwt_secret_tests {
    use super::{resolve_jwt_secret, DEV_JWT_SECRET};
    use std::path::PathBuf;

    /// A uniquely named temp file holding `contents`, removed on drop.
    struct TempKey(PathBuf);

    impl TempKey {
        fn new(name: &str, contents: &[u8]) -> Self {
            let path = std::env::temp_dir()
                .join(format!("tiny-tracker-jwt-{}-{name}", std::process::id()));
            std::fs::write(&path, contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TempKey {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn from_file(name: &str, contents: &[u8], dev_mode: bool) -> Result<String, String> {
        let file = TempKey::new(name, contents);
        resolve_jwt_secret(None, Some(file.0.clone()), dev_mode)
    }

    #[test]
    fn file_contents_are_the_secret() {
        assert_eq!(from_file("plain", b"s3cret", false).unwrap(), "s3cret");
    }

    #[test]
    fn file_matches_env_var_byte_for_byte_after_one_newline() {
        // Cutover writes the current env value into the file; it must sign
        // identically or every session and magic link dies.
        let env = resolve_jwt_secret(Some("abc DEF=+/".into()), None, false).unwrap();
        assert_eq!(from_file("lf", b"abc DEF=+/\n", false).unwrap(), env);
        assert_eq!(from_file("crlf", b"abc DEF=+/\r\n", false).unwrap(), env);
    }

    #[test]
    fn only_one_trailing_newline_is_stripped() {
        assert_eq!(from_file("two-lf", b"key\n\n", false).unwrap(), "key\n");
        assert_eq!(from_file("lead", b" key \n", false).unwrap(), " key ");
    }

    #[test]
    fn both_set_is_an_error() {
        let file = TempKey::new("both", b"from-file");
        for dev_mode in [false, true] {
            let err = resolve_jwt_secret(Some("from-env".into()), Some(file.0.clone()), dev_mode)
                .unwrap_err();
            assert!(err.contains("both set"), "{err}");
        }
    }

    #[test]
    fn missing_file_is_an_error_even_in_dev_mode() {
        let path = std::env::temp_dir().join("tiny-tracker-jwt-definitely-missing");
        for dev_mode in [false, true] {
            assert!(resolve_jwt_secret(None, Some(path.clone()), dev_mode).is_err());
        }
    }

    #[test]
    fn empty_file_is_an_error_even_in_dev_mode() {
        for dev_mode in [false, true] {
            assert!(from_file("empty", b"", dev_mode).is_err());
            assert!(from_file("only-lf", b"\n", dev_mode).is_err());
            assert!(from_file("only-crlf", b"\r\n", dev_mode).is_err());
        }
    }

    #[test]
    fn non_utf8_file_is_an_error() {
        assert!(from_file("binary", &[0xff, 0xfe, 0x00], false).is_err());
    }

    #[test]
    fn errors_do_not_leak_the_secret() {
        let file = TempKey::new("leak", b"hunter2-super-secret");
        let err = resolve_jwt_secret(
            Some("hunter2-super-secret".into()),
            Some(file.0.clone()),
            false,
        )
        .unwrap_err();
        assert!(!err.contains("hunter2"));
    }

    #[test]
    fn neither_set_keeps_existing_behaviour() {
        assert!(resolve_jwt_secret(None, None, false).is_err());
        assert_eq!(
            resolve_jwt_secret(None, None, true).unwrap(),
            DEV_JWT_SECRET
        );
    }
}
