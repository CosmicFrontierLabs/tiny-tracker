mod db;
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
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use routes::{auth, categories, health, items, notes, status, users, vendors};

pub type DbPool = Pool<AsyncPgConnection>;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub config: AppConfig,
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
        }
    }
}

fn establish_connection(
    config: &str,
) -> futures_util::future::BoxFuture<'_, diesel::ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        let rustls_config = rustls::ClientConfig::with_platform_verifier()
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
        let (client, conn) = tokio_postgres::connect(config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        AsyncPgConnection::try_from_client_and_connection(client, conn).await
    };
    fut.boxed()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present
    dotenvy::dotenv().ok();

    // Set up tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    let state = AppState {
        pool,
        config: config.clone(),
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
        // Vendor routes
        .route("/api/vendors", get(vendors::list).post(vendors::create))
        .route("/api/vendors/:id", get(vendors::get).patch(vendors::update))
        // Item routes
        .route("/api/items", get(items::list_all))
        .route(
            "/api/vendors/:id/items",
            get(items::list).post(items::create),
        )
        .route("/api/items/:item_id", get(items::get).patch(items::update))
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
        // Category routes
        .route("/api/categories", get(categories::list_all))
        .route(
            "/api/vendors/:id/categories",
            get(categories::list_by_vendor).post(categories::create),
        )
        // Deep link redirect
        .route("/go/:item_id", get(items::go_redirect))
        // Static files (frontend) - fallback for everything else
        .fallback(static_files::static_handler)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
