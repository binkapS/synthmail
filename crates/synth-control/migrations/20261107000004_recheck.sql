-- Re-audit (AUDIT-RECHECK.md) hardening:
-- * N-08: pending TOTP secret — enrollment stores here first and only
--   activates on totp_verify, so abandoning enrollment cannot disable 2FA.
ALTER TABLE user_totp ADD COLUMN IF NOT EXISTS secret_pending TEXT;
ALTER TABLE user_totp ADD COLUMN IF NOT EXISTS pending_expires_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_user_totp_pending_expiry
  ON user_totp(pending_expires_at) WHERE pending_expires_at IS NOT NULL;

-- M-23 (light): keep users.email domain-managed with a trigger instead of a
-- full schema rewrite. quota_overrides is deprecated (per-user quotas live on
-- users.quota_mb); drop it so it cannot drift from the source of truth.
DROP TABLE IF EXISTS quota_overrides;

CREATE OR REPLACE FUNCTION fn_users_domain_managed() RETURNS TRIGGER AS $$
BEGIN
  -- The domain part of the email must exist in domains when it changes.
  IF (NEW.email) IS DISTINCT FROM (OLD.email) OR TG_OP = 'INSERT' THEN
    IF NOT EXISTS (
      SELECT 1 FROM domains d
      WHERE LOWER(d.domain) = LOWER(SPLIT_PART(NEW.email, '@', 2))
    ) THEN
      RAISE EXCEPTION 'user % domain not managed', NEW.email
        USING ERRCODE = 'check_violation';
    END IF;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_users_domain_managed ON users;
CREATE TRIGGER trg_users_domain_managed
  BEFORE INSERT OR UPDATE OF email ON users
  FOR EACH ROW EXECUTE FUNCTION fn_users_domain_managed();
