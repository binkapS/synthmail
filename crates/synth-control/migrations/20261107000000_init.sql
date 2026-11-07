-- SynthMail initial schema (Phase 0/1)
-- Extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "citext";

-- Domains
CREATE TABLE IF NOT EXISTS domains (
  id UUID PRIMARY KEY,
  domain CITEXT UNIQUE NOT NULL,
  description TEXT,
  catch_all BOOLEAN DEFAULT false,
  dkim_selector TEXT,
  dkim_enabled BOOLEAN DEFAULT false,
  spf_value TEXT,
  dmarc_policy TEXT DEFAULT 'none',
  active BOOLEAN DEFAULT true,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_domains_active ON domains(active) WHERE active=true;

-- Users
CREATE TABLE IF NOT EXISTS users (
  id UUID PRIMARY KEY,
  email CITEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'user' CHECK (kind IN ('user','relay','admin','superadmin')),
  quota_mb INT NOT NULL DEFAULT 1024,
  used_mb INT NOT NULL DEFAULT 0,
  active BOOLEAN DEFAULT true,
  force_password_change BOOLEAN DEFAULT false,
  last_login_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_active ON users(active) WHERE active=true;

-- Aliases
CREATE TABLE IF NOT EXISTS aliases (
  id UUID PRIMARY KEY,
  address CITEXT UNIQUE NOT NULL,
  goto TEXT NOT NULL,
  active BOOLEAN DEFAULT true,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_aliases_address ON aliases(address);

-- Transport overrides
CREATE TABLE IF NOT EXISTS transport (
  user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  domain_id UUID REFERENCES domains(id) ON DELETE CASCADE,
  nexthop TEXT NOT NULL,
  PRIMARY KEY (user_id, domain_id)
);

-- Sieve
CREATE TABLE IF NOT EXISTS sieve_scripts (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(id) ON DELETE CASCADE NOT NULL,
  name TEXT NOT NULL,
  content TEXT NOT NULL,
  active BOOLEAN DEFAULT true,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_sieve_user ON sieve_scripts(user_id) WHERE active=true;

-- DKIM
CREATE TABLE IF NOT EXISTS dkim_keys (
  id UUID PRIMARY KEY,
  domain_id UUID REFERENCES domains(id) ON DELETE CASCADE NOT NULL,
  selector TEXT NOT NULL,
  public_key TEXT NOT NULL,
  private_key_enc TEXT NOT NULL,
  key_type TEXT NOT NULL DEFAULT 'rsa2048',
  dns_value TEXT GENERATED ALWAYS AS ('v=DKIM1; k=' || CASE WHEN key_type='ed25519' THEN 'ed25519' ELSE 'rsa' END || '; p=' || public_key) STORED,
  created_at TIMESTAMPTZ DEFAULT now(),
  revoked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_dkim_domain ON dkim_keys(domain_id) WHERE revoked_at IS NULL;

-- Quota overrides
CREATE TABLE IF NOT EXISTS quota_overrides (
  user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  quota_mb INT NOT NULL
);

-- Vacation
CREATE TABLE IF NOT EXISTS vacation (
  user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  subject TEXT NOT NULL DEFAULT '',
  message TEXT NOT NULL DEFAULT '',
  enabled BOOLEAN DEFAULT false,
  start_date TIMESTAMPTZ,
  end_date TIMESTAMPTZ,
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- API tokens
CREATE TABLE IF NOT EXISTS api_tokens (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES users(id) ON DELETE SET NULL,
  token_hash TEXT NOT NULL,
  scopes TEXT[] NOT NULL DEFAULT '{}',
  expires_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT now(),
  last_used_at TIMESTAMPTZ
);

-- Audit
CREATE TABLE IF NOT EXISTS audit_log (
  id BIGSERIAL PRIMARY KEY,
  actor UUID,
  action TEXT NOT NULL,
  target JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at DESC);

-- Mail search (Phase 1.5 FTS)
CREATE TABLE IF NOT EXISTS mail_search (
  id TEXT PRIMARY KEY,
  user_id UUID REFERENCES users(id) ON DELETE CASCADE,
  subject TEXT,
  body_tsv tsvector,
  folder TEXT NOT NULL,
  message_id TEXT,
  received_at TIMESTAMPTZ DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_mail_search_user ON mail_search(user_id, folder);
CREATE INDEX IF NOT EXISTS idx_mail_search_tsv ON mail_search USING GIN(body_tsv);
