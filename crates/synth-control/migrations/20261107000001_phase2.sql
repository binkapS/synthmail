-- Phase 2: 2FA, API tokens hardening, login throttling, CardDAV/CalDAV, audit indexes
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- TOTP storage (one row per user; secret encrypted at rest in app layer via SOPS/age in Phase 2 prod)
CREATE TABLE IF NOT EXISTS user_totp (
  user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  secret TEXT NOT NULL,
  enabled BOOLEAN DEFAULT false,
  verified_at TIMESTAMPTZ,
  last_used_step TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT now()
);

-- Extend api_tokens with label/prefix for UX + revocation
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS token_prefix TEXT;
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(token_prefix);

-- Login attempts for rate-limit + fail2ban sync
CREATE TABLE IF NOT EXISTS login_attempts (
  id BIGSERIAL PRIMARY KEY,
  email CITEXT NOT NULL,
  ip TEXT,
  success BOOLEAN DEFAULT false,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_login_attempts_email ON login_attempts(email, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_login_attempts_ip ON login_attempts(ip, created_at DESC);

-- CardDAV / CalDAV storage (WebDAV resources as JSON blobs; backed by synth-carddav + Radicale)
CREATE TABLE IF NOT EXISTS carddav_contacts (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(id) ON DELETE CASCADE NOT NULL,
  display_name TEXT NOT NULL,
  vcard TEXT NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_carddav_user ON carddav_contacts(user_id);

CREATE TABLE IF NOT EXISTS caldav_events (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(id) ON DELETE CASCADE NOT NULL,
  uid TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  ical TEXT NOT NULL,
  starts_at TIMESTAMPTZ,
  ends_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_caldav_user ON caldav_events(user_id, starts_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_caldav_uid ON caldav_events(user_id, uid);

-- Mail search enrichment: snippet + received highlighting
ALTER TABLE mail_search ADD COLUMN IF NOT EXISTS snippet TEXT;
ALTER TABLE mail_search ADD COLUMN IF NOT EXISTS from_addr TEXT;
ALTER TABLE mail_search ADD COLUMN IF NOT EXISTS body TEXT;

-- Audit: allow ip/device context
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS ip TEXT;
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS user_agent TEXT;

-- Settings: per-domain DMARC reports staging
CREATE TABLE IF NOT EXISTS dmarc_reports (
  id UUID PRIMARY KEY,
  domain CITEXT NOT NULL,
  report_id TEXT NOT NULL,
  xml TEXT NOT NULL,
  report_type TEXT NOT NULL DEFAULT 'aggregate',
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dmarc_domain ON dmarc_reports(domain, created_at DESC);
