//! Service tokens: static bearer credentials for non-browser clients.
//!
//! Staff sign in through Google and get a 24h JWT cookie. That is the wrong
//! shape for a script or an agent: it cannot complete an OAuth redirect, and
//! a token that expires daily is a token that is silently broken most of the
//! time. So `API_TOKENS` (or `API_TOKENS_FILE`) carries a list of long-lived
//! secrets, each bound to an email address. A request bearing one acts as
//! that user — the same `AuthUser` the handlers already take, so every rule
//! the handlers enforce applies unchanged, and every note, status change and
//! risk edit is attributed to the service identity rather than to whichever
//! human's cookie happened to be lying around.
//!
//! Tokens are compared in constant time and never logged. Admin privilege
//! is not a property of the token: it comes from `ADMIN_EMAILS`, same as for
//! a person, so a token's reach can be widened or narrowed without reissuing
//! it.
//!
//! Each line (or comma-separated entry) is `email=secret`. One email may
//! carry several secrets, which is how a token is rotated without a gap:
//! add the new one, move the client over, remove the old one.

use std::path::Path;

use subtle::ConstantTimeEq;

/// Shortest secret accepted. 32 characters of a random alphabet is
/// comfortably beyond brute force; anything shorter is probably a
/// placeholder someone forgot to replace.
pub const MIN_SECRET_LEN: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiToken {
    email: String,
    secret: String,
}

/// The configured service tokens. Empty means the bearer path admits nobody.
#[derive(Clone, Debug, Default)]
pub struct ApiTokens {
    tokens: Vec<ApiToken>,
}

impl ApiTokens {
    /// Resolve from `API_TOKENS` or `API_TOKENS_FILE`, following the same
    /// rules as the JWT secret: at most one of the two may be set, and a
    /// configured file that cannot be read is fatal rather than silently
    /// admitting nobody.
    pub fn resolve(env_value: Option<String>, file: Option<&Path>) -> Result<Self, String> {
        match (env_value, file) {
            (Some(_), Some(_)) => {
                Err("API_TOKENS and API_TOKENS_FILE are both set; set at most one".to_string())
            }
            (Some(value), None) => Self::parse(&value),
            (None, Some(path)) => {
                let shown = path.display();
                let bytes = std::fs::read(path)
                    .map_err(|e| format!("API_TOKENS_FILE {shown} could not be read: {e}"))?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("API_TOKENS_FILE {shown} is not valid UTF-8"))?;
                Self::parse(&text).map_err(|e| format!("API_TOKENS_FILE {shown}: {e}"))
            }
            (None, None) => Ok(Self::default()),
        }
    }

    /// Parse `email=secret` entries separated by commas or newlines. Blank
    /// entries and `#` comment lines are ignored, so a mounted file can be
    /// annotated with who holds each token.
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut tokens: Vec<ApiToken> = Vec::new();

        for raw in source.split([',', '\n']) {
            let entry = raw.trim().trim_end_matches('\r');
            if entry.is_empty() || entry.starts_with('#') {
                continue;
            }

            let Some((email, secret)) = entry.split_once('=') else {
                return Err(format!(
                    "API token entry for '{}' is not of the form email=secret",
                    redact(entry)
                ));
            };
            let email = email.trim().to_lowercase();
            let secret = secret.trim();

            if email.is_empty() || !email.contains('@') || email.chars().any(char::is_whitespace) {
                return Err(format!("API token entry has an invalid email '{}'", email));
            }
            if secret.chars().count() < MIN_SECRET_LEN {
                return Err(format!(
                    "API token for {email} is shorter than {MIN_SECRET_LEN} characters"
                ));
            }
            if secret.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(format!(
                    "API token for {email} contains whitespace or control characters"
                ));
            }
            if tokens.iter().any(|t| t.secret == secret) {
                return Err(format!(
                    "API token for {email} duplicates another entry's secret"
                ));
            }

            tokens.push(ApiToken {
                email,
                secret: secret.to_string(),
            });
        }

        Ok(Self { tokens })
    }

    /// The email bound to `presented`, if it matches a configured secret.
    ///
    /// Every configured secret is compared, in constant time, whether or not
    /// an earlier one already matched, so timing reveals neither which entry
    /// matched nor how far down the list the match was.
    pub fn lookup(&self, presented: &str) -> Option<&str> {
        let presented = presented.as_bytes();
        let mut found: Option<&str> = None;
        for token in &self.tokens {
            let secret = token.secret.as_bytes();
            let same_len = secret.len() == presented.len();
            // Compare against something of the right length even on a
            // length mismatch so the work done per entry is uniform.
            let candidate = if same_len { presented } else { secret };
            let matches = same_len & bool::from(secret.ct_eq(candidate));
            if matches {
                found = Some(token.email.as_str());
            }
        }
        found
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Distinct emails with at least one token, for the startup log.
    pub fn emails(&self) -> Vec<&str> {
        let mut emails: Vec<&str> = self.tokens.iter().map(|t| t.email.as_str()).collect();
        emails.sort_unstable();
        emails.dedup();
        emails
    }
}

