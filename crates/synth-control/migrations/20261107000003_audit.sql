-- Audit hardening (C-02, C-05, H-04, L-01, M-11, M-13):
ALTER TABLE users ADD COLUMN IF NOT EXISTS token_version BIGINT NOT NULL DEFAULT 0;

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='user_totp' AND column_name='last_used_step'
      AND udt_name = 'timestamptz'
  ) THEN
    ALTER TABLE user_totp DROP COLUMN last_used_step;
    ALTER TABLE user_totp ADD COLUMN last_used_step BIGINT;
  END IF;
END $$;
ALTER TABLE user_totp ADD COLUMN IF NOT EXISTS last_used_step BIGINT;

DELETE FROM dmarc_reports a USING dmarc_reports b
  WHERE a.id <> b.id
    AND a.domain = b.domain
    AND a.report_id = b.report_id
    AND a.created_at >= b.created_at;
CREATE UNIQUE INDEX IF NOT EXISTS idx_dmarc_unique_report ON dmarc_reports(domain, report_id);

DELETE FROM push_subscriptions a USING push_subscriptions b
  WHERE a.id <> b.id
    AND a.user_id = b.user_id
    AND a.endpoint = b.endpoint
    AND a.created_at >= b.created_at;
CREATE UNIQUE INDEX IF NOT EXISTS idx_push_unique_endpoint ON push_subscriptions(user_id, endpoint);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mail_search_unique_msg ON mail_search(user_id, message_id)
  WHERE message_id IS NOT NULL;
