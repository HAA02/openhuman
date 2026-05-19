# Security & Threat Model

> **Scope**: Threat model for the OpenHuman desktop runtime, covering the
> security domain modules introduced by the Unified Agent integration
> (PRD `docs/plan/unified-agent-integration-plan.md`).
>
> **Audience**: Contributors touching `src/openhuman/security/`,
> `src/openhuman/skills/`, or any RPC controller that handles untrusted input.
>
> **Out of scope**: Multi-tenant ACLs, cloud SIEM integration, network-level
> hardening of the host operating system.

---

## 1. Architecture at a glance

```
React UI ──▶ Tauri shell ──▶ core_rpc_relay ──▶ Rust core
                                                  │
                                                  ▼
                       ┌──────────────────────────────────────┐
                       │ src/openhuman/security/              │
                       │   input_guard  (prompt_injection)    │  ← scan_input
                       │   audit        (JSONL pre/post)      │  ← get/export_audit
                       │   cost_guard   (SQLite budget)       │  ← set_budget / get_usage / check_quota
                       │   permissions  (manifest + gate)     │  ← verify_skill / list_permissions
                       │   policy / pairing / secrets / …     │  (existing)
                       └──────────────────────────────────────┘
```

LLM call path (left to right): `input_guard` → `permissions` → `audit_pre` →
LLM provider → `audit_post` → `cost_guard` → memory.

Every gate is **fail-safe**: on detection of a violation the call is rejected
and **no partial result** is returned to the caller (PRD §1.4 NFR-04).

---

## 2. Threat → mitigation matrix

| # | Threat | Mitigation module | Validation |
|---|---|---|---|
| T-01 | Direct prompt injection (`"ignore previous instructions…"`) | `prompt_injection` + `security::ops::security_scan_input` | unit tests, 50-pattern corpus (planned) |
| T-02 | Indirect prompt injection from tool output | `audit` records full pre/post payloads with redaction; `permissions::gate` keeps skill output scoped | integration test (planned) |
| T-03 | Token-cost runaway | `security::cost_guard::CostGuard::check_quota` blocks at daily/weekly/monthly limits | unit tests in `cost_guard/tests.rs`; bench `cargo bench security_check_quota` |
| T-04 | Skill exfiltrating files without permission | `security::permissions::PermissionGate` deny-all default + 5-category gate | unit tests in `permissions/gate.rs::tests` |
| T-05 | Skill manifest tampering | `security::permissions::verify_manifest` SHA-256 pinning | `permissions/manifest.rs::tests::rejects_tampered_manifest_when_expected_hash_provided` |
| T-06 | Secret/PII leakage to logs or audit | `security::audit::redact` patterns (API keys, JWT, email, URL credentials, PAN) | `cargo test redact_pii` (planned 50-fixture corpus) |
| T-07 | Audit log tampering | Unix `0600` / Windows DPAPI on JSONL files, append-only writer | E2E permission check (planned) |
| T-08 | Skill install RCE via malicious SKILL.md fetch | `skills::ops_install::validate_install_url` (https only, no private hosts, bounded body, allowlisted extensions) | unit tests in `skills/ops_install.rs` |
| T-09 | Cost-ledger SQL injection | `rusqlite` parameter binding throughout `cost_guard::ledger` (no string interpolation) | code review; integration tests rely on bundled SQLite |

---

## 3. Module reference

### 3.1 `security/input_guard` (Phase 1)

Wraps `crate::openhuman::prompt_injection::enforce_prompt_input`. Returns a
`PromptEnforcementDecision` with verdict (`allow` / `review` / `block`), score,
matched reasons, action, and SHA-256 hash of the scanned text (no raw text
echoed back).

**RPC**: `openhuman.security_scan_input` — see `src/openhuman/security/schemas.rs:51`.

### 3.2 `security/audit` (Phase 2)

Append-only JSONL writer at `<openhuman_dir>/audit.log`. Records LLM call
pre/post, skill execution, and blocked-input events.

**RPCs**:
- `openhuman.security_get_audit` — paginated read with `since` timestamp.
- `openhuman.security_export_audit` — filtered export to a JSONL file.

### 3.3 `security/cost_guard` (Phase 3)

