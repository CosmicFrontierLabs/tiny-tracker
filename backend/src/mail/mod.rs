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
use chrono::{DateTime, Utc};
use std::sync::RwLock;

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
    /// Domain to send in the SMTP EHLO command. Defaults to the `MAIL_FROM`
    /// domain, which is what Google's relay wants to see.
    pub smtp_helo_name: Option<String>,
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
            smtp_helo_name: std::env::var("SMTP_HELO_NAME")
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

/// Records the outcome of the most recent send attempt.
///
/// An unverified sending domain produces a perfectly healthy container that
/// rejects every message — no crash, no restart, and with no bounce webhook, no
/// signal anywhere. This exists so that state is visible in the UI the first
/// time anyone tries to send, rather than discovered by a vendor who never got
/// their link.
#[derive(Default)]
pub struct MailHealthTracker {
    last: RwLock<Option<Attempt>>,
}

struct Attempt {
    at: DateTime<Utc>,
    /// `None` on success.
    error: Option<String>,
}

impl MailHealthTracker {
    pub fn record_success(&self) {
        self.write(None);
    }

    pub fn record_failure(&self, error: impl Into<String>) {
        self.write(Some(error.into()));
    }

    fn write(&self, error: Option<String>) {
        if let Ok(mut guard) = self.last.write() {
            *guard = Some(Attempt {
                at: Utc::now(),
                error,
            });
        }
    }

    /// Current status. `delivers` comes from the active transport, so the `log`
    /// backend reports NotDelivering regardless of how many sends "succeeded".
    pub fn status(&self, backend: &str, delivers: bool) -> shared::MailStatus {
        let guard = self.last.read().ok();
        let last = guard.as_ref().and_then(|g| g.as_ref());

        let health = if !delivers {
            shared::MailHealth::NotDelivering
        } else {
            match last {
                None => shared::MailHealth::Untested,
                Some(a) if a.error.is_some() => shared::MailHealth::Failing,
                Some(_) => shared::MailHealth::Ok,
            }
        };

        shared::MailStatus {
            backend: backend.to_string(),
            health,
            last_attempt_at: last.map(|a| a.at),
            last_error: last.and_then(|a| a.error.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MailHealthTracker;
    use shared::MailHealth;

    #[test]
    fn a_real_backend_with_no_attempts_is_untested_not_ok() {
        let tracker = MailHealthTracker::default();
        let status = tracker.status("resend", true);
        assert_eq!(status.health, MailHealth::Untested);
        assert!(status.last_attempt_at.is_none());
    }

    #[test]
    fn the_log_backend_never_reports_ok_however_many_sends_succeed() {
        // The masking that matters: the log backend "succeeds" every time while
        // delivering nothing, so transport capability must override attempt
        // history rather than the other way round.
        let tracker = MailHealthTracker::default();
        tracker.record_success();
        tracker.record_success();
        let status = tracker.status("log", false);
        assert_eq!(status.health, MailHealth::NotDelivering);
        // The attempt is still recorded, just not treated as evidence of health.
        assert!(status.last_attempt_at.is_some());
    }

    #[test]
    fn a_failure_surfaces_the_verbatim_error() {
        let tracker = MailHealthTracker::default();
        tracker.record_failure("403 domain is not verified");
        let status = tracker.status("resend", true);
        assert_eq!(status.health, MailHealth::Failing);
        assert_eq!(
            status.last_error.as_deref(),
            Some("403 domain is not verified")
        );
    }

    #[test]
    fn success_reports_ok_and_clears_the_previous_error() {
        let tracker = MailHealthTracker::default();
        tracker.record_failure("403 domain is not verified");
        tracker.record_success();
        let status = tracker.status("resend", true);
        assert_eq!(status.health, MailHealth::Ok);
        assert!(status.last_error.is_none());
    }

    #[test]
    fn a_later_failure_overrides_an_earlier_success() {
        let tracker = MailHealthTracker::default();
        tracker.record_success();
        tracker.record_failure("timed out");
        let status = tracker.status("smtp", true);
        assert_eq!(status.health, MailHealth::Failing);
        assert_eq!(status.last_error.as_deref(), Some("timed out"));
    }
}
