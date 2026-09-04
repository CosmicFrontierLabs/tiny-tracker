//! Google Workspace SMTP transport.
//!
//! Expects a dedicated licensed Workspace user (aliases and groups cannot hold an
//! app password) with 2-step verification on and an app password issued to it.
//! Enable DKIM for the sending subdomain separately in the Admin Console — it is
//! configured per-domain and is not inherited from the primary domain.

use axum::async_trait;
use lettre::message::{header::ContentType, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use super::{Email, MailConfig, Mailer};

pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    pub fn new(config: &MailConfig) -> anyhow::Result<Self> {
        let user = config
            .smtp_user
            .clone()
            .ok_or_else(|| anyhow::anyhow!("SMTP_USER must be set when MAIL_BACKEND=smtp"))?;
        let password = config
            .smtp_password
            .clone()
            .ok_or_else(|| anyhow::anyhow!("SMTP_PASSWORD must be set when MAIL_BACKEND=smtp"))?;

        let from: Mailbox = config
            .from
            .parse()
            .map_err(|e| anyhow::anyhow!("MAIL_FROM is not a valid address: {e}"))?;

        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| anyhow::anyhow!("Failed to configure SMTP relay: {e}"))?
            .port(config.smtp_port)
            .credentials(Credentials::new(user, password))
            .build();

        Ok(Self { transport, from })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, email: Email) -> anyhow::Result<()> {
        let to: Mailbox = email
            .to
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid recipient address: {e}"))?;

        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(email.subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(email.text),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(email.html),
                    ),
            )?;

        self.transport.send(message).await?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "smtp"
    }
}
