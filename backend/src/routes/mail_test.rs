//! Staff-only mail diagnostics.
//!
//! Sends a fixed test message to an address the operator types in, so the mail
//! transport can be verified from the UI instead of by waiting for a vendor to
//! report that no link arrived.
//!
//! Unlike the vendor portal's send, this awaits delivery and returns the
//! transport's own error text: the whole point is to see *why* a misconfigured
//! SMTP relay or Resend key is failing.
//!
//! The message body is fixed and the endpoint requires a staff session, so it is
//! not a vector for sending arbitrary content. Every call is logged with the
//! requesting user for auditability.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use shared::{ApiError, SendTestEmail, TestEmailResponse};
use std::sync::Arc;

use crate::mail::Email;
use crate::AppState;

use super::AuthUser;

pub async fn send(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<SendTestEmail>,
) -> Response {
    let recipient = payload.email.trim().to_string();

    if super::vendor_portal::email_domain(&recipient).is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation_error(
                "Enter a valid email address, e.g. you@example.com",
            )),
        )
            .into_response();
    }

    let mailer = &state.mailer;
    let backend = mailer.name().to_string();
    let delivers = mailer.delivers();

    tracing::info!(
        "Mail test requested by {} -> {} via {}",
        auth.email,
        recipient,
        backend
    );

    let body_text = format!(
        "This is a test message from the Cosmic Frontier action tracker.\n\n\
         Requested by: {}\nTransport: {}\n\n\
         If you received this, outbound email is working and vendor portal \
         sign-in links will reach this address.",
        auth.email, backend
    );

    let email = Email {
        to: recipient.clone(),
        subject: "Action Tracker test email".to_string(),
        html: format!("<p>{}</p>", body_text.replace('\n', "<br>")),
        text: body_text,
    };

    match mailer.send(email).await {
        Ok(()) => Json(TestEmailResponse {
            recipient,
            backend,
            delivered: delivers,
        })
        .into_response(),
        Err(e) => {
            tracing::error!("Mail test to {} failed: {e:#}", recipient);
            // Surface the transport error verbatim — diagnosing the transport is
            // the reason this endpoint exists, and it is staff-only.
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(
                    "MAIL_SEND_FAILED",
                    format!("{backend} transport rejected the message: {e}"),
                )),
            )
                .into_response()
        }
    }
}
