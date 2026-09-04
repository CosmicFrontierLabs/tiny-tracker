//! Development transport: logs the message instead of sending it.
//!
//! The full body is logged at INFO so a magic link can be copied straight out of
//! the server output. That obviously leaks the link into the logs, which is why
//! `build` refuses to select this transport outside dev mode unless the operator
//! asks for it explicitly with `MAIL_BACKEND=log`.

use axum::async_trait;

use super::{Email, MailConfig, Mailer};

pub struct LogMailer {
    from: String,
}

impl LogMailer {
    pub fn new(config: &MailConfig) -> Self {
        Self {
            from: config.from.clone(),
        }
    }
}

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, email: Email) -> anyhow::Result<()> {
        tracing::info!(
            "\n--- email (not sent; MAIL_BACKEND=log) ---\nFrom: {}\nTo: {}\nSubject: {}\n\n{}\n--- end email ---",
            self.from,
            email.to,
            email.subject,
            email.text
        );
        Ok(())
    }

    fn name(&self) -> &'static str {
        "log (messages are written to the log, not delivered)"
    }
}
