use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Deserialize;
use std::sync::Arc;

use crate::db::schema::users;
use crate::models::{NewUser, User};
use crate::AppState;

use super::{AuthUser, Claims};

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    #[allow(dead_code)]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    email: String,
    name: String,
}

pub async fn login(State(state): State<Arc<AppState>>) -> Response {
    if state.config.dev_mode {
        // In dev mode, just redirect to callback with a fake code
        return Redirect::to("/auth/callback?code=dev").into_response();
    }

    let client_id = match &state.config.google_client_id {
        Some(id) => id,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth not configured").into_response(),
    };

    let redirect_uri = format!("{}/auth/callback", state.config.public_url);
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope=email%20profile&\
        access_type=offline",
        client_id,
        urlencoding::encode(&redirect_uri)
    );

    Redirect::to(&auth_url).into_response()
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if state.config.dev_mode {
        // Dev mode: create a token for the dev user
        let mut conn = match state.pool.get().await {
            Ok(c) => c,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        };

        // Get or create dev user
        let dev_user: User = match users::table
            .filter(users::email.eq("dev@localhost"))
            .first(&mut conn)
            .await
        {
            Ok(user) => user,
            Err(diesel::NotFound) => {
                // Create dev user
                let new_user = NewUser {
                    email: "dev@localhost".to_string(),
                    name: "Dev User".to_string(),
                    initials: Some("DV".to_string()),
                };
                diesel::insert_into(users::table)
                    .values(&new_user)
                    .returning(User::as_returning())
                    .get_result(&mut conn)
                    .await
                    .unwrap()
            }
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
        };

        let token = create_jwt(&state.config.jwt_secret, &dev_user);
        return set_token_cookie_and_redirect(token);
    }

    // Exchange code for token
    let client_id = match &state.config.google_client_id {
        Some(id) => id,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth not configured").into_response(),
    };
    let client_secret = match &state.config.google_client_secret {
        Some(secret) => secret,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth not configured").into_response(),
    };

    let redirect_uri = format!("{}/auth/callback", state.config.public_url);

    let client = reqwest::Client::new();
    let token_response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", &redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let token_response: GoogleTokenResponse = match token_response {
        Ok(resp) => match resp.json().await {
            Ok(t) => t,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "Failed to parse token response").into_response()
            }
        },
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Failed to exchange code for token").into_response()
        }
    };

    // Get user info
    let user_info: GoogleUserInfo = match client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token_response.access_token)
        .send()
        .await
    {
        Ok(resp) => match resp.json().await {
            Ok(info) => info,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "Failed to parse user info").into_response()
            }
        },
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to get user info").into_response(),
    };

    // Check email domain
    if !state.config.allowed_email_domains.is_empty() {
        let domain = user_info.email.rsplit('@').next().unwrap_or("");
        if !state
            .config
            .allowed_email_domains
            .contains(&domain.to_string())
        {
            return (StatusCode::FORBIDDEN, "Email domain not allowed").into_response();
        }
    }

    // Get or create user
    let mut conn = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get database connection: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database connection error",
            )
                .into_response();
        }
    };

    tracing::info!("Looking up user by email: {}", user_info.email);

    let user: User = match users::table
        .filter(users::email.eq(&user_info.email))
        .first::<User>(&mut conn)
        .await
    {
        Ok(user) => {
            tracing::info!("Found existing user: id={}", user.id);
            user
        }
        Err(diesel::NotFound) => {
            tracing::info!("User not found, creating new user for {}", user_info.email);
            // Create new user
            let initials = user_info
                .name
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(2)
                .collect::<String>()
                .to_uppercase();

            let new_user = NewUser {
                email: user_info.email.clone(),
                name: user_info.name.clone(),
                initials: Some(initials),
            };

            match diesel::insert_into(users::table)
                .values(&new_user)
                .returning(User::as_returning())
                .get_result(&mut conn)
                .await
            {
                Ok(user) => {
                    tracing::info!("Created new user: id={}", user.id);
                    user
                }
                Err(e) => {
                    tracing::error!("Failed to create user: {e}");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create user: {e}"),
                    )
                        .into_response();
                }
            }
        }
        Err(e) => {
            tracing::error!("Database query error looking up user: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {e}"),
            )
                .into_response();
        }
    };

    let token = create_jwt(&state.config.jwt_secret, &user);
    set_token_cookie_and_redirect(token)
}

pub async fn logout() -> Response {
    let cookie = "token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(shared::LogoutResponse {
            status: "logged out".to_string(),
        }),
    )
        .into_response()
}

pub async fn me(auth_user: AuthUser) -> Json<shared::CurrentUserResponse> {
    Json(shared::CurrentUserResponse {
        user_id: auth_user.user_id,
        email: auth_user.email,
        name: auth_user.name,
    })
}

fn create_jwt(secret: &str, user: &User) -> String {
    let now = Utc::now();
    let exp = now + Duration::hours(24);

    let claims = Claims {
        sub: user.email.clone(),
        name: user.name.clone(),
        user_id: user.id,
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to create JWT")
}

fn set_token_cookie_and_redirect(token: String) -> Response {
    let cookie = format!(
        "token={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400",
        token
    );
    (
        StatusCode::FOUND,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/".to_string()),
        ],
    )
        .into_response()
}
