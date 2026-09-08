//! Resend HTTP API transport.
//!
//! Requires SPF, DKIM and DMARC records on the sending subdomain. Because those
//! records live on a subdomain, they do not disturb the Workspace records on the
//! apex domain.

use axum::async_trait;
use serde::Serialize;

use super::{Email, MailConfig, Mailer};

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

pub struct ResendMailer {
    client: reqwest::Client,
    api_key: String,
    from: String,
}

#[derive(Serialize)]
struct ResendRequest<'a> {
    from: &'a str,
    to: [&'a str; 1],
    subject: &'a str,
    html: &'a str,
    text: &'a str,
}

impl ResendMailer {
    pub fn new(config: &MailConfig) -> anyhow::Result<Self> {
        let api_key = config.resend_api_key.clone().ok_or_else(|| {
            anyhow::anyhow!("RESEND_API_KEY must be set when MAIL_BACKEND=resend")
        })?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            from: config.from.clone(),
        })
    }
}

#[async_trait]
impl Mailer for ResendMailer {
    async fn send(&self, email: Email) -> anyhow::Result<()> {
        let body = ResendRequest {
            from: &self.from,
            to: [&email.to],
            subject: &email.subject,
            html: &email.html,
            text: &email.text,
        };

        let response = self
            .client
            .post(RESEND_ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            anyhow::bail!("Resend rejected the message ({status}): {detail}");
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "resend"
    }
}