/// Keep a malformed entry out of the error message beyond its first few
/// characters: it may be a secret with the email missing.
fn redact(entry: &str) -> String {
    let shown: String = entry.chars().take(8).collect();
    if entry.chars().count() > 8 {
        format!("{shown}…")
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 32 chars
    const SECRET_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"; // 40 chars

    #[test]
    fn parses_comma_separated_entries() {
        let tokens = ApiTokens::parse(&format!(
            "agent@example.org={SECRET_A}, bot@example.org={SECRET_B}"
        ))
        .unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens.lookup(SECRET_A), Some("agent@example.org"));
        assert_eq!(tokens.lookup(SECRET_B), Some("bot@example.org"));
    }

    #[test]
    fn parses_a_file_with_comments_and_blank_lines() {
        let text = format!(
            "# agents\n\nAgent@Example.org={SECRET_A}\r\n   \n# retired\nbot@example.org = {SECRET_B}\n"
        );
        let tokens = ApiTokens::parse(&text).unwrap();
        assert_eq!(tokens.len(), 2);
        // Emails are folded to lower case so ADMIN_EMAILS matching behaves.
        assert_eq!(tokens.lookup(SECRET_A), Some("agent@example.org"));
        assert_eq!(
            tokens.emails(),
            vec!["agent@example.org", "bot@example.org"]
        );
    }

    #[test]
    fn unknown_or_near_miss_secrets_do_not_match() {
        let tokens = ApiTokens::parse(&format!("agent@example.org={SECRET_A}")).unwrap();
        assert_eq!(tokens.lookup(""), None);
        assert_eq!(tokens.lookup(&SECRET_A[..31]), None);
        assert_eq!(tokens.lookup(&format!("{SECRET_A}a")), None);
        assert_eq!(tokens.lookup(&SECRET_A.to_uppercase()), None);
    }

    #[test]
    fn one_email_may_hold_several_secrets_for_rotation() {
        let tokens = ApiTokens::parse(&format!(
            "agent@example.org={SECRET_A}\nagent@example.org={SECRET_B}"
        ))
        .unwrap();
        assert_eq!(tokens.lookup(SECRET_A), Some("agent@example.org"));
        assert_eq!(tokens.lookup(SECRET_B), Some("agent@example.org"));
        assert_eq!(tokens.emails(), vec!["agent@example.org"]);
    }

    #[test]
    fn rejects_short_secrets() {
        let err = ApiTokens::parse("agent@example.org=tooshort").unwrap_err();
        assert!(err.contains("shorter than"), "{err}");
    }

    #[test]
    fn rejects_duplicate_secrets_across_emails() {
        // Two identities sharing a secret would make attribution ambiguous.
        let err = ApiTokens::parse(&format!(
            "a@example.org={SECRET_A},b@example.org={SECRET_A}"
        ))
        .unwrap_err();
        assert!(err.contains("duplicates"), "{err}");
    }

    #[test]
    fn rejects_malformed_entries_without_echoing_them() {
        let err = ApiTokens::parse(SECRET_A).unwrap_err();
        assert!(!err.contains(SECRET_A), "{err}");
        assert!(err.contains("email=secret"), "{err}");

        let err = ApiTokens::parse(&format!("not-an-email={SECRET_A}")).unwrap_err();
        assert!(err.contains("invalid email"), "{err}");
    }

    #[test]
    fn empty_and_unset_admit_nobody() {
        let tokens = ApiTokens::resolve(None, None).unwrap();
        assert!(tokens.is_empty());
        assert_eq!(tokens.lookup(SECRET_A), None);

        let tokens = ApiTokens::parse("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn both_sources_set_is_an_error() {
        let err =
            ApiTokens::resolve(Some("x".into()), Some(Path::new("/nonexistent"))).unwrap_err();
        assert!(err.contains("both set"), "{err}");
    }

    #[test]
    fn missing_file_is_an_error_not_an_empty_list() {
        let err = ApiTokens::resolve(None, Some(Path::new("/nonexistent/api-tokens"))).unwrap_err();
        assert!(err.contains("could not be read"), "{err}");
    }

    #[test]
    fn reads_tokens_from_a_file() {
        let path =
            std::env::temp_dir().join(format!("tiny-tracker-api-tokens-{}", std::process::id()));
        std::fs::write(&path, format!("agent@example.org={SECRET_A}\n")).unwrap();
        let tokens = ApiTokens::resolve(None, Some(&path)).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(tokens.lookup(SECRET_A), Some("agent@example.org"));
    }
}
