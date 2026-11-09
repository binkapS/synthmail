# Audit history

Pointers for the closed audit findings referenced throughout the
codebase (`audit C-05`, `audit H-08`, `audit M-15`, `audit N-01`, …).
Previously this directory was empty while `README.md` listed it,
which broke the provenance trail those comments rely on (audit D-01).

## Records

| Date | Record | Scope |
|---|---|---|
| 2026-11-09 | [2026-11-09-audit.md](2026-11-09-audit.md) | Full code + security audit: `1c3c33c` (B-01…B-04, S-01…S-04, D-01…D-03, L-01…L-04) and its day-1/day-2 remediation commits (`274268c` B-01/B-02/B-03, `3bbcebe` S-01…S-04, `264eb49`/`091b222` L-01…L-04). The canonical copy lives at [`Audit.md`](../../Audit.md); this dated snapshot is the historical record. |

## Earlier audits

The pre-2026-11-09 audits (C-05, C-06, H-05, H-07, H-08, H-10, M-15,
N-01, N-04, R-04, …) were closed in `2f52fbb` / `d8edf83` /
`979dc60` / `1528979`; the per-ID rationale is not retained as a
single document, but every closed item is still cited inline where it
was fixed (for example `compose.yaml`'s `audit H-05`/`audit H-08`,
`crates/synth-common/src/rate_limit.rs`'s `audit N-01`, and
`crates/synth-webmail/src/routes.rs`'s `audit R-04`). This file is
the entry point for anyone tracing those IDs back to the
2026-11-09 re-audit.
