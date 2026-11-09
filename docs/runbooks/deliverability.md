# Deliverability runbook

## Checklist (per domain)

1. **A/MX**: `mail.example.com A <VPS IP>`, `example.com MX 10 mail.example.com`
2. **SPF**: `example.com TXT v=spf1 a:mail.example.com -all`
3. **DKIM**: `default._domainkey TXT v=DKIM1; k=rsa; p=...` (generate via `POST /api/v1/dkim/{id}/generate`)
4. **DMARC**: `_dmarc.example.com TXT v=DMARC1; p=none; rua=mailto:dmarc@example.com` → start `none`, tighten to `quarantine`/`reject` after 1–2 weeks
5. **PTR**: `<VPS IP> PTR → mail.example.com` (request from VPS provider)
6. **MTA-STS/TLS-RPT**: `_mta-sts TXT v=STSv1`, `_smtp._tls TXT v=TLSRPTv1;`

## Verify

```bash
# Live DNS check (DoH via Cloudflare, no extra tools)
curl https://mail.example.com/api/v1/domains/$ID/verify -H "Authorization: Bearer $JWT"
# Expect: {"checks":{"mx":{"ok":true},"spf":{"ok":true},"dkim":{"ok":true},"dmarc":{"ok":true},"mta_sts":{"ok":...},"tls_rpt":{"ok":...}},"deliverable":true}

# External scoring
# https://www.mail-tester.com — send test mail, target 10/10, spam score <1.0
```

Exit criteria: `deliverable: true` in `/verify` + mail-tester 10/10 (live domain). PTR warm-up: send gradually, monitor bounce/DMARC aggregate reports (`/api/v1/dmarc/reports?domain=example.com`).

## Host-network edge validation (Plan §1)

`compose.yaml` defaults the four public-facing mail daemons to
`network_mode: host` (`MAIL_EDGE_NETWORK=host` in `.env`), so inbound SMTP
shows the real public `client=` — not the Docker bridge gateway.

### Evidence to capture after a controlled external inbound test

1. **Postfix log line** — send a single test message from an external host
   (not the host itself) to `user@example.com` and pull the corresponding
   line:
   ```bash
   docker compose logs postfix | grep 'client=.*user@example.com' | tail -1
   ```
   Record the `client=<public-IP>` value. It must be the sending host's public
   IP, not `172.x.x.x` (the bridge gateway) or `127.0.0.1`.

2. **Rspamd/RBL confirmation** — for a message with a known sender IP,
   confirm the RBL/DNSBL result in the rspamd log references the same IP:
   ```bash
   docker compose logs rspamd | grep -i 'dnsbl\|rbl' | tail -5
   ```
   The IP in that line must match the `client=` value from step 1.

3. **Save both excerpts** below and keep them in this file (or
   `docs/history/`) as the acceptance record:

   ```
   Postfix client=: <paste the log line here>
   Rspamd/RBL IP:   <paste the log line here>
   Test date / sender IP: <fill in>
   ```

### Bridge-NAT fallback

Set `MAIL_EDGE_NETWORK=` (empty) in `.env` to switch back to the
`mailnet` bridge + bridge-NAT published ports. This is only appropriate when
the public source IP is not needed in Postfix logs (e.g. the whole stack sits
behind an existing NAT/load-balancer that already terminates the public IP).
Re-attach `networks: [mailnet]` and uncomment the `ports:` blocks for
`postfix` and `dovecot` in `compose.yaml` when using this fallback.

