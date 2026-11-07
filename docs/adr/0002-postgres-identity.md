# ADR 0002 — Postgres as single identity source

Decision: Postgres 16 is only identity store; Dovecot/Postfix query via SQL lookup; synth-control is sole writer (Argon2id).
Rationale: Replaces mailcow's 3 overlapping stores (MySQL + Dovecot + PostfixAdmin).
Fallback: SCRAM-SHA-256 if Dovecot {argon2id} support gap found.
