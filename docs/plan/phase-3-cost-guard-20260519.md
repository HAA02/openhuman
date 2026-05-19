# Phase 3 — Cost Guard 구현 보고 (2026-05-19)

> **PRD 참조**: `unified-agent-integration-plan.md` v1.1 §2.2 / §2.3 / §2.4 / §3.1
> **상태**: 🟡 코드 작성 완료 · 시스템 빌드 차단으로 V3 게이트 실행 보류

---

## 1. 작성된 파일

| 파일 | 라인 수 | 역할 |
|---|---|---|
| `src/openhuman/security/cost_guard/mod.rs` | 19 | 모듈 export (PRD §2.2 ≤ 80라인 룰 ✅) |
| `src/openhuman/security/cost_guard/budget.rs` | 158 | `BudgetScope` enum + scope 경계 계산 (UTC, Monday-start ISO week) + 5 단위 테스트 |
| `src/openhuman/security/cost_guard/ledger.rs` | 211 | `CostGuard` SQLite handle, 멱등 migration, `set_budget`/`active_budget`/`record_cost`/`used_in_window`/`usage`/`check_quota` |
| `src/openhuman/security/cost_guard/ops.rs` | 252 | 3개 RPC 핸들러 + `ControllerSchema` 메타 + scope 파싱 |
| `src/openhuman/security/cost_guard/tests.rs` | 142 | PRD §3.1 Red 테스트 13개 |

**500-line 룰(NFR-10) 준수**: 모든 파일 < 500 라인.

**와이어업**:
- `src/openhuman/security/mod.rs` — `pub mod cost_guard;` 추가
- `src/openhuman/security/schemas.rs` — `all_controller_schemas()` 와 `all_registered_controllers()` 에서 `cost_guard::ops::cost_guard_schemas()` / `cost_guard_registered_controllers()` 를 extend

---

## 2. PRD §2.3 RPC 메서드 매핑

| PRD 메서드 | 구현 ControllerSchema | 노출 형식 |
|---|---|---|
| `security_set_budget` | `namespace="security"` + `function="set_budget"` | `openhuman.security_set_budget` (core/all.rs::rpc_method_name 규약) |
| `security_get_usage` | `namespace="security"` + `function="get_usage"` | `openhuman.security_get_usage` |
| `security_check_quota` | `namespace="security"` + `function="check_quota"` | `openhuman.security_check_quota` |

I/O 시그니처는 PRD §2.3 표를 그대로 반영.

---

## 3. PRD §2.4 데이터 모델 — SQLite 스키마

```sql
CREATE TABLE IF NOT EXISTS budgets (
  id TEXT PRIMARY KEY,
  scope TEXT NOT NULL CHECK(scope IN ('daily','weekly','monthly')),
  limit_usd REAL NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS cost_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  scope TEXT NOT NULL,
  cost_usd REAL NOT NULL,
  model TEXT,
  session_id TEXT,
  audit_record_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_ledger_ts ON cost_ledger(ts);
CREATE INDEX IF NOT EXISTS idx_ledger_scope_ts ON cost_ledger(scope, ts);
```

PRD 스키마와 완전 일치. 저장 경로: `<openhuman_dir>/cost/budget.db`. In-memory 모드 지원(`CostGuard::new_in_memory`)으로 hermetic 테스트 가능.

---

## 4. PRD §3.1 Red 테스트 매핑

| PRD 요구 테스트 | 본 구현 함수 (`security/cost_guard/tests.rs`) |
|---|---|
| `blocks_call_exceeding_daily_budget` | ✅ `blocks_call_exceeding_daily_budget` |
| `allows_call_within_budget` | ✅ `allows_call_within_budget` |
| `weekly_rolls_over_on_monday_utc` | ✅ `budget.rs::tests::weekly_window_rolls_over_on_monday_utc` (시계 mock 없이 경계 계산만 단위 검증) + `tests.rs::weekly_scope_aggregates_across_days` |
| `emits_warning_at_80_percent` | ✅ `emits_warning_at_80_percent` |

**추가 Red 테스트** (구현 견고성 확보):

- `daily_window_is_midnight_to_next_midnight`
- `monthly_window_covers_full_month` (12월→1월 wraparound 포함)
- `replacing_budget_deactivates_prior_row`
- `rejects_non_finite_or_negative_limit`
- `rejects_non_finite_or_negative_estimate`
- `migration_is_idempotent` (V3-4 검증용)
- `usage_is_zero_when_no_budget_set`
- `check_quota_with_no_active_budget_allows` (default deny? — PRD §1.4 NFR-04 "fail-safe" 해석상 한도 미설정시 거부 OR 허용. PRD §3.1의 `allows_call_within_budget`이 budget 설정 후 통과를 검증하므로, 한도 미설정 = 허용으로 결정. 명시화 필요)
- `cost_guard_schemas_advertise_three_methods`
- `cost_guard_registered_controllers_match_schema_count`

---

## 5. PRD §3.4 V3 검증 게이트 진척

