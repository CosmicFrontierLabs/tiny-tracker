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

        // Validate at startup so a malformed MAIL_FROM fails here rather than as
        // an opaque API rejection on the first send. The parsed value is
        // discarded: Resend wants the raw header string, and the address must
        // belong to a domain verified in Resend regardless.
        config
            .from
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| anyhow::anyhow!("MAIL_FROM is not a valid address: {e}"))?;

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

#[cfg(test)]
mod tests {
    use super::ResendRequest;

    /// Pins the wire shape of the Resend payload.
    ///
    /// Every field name here is one Resend's API dictates, and a typo in any of
    /// them fails only at runtime, against a live key, with a 4xx that would be
    /// easy to misread as a credential or domain-verification problem. `to` in
    /// particular must serialise as an array, not a bare string.
    #[test]
    fn serialises_to_the_shape_resend_expects() {
        let request = ResendRequest {
            from: "Tracker <notifications@tracker.example.org>",
            to: ["vendor@acme.com"],
            subject: "Action Tracker test email",
            html: "<p>hello</p>",
            text: "hello",
        };

        let json: serde_json::Value = serde_json::to_value(&request).unwrap();

        assert_eq!(json["from"], "Tracker <notifications@tracker.example.org>");
        assert_eq!(json["subject"], "Action Tracker test email");
        assert_eq!(json["html"], "<p>hello</p>");
        assert_eq!(json["text"], "hello");

        // Array, single element — not a string.
        assert!(json["to"].is_array(), "`to` must serialise as an array");
        assert_eq!(json["to"][0], "vendor@acme.com");
        assert_eq!(json["to"].as_array().unwrap().len(), 1);

        // Exactly these fields, no more and no fewer. Compared as a sorted set
        // because `serde_json::Value` orders keys alphabetically, and key order
        // carries no meaning in JSON anyway.
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["from", "html", "subject", "text", "to"]);
    }
}
