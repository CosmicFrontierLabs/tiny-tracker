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
        let from: Mailbox = config
            .from
            .parse()
            .map_err(|e| anyhow::anyhow!("MAIL_FROM is not a valid address: {e}"))?;

        // Credentials are optional: Google Workspace's SMTP relay
        // (smtp-relay.gmail.com) can authorise by static egress IP instead, in
        // which case there is no credential to supply. Requiring one would make
        // that deployment impossible.
        let credentials = match (&config.smtp_user, &config.smtp_password) {
            (Some(user), Some(password)) => Some(Credentials::new(user.clone(), password.clone())),
            (None, None) => {
                tracing::info!(
                    "SMTP configured without credentials; relying on IP-based \
                     authorisation at {}",
                    config.smtp_host
                );
                None
            }
            // One without the other is a typo, not a deliberate mode.
            (Some(_), None) => anyhow::bail!(
                "SMTP_USER is set but SMTP_PASSWORD is not. Set both for authenticated \
                 SMTP, or neither to use IP-authorised relay."
            ),
            (None, Some(_)) => anyhow::bail!(
                "SMTP_PASSWORD is set but SMTP_USER is not. Set both for authenticated \
                 SMTP, or neither to use IP-authorised relay."
            ),
        };

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| anyhow::anyhow!("Failed to configure SMTP relay: {e}"))?
            .port(config.smtp_port);

        if let Some(credentials) = credentials {
            builder = builder.credentials(credentials);
        }

        Ok(Self {
            transport: builder.build(),
            from,
        })
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
