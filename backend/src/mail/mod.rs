//! Outbound transactional email.
//!
//! The only message the app sends today is the vendor portal magic link, but the
//! transport is behind a trait so the backend can be swapped with one env var.
//! `MAIL_BACKEND` selects the implementation:
//!
//! - `smtp`   — Google Workspace SMTP (`smtp.gmail.com:587` + app password)
//! - `resend` — Resend's HTTP API, for delivery logs and bounce webhooks
//! - `log`    — writes the message to the tracing log instead of sending it
//!
//! `log` is the default in dev mode so magic links can be copied out of the
//! server output without any mail credentials.

use axum::async_trait;

mod log_mailer;
mod resend;
mod smtp;

pub use log_mailer::LogMailer;
pub use resend::ResendMailer;
pub use smtp::SmtpMailer;

/// A message to send. Both bodies are always supplied; transports that only
/// support one should prefer `html`.
pub struct Email {
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: Email) -> anyhow::Result<()>;

    /// Short transport id (`smtp`, `resend`, `log`). Shown to staff in the mail
    /// test dialog, so keep it terse.
    fn name(&self) -> &'static str;

    /// False when the transport only pretends to send. Callers must surface this,
    /// or the `log` backend makes a test look successful while delivering nothing.
    fn delivers(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailBackend {
    Smtp,
    Resend,
    Log,
}

impl MailBackend {
    pub fn from_env(dev_mode: bool) -> Self {
        match std::env::var("MAIL_BACKEND")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "smtp" => MailBackend::Smtp,
            "resend" => MailBackend::Resend,
            "log" => MailBackend::Log,
            "" if dev_mode => MailBackend::Log,
            "" => MailBackend::Smtp,
            other => panic!("Unknown MAIL_BACKEND '{other}' (expected smtp, resend, or log)"),
        }
    }
}

/// Settings shared by every transport, read once at startup.
#[derive(Clone)]
pub struct MailConfig {
    pub backend: MailBackend,
    /// RFC 5322 From header, e.g. `Tracker <notifications@tracker.example.org>`.
    pub from: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub resend_api_key: Option<String>,
}

impl MailConfig {
    pub fn from_env(dev_mode: bool) -> Self {
        Self {
            backend: MailBackend::from_env(dev_mode),
            from: std::env::var("MAIL_FROM")
                .unwrap_or_else(|_| "Action Tracker <noreply@localhost>".to_string()),
            smtp_host: std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string()),
            smtp_port: std::env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(587),
            smtp_user: std::env::var("SMTP_USER").ok().filter(|s| !s.is_empty()),
            smtp_password: std::env::var("SMTP_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty()),
            resend_api_key: std::env::var("RESEND_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }
}

/// Construct the configured transport, failing fast if its credentials are absent.
pub fn build(config: &MailConfig) -> anyhow::Result<std::sync::Arc<dyn Mailer>> {
    let mailer: std::sync::Arc<dyn Mailer> = match config.backend {
        MailBackend::Smtp => std::sync::Arc::new(SmtpMailer::new(config)?),
        MailBackend::Resend => std::sync::Arc::new(ResendMailer::new(config)?),
        MailBackend::Log => std::sync::Arc::new(LogMailer::new(config)),
    };
    if mailer.delivers() {
        tracing::info!("Mail transport: {}", mailer.name());
    } else {
        tracing::warn!(
            "Mail transport: {} - messages are written to the log, NOT delivered",
            mailer.name()
        );
    }
    Ok(mailer)
}
