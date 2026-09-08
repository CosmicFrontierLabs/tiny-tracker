//! Google Workspace SMTP transport, in either of two authorisation modes.
//!
//! **IP-authorised relay** (`SMTP_USER`/`SMTP_PASSWORD` both unset): point
//! `SMTP_HOST` at `smtp-relay.gmail.com` and allowlist the sending host's static
//! egress IP under Admin → Apps → Google Workspace → Gmail → Routing → SMTP
//! relay service. No credential is involved, so the sender address only has to
//! be one the relay accepts for the domain — an alias is fine.
//!
//! **Authenticated SMTP** (both set): `SMTP_USER` must be a dedicated *licensed*
//! Workspace user with 2-step verification on and an app password issued to it.
//! Aliases and groups cannot hold an app password, so they cannot be used in
//! this mode — but that restriction is specific to app passwords and does not
//! apply to the relay mode above.
//!
//! Either way, enable DKIM for the sending subdomain separately in the Admin
//! Console — it is configured per-domain and is not inherited from the primary
//! domain.

use axum::async_trait;
use lettre::message::{header::ContentType, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::extension::ClientId;
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

        // EHLO name. lettre defaults to `ClientId::hostname()`, which inside a
        // container is the container ID — a hex string, not a domain. Google's
        // relay names the EHLO domain as one of the two ways it identifies a
        // sending domain, so leaving it as a container ID risks a 550 on exactly
        // the credential-less path where SMTP AUTH is not there to identify us.
        // Default to the MAIL_FROM domain, which is the right answer nearly
        // always, and let an operator override it.
        let helo_name = match &config.smtp_helo_name {
            Some(name) => {
                validate_helo_name(name)?;
                name.clone()
            }
            None => from.email.domain().to_string(),
        };
        tracing::info!("SMTP EHLO name: {helo_name}");

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| anyhow::anyhow!("Failed to configure SMTP relay: {e}"))?
            .port(config.smtp_port)
            .hello_name(ClientId::Domain(helo_name));

        if let Some(credentials) = credentials {
            builder = builder.credentials(credentials);
        }

        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

/// Reject an EHLO name that is obviously not a domain, at startup rather than on
/// first send.
fn validate_helo_name(name: &str) -> anyhow::Result<()> {
    if name.contains(' ') || name.contains('@') || !name.contains('.') {
        anyhow::bail!(
            "SMTP_HELO_NAME must be a bare domain such as tracker.example.org, got {name:?}"
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::validate_helo_name;

    #[test]
    fn accepts_bare_domains() {
        assert!(validate_helo_name("cosmicfrontier.org").is_ok());
        assert!(validate_helo_name("tracker.cosmicfrontier.org").is_ok());
    }

    #[test]
    fn rejects_non_domains() {
        // A bare hostname is the failure this guard exists for: lettre's default
        // is the container ID, which has no dot.
        assert!(validate_helo_name("a1b2c3d4e5f6").is_err());
        assert!(validate_helo_name("not a domain").is_err());
        assert!(validate_helo_name("user@example.org").is_err());
        assert!(validate_helo_name("").is_err());
    }
}