| Gate | 항목 | 상태 | 비고 |
|---|---|---|---|
| V3-1 | 단위 테스트 PASS | ⏸️ BLOCKED | 코드 완료, 빌드 차단으로 실행 불가 |
| V3-2 | 한도 정확도 1000회 시뮬 | ⚪ TODO | `budget_accuracy_stress` 테스트 별도 추가 필요 |
| V3-3 | Quota 검사 ≤ 2ms p95 | ⚪ TODO | `cargo bench` 작성 후속. 현 구현은 매 호출마다 SQLite open — 캐싱 리팩터 필요 가능성 |
| V3-4 | 마이그레이션 멱등성 | ✅ 테스트 작성 | `migration_is_idempotent` |
| V3-5 | RPC E2E | ⏸️ BLOCKED | `tests/json_rpc_e2e.rs` 통합 테스트 추가 + 빌드 통과 후속 |
| V3-6 | UI BudgetGauge | ⚪ Phase 5 | 미착수 |
| V3-7 | E2E 한도 초과 시나리오 | ⚪ Phase 5 | 미착수 |
| V3-8 | 기존 테스트 회귀 0건 | ⏸️ BLOCKED | 빌드 통과 후속 |
| V3-9 | 커버리지 ≥ 85% | ⏸️ BLOCKED | `cargo llvm-cov` 실행 후속 |
| V3-10 | 수동 스모크 | ⚪ TODO | UI 통합(Phase 5) 후 실시 |

**Refactor 단계 (PRD §3.3) 향후 작업**:
- 클럭을 trait로 주입 — 현재 `Utc::now()` 직접 호출. 시계 mock이 필요한 시나리오(주간 경계 횡단)는 boundary 함수에서만 테스트로 우회.
- `CostGuard` 싱글톤화 — 매 RPC 호출시 open 비용 절감 (V3-3 ≤ 2ms p95 충족 가능성 확보).

---

## 6. 빌드 차단 — 시스템 의존성

```
error: failed to run custom build command for `alsa-sys v0.3.1`
  → libasound2-dev 누락 (cpal=0.15 → alsa-sys)

error: failed to run custom build command for `whisper-rs-sys v0.15.0`
  → cmake 미설치 + ggml.h:211 stdbool.h not found (clang/libc 헤더)
```

**해결 절차** (사용자 권한 필요):

```bash
sudo apt-get update
sudo apt-get install -y \
    libasound2-dev \
    cmake \
    libclang-dev \
    pkg-config \
    libssl-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev \
    librsvg2-dev
```

(Tauri 데스크톱 빌드에 일반적으로 필요한 시스템 패키지 일괄. 정확한 목록은 OpenHuman의 `scripts/setup-linux-deps.sh` 등이 있다면 그것을 우선 참고.)

설치 후 검증 명령:

```bash
source "$HOME/.cargo/env"
cd /run/media/iaan/1TB-WD/Github/openhuman

# V0 Phase 0 EXIT 검증
cargo check --workspace                                    # V0-1
pnpm --filter openhuman-app test:unit                      # V0-3 (이미 환경 미설치 시 vitest 동작 불가 — pnpm install --frozen-lockfile 이미 완료)
cargo test --workspace                                     # V0-4

# Phase 3 V3 게이트
cargo test -p openhuman_core security::cost_guard          # V3-1
cargo bench security_check_quota                           # V3-3 (벤치 작성 후)
cargo llvm-cov --json -p openhuman_core | diff-cover --fail-under 85   # V3-9
```

---

## 7. PRD 원칙 준수 확인

| 원칙 | 적용 |
|---|---|
| §1.4 NFR-04 fail-safe | `check_quota`는 한도 초과 시 즉시 `Block` 반환, 부분 결과 없음. |
| §1.4 NFR-10 ≤ 500라인 | 모든 신규 파일 < 260 라인. |
| §2.5 fail-safe + redaction | 본 모듈은 PII 미취급(달러/scope만 다룸) — Redaction 무관. tracing::warn은 `verdict="block"` 라벨만. |
| §2.6 관측성 | `tracing::warn!` on Block, 로그 라인은 RpcOutcome.logs로 클라이언트 전달. Prometheus histogram(`security_scan_duration_seconds`)는 후속(전체 RPC 디스패치 레이어 작업). |
| Karpathy CLAUDE.md "단순성" | 외부 mock·DI 없이 단일 mutex + bundled rusqlite. 추가 추상화 없음. |

---

## 8. 다음 단계 권장 순서

1. **(즉시·사용자)** 시스템 패키지 설치 → `cargo check --workspace` PASS 확인.
2. **(직후·자동)** `cargo test -p openhuman_core security::cost_guard` 13개 테스트 실행 → 통과 확인.
3. **(직후·자동)** RPC E2E 통합 테스트 1건 작성 (`tests/json_rpc_e2e.rs` 또는 신규 `tests/cost_guard_rpc.rs`) — V3-5 충족.
4. **(직후·자동)** `cargo bench` 작성 — V3-3 ≤ 2ms p95 측정. 미달 시 `CostGuard` 싱글톤화 Refactor.
5. **(Phase 5)** React `BudgetGauge` 컴포넌트 + 설정 화면 — V3-6/7/10.
6. **(Phase 4)** 매니페스트 SHA-256 + `Permission` enum — `security/permissions/` 신설. Phase 3와 독립이라 병행 가능.