SQLite-backed budget enforcement. Three independent scopes:
- **Daily**: UTC midnight to next midnight.
- **Weekly**: Monday 00:00 UTC to next Monday (ISO 8601).
- **Monthly**: 1st of month UTC to 1st of next month.

`check_quota(estimated_usd)` is **fail-safe**: any scope projected over its
limit blocks the call. Warnings fire at ≥ 80 % of any limit but never block.

**RPCs**:
- `openhuman.security_set_budget` — replace the active budget for a scope.
- `openhuman.security_get_usage` — `{ used_usd, limit_usd, percent, window_start, window_end }`.
- `openhuman.security_check_quota` — `{ verdict, reason }`.

**Storage**: `<openhuman_dir>/cost/budget.db`. Schema is idempotent
(`CREATE TABLE IF NOT EXISTS`); see `cost_guard/ledger.rs::apply_migrations`.

### 3.4 `security/permissions` (Phase 4)

Two surfaces:

- **`manifest::verify_manifest(path, expected_sha256)`** — parses YAML
  frontmatter, validates required fields, computes SHA-256, optionally
  compares against a pinned hash. Returns `ManifestVerification` with verdict
  + sha256 + issues.
- **`PermissionGate`** — `Arc<RwLock<HashMap<SkillId, BTreeSet<Permission>>>>`.
  Deny-all default; explicit `grant` / `revoke` API; `check` returns
  `Decision::Allow` only on a recorded grant.

Permission categories: `file_read`, `file_write`, `network`, `process`,
`system_info` (PRD §1.3 FR-06).

**RPCs**:
- `openhuman.security_verify_skill` — `{ verdict, sha256, issues, path }`.
- `openhuman.security_list_permissions` — `{ skill_id, granted: [...] }`.

---

## 4. Redaction policy

`audit::redact` removes these patterns before persisting to JSONL:

1. Provider API keys (`sk-…`, `ghp_…`, `AKIA…`)
2. JSON Web Tokens (`eyJ…`)
3. Email addresses (`user@host.tld` → `user@***.tld`)
4. URL userinfo (`https://u:p@h/…` → `https://u:***@h/…`)
5. PAN (16-digit card numbers) — last four digits retained

Test corpus (planned 50 fixtures) lives at
`tests/fixtures/pii_corpus.json`; `cargo test redact_pii_corpus` enforces
100 % coverage.

---

## 5. Observability

| Signal | Implementation | Used by |
|---|---|---|
| `tracing::warn!` on block | All gates emit on `verdict = block` | Sentry breadcrumb integration |
| `RpcOutcome.logs` | One human-readable line per gate decision | UI debugging panel |
| Prometheus histograms (planned) | `security_scan_duration_seconds`, `security_audit_writes_total`, `security_budget_used_ratio` | Metrics dashboard |

Sentry samples are configured at `traces_sample_rate=0.1` (see PRD §4 R-06)
to avoid breadcrumb storms on noisy `block` cascades.

---

## 6. Performance budgets

| Hot-path operation | Budget | Measurement |
|---|---|---|
| `input_guard::scan` | ≤ 5 ms p95 | `cargo bench security_scan` |
| `audit::write` (background) | ≤ 1 ms p95 added to call path | `cargo bench agent_with_audit` |
| `cost_guard::check_quota` | ≤ 2 ms p95 | `cargo bench security_check_quota` (planned) |

If `cost_guard::check_quota` approaches the budget, the first refactor is
moving the `Connection` behind a `OnceLock` singleton so SQLite open cost is
paid once per process.

---

## 7. Reporting a vulnerability

See [`SECURITY.md`](../../SECURITY.md) in the repo root. Do not file public
issues for security reports; use the disclosure channel documented there.

---

## 8. References

- PRD: `docs/plan/unified-agent-integration-plan.md`
- Phase 0 validation report: `docs/plan/phase-0-validation-report.md`
- Phase 0 Linux re-validation + gap analysis: `docs/plan/phase-0-revalidation-and-gap-20260519.md`
- Phase 3 implementation report: `docs/plan/phase-3-cost-guard-20260519.md`
- Inspiration / cross-comparison: `docs/report/report-openhuman-vs-disbot-20260512.html`
