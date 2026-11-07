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
