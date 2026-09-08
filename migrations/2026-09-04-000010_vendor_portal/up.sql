-- Email domains permitted to request a read-only portal link for a vendor.
-- Matching is an exact, case-insensitive comparison against the part after '@',
-- so "acme.com" does not grant access to "mail.acme.com".
CREATE TABLE vendor_allowed_domains (
    id SERIAL PRIMARY KEY,
    vendor_id INTEGER NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    domain VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (vendor_id, domain)
);

CREATE INDEX idx_vendor_allowed_domains_domain ON vendor_allowed_domains (domain);

-- Single-use, short-lived magic links emailed to vendor contacts.
-- token_hash is the SHA-256 of the nonce that appears in the emailed URL; the
-- nonce itself is never stored, so a database leak cannot mint portal sessions.
CREATE TABLE vendor_magic_links (
    id SERIAL PRIMARY KEY,
    vendor_id INTEGER NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX idx_vendor_magic_links_email_created ON vendor_magic_links (email, created_at DESC);
