# Phase 0 재검증 + 갭 분석 — 2026-05-19

> **실행일**: 2026-05-19
> **환경**: Linux (ha02 머신, libclang21 시스템 lib 확인)
> **참고**: `unified-agent-integration-plan.md` v1.1, `phase-0-validation-report.md` (2026-05-12 Windows)
> **결과**: ⛔ Phase 0 여전히 차단 — 차단 원인이 libclang에서 **Rust 툴체인 + node_modules 미설치**로 이동

---

## 1. Phase 0 게이트 재실행 결과 (Linux)

| Gate | 항목 | 결과 | 차단 사유 | 로그 |
|---|---|---|---|---|
| V0-1 | `cargo check --workspace` | ❌ FAIL | `cargo: 명령을 찾을 수 없음` — rustup/cargo 미설치 | `target/debug-logs/v0-1-cargo-check-linux.log` |
| V0-2 | `pnpm --filter openhuman-app build` | ❌ FAIL | `app/node_modules` 미설치 → vite 바이너리 없음 | `target/debug-logs/v0-2-build-linux.log` |
| V0-3 | `pnpm --filter openhuman-app test:unit` | ❌ FAIL | `vitest: not found` (동일 원인) | `target/debug-logs/v0-3-test-unit-real.log` |
| V0-4 | `cargo test -p openhuman_core` | ❌ FAIL | cargo 미설치 | `target/debug-logs/v0-4-cargo-test-security.log` |
| V0-5 | RPC 스냅샷 | ⏸️ BLOCKED | V0-1/V0-4 차단 | — |
| V0-6 | `pnpm typecheck` | ❌ FAIL | `tsc` 바이너리 없음 (node_modules 미설치) | `target/debug-logs/v0-6-typecheck-linux.log` |
| V0-7 | `pnpm lint` | ❌ FAIL | `eslint` 바이너리 없음 (동일 원인) | `target/debug-logs/v0-7-lint-linux.log` |

**진단**:
- 2026-05-12 Windows 검증의 차단 원인이었던 `libclang`은 본 머신(`/usr/lib/x86_64-linux-gnu/libclang-21.so.21`)에서 해소.
- 하지만 본 머신은 OpenHuman 개발 환경이 처음 셋업되는 상태로, 두 종류의 사전 준비가 누락됨:
  1. **Rust 툴체인**: `rustup` 미설치, `~/.cargo/bin/cargo` 없음, PATH에 cargo 없음.
  2. **JS 의존성**: 루트 및 `app/`에 `node_modules` 없음. `pnpm install` 미실행.

**복구 절차 (제안)**:

```bash
# 1) Rust 툴체인 (`rust-toolchain.toml` 기반 자동 선정)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
source "$HOME/.cargo/env"
( cd /run/media/iaan/1TB-WD/Github/openhuman && rustup show )   # toolchain 자동 설치

# 2) JS 의존성
( cd /run/media/iaan/1TB-WD/Github/openhuman && pnpm install --frozen-lockfile )

# 3) Phase 0 재실행
( cd /run/media/iaan/1TB-WD/Github/openhuman && cargo check --workspace )
( cd /run/media/iaan/1TB-WD/Github/openhuman && pnpm --filter openhuman-app build )
( cd /run/media/iaan/1TB-WD/Github/openhuman && pnpm --filter openhuman-app test:unit )
( cd /run/media/iaan/1TB-WD/Github/openhuman && cargo test --workspace )
( cd /run/media/iaan/1TB-WD/Github/openhuman && pnpm --filter openhuman-app compile )   # typecheck
```

**플랜 명령 vs 실제 명령**: 플랜은 `pnpm test:unit` / `pnpm test:rust` 형태로 기재되어 있으나 루트 `package.json`에는 해당 스크립트가 **없음**. 실제로는 `pnpm --filter openhuman-app <name>` 또는 직접 `cargo`/`vitest` 호출이 필요. v1.2에서 명령 표를 갱신해야 함.

---

## 2. 갭 분석 — 플랜 v1.1 vs 실제 `develop` 브랜치

### 2.1 모듈 디렉토리 구조 갭

플랜 §2.2의 모듈 분해는 **하위 디렉토리 구조**(`security/input_guard/`, `security/audit/`, `security/cost_guard/`, `security/permissions/`)를 가정하지만, 실제 코드는 **flat 파일 구조**이며 일부 도메인은 보안 모듈 밖으로 분리됨.

| 플랜 요구 (§2.2) | 실제 위치 | 갭 |
|---|---|---|
| `security/input_guard/{patterns,detector,ops,tests}.rs` | `prompt_injection/{detector,tests}.rs` (332+136줄) + `security/ops.rs:security_scan_input` | **이중 모듈** — 보안 표면이 두 트리에 분산. 단일 입구 부재. |
| `security/audit/{writer,schema,ops,tests}.rs` | `security/audit.rs` (581줄, **500룰 위반**) | flat 단일 파일. 분리 필요. |
| `security/cost_guard/{ledger,budget,ops,tests}.rs` | `cost/{tracker,types,schemas}.rs` (별도 최상위 모듈) | **위치 다름** + `cost/schemas.rs`가 **빈 vec 반환** → Phase 3 RPC 노출 0건. |
| `security/permissions/{manifest,gate,ops,tests}.rs` | `security/policy.rs` (816줄) + `security/policy_tests.rs` (954줄) | flat·거대 파일, 매니페스트 SHA-256 검증 / deny-all 권한 게이트 미확인. |
| 비용·결제 관련 | 별도 `billing/` (Stripe/Coinbase 결제) | 플랜 외 추가 도메인. Phase 3 cost_guard와 **혼동·중복 가능**. 경계 정리 필요. |

### 2.2 500-line 룰(NFR-10) 위반 현황

```
audit.rs              581  (+81)
policy.rs             816  (+316)
policy_tests.rs       954  (+454)
secrets_tests.rs      618  (+118)
billing/schemas.rs    576  (+76)
billing/ops.rs        459  (한계 근접)
```

플랜이 명시한 CI 게이트(`wc -l` 위반 시 실패)가 **아직 강제되지 않음**.

### 2.3 RPC 메서드 노출 — 플랜 vs 실제

**플랜 §2.3 요구 신규 메서드 8개:**

| Method (플랜) | 실제 노출 | 상태 |
|---|---|---|
| `security_scan_input` | `security.scan_input` (네임스페이스 분리 형식) | ✅ 구현 — `schemas.rs:51` |
| `security_get_audit` | `security.get_audit` | ✅ 구현 — `schemas.rs:104` |
| `security_export_audit` | `security.export_audit` | ✅ 구현 — `schemas.rs:143` |
| `security_set_budget` | **없음** | ❌ Phase 3 미구현 |
| `security_get_usage` | **없음** | ❌ Phase 3 미구현 |
| `security_check_quota` | **없음** | ❌ Phase 3 미구현 |
| `security_verify_skill` | **없음** | ❌ Phase 4 미구현 |
| `security_list_permissions` | **없음** | ❌ Phase 4 미구현 |

**부가 발견**: 플랜은 메서드를 `openhuman.security_*` prefix로 가정하나 실제 컨트롤러는 `namespace="security"` + `function="<name>"` 분리 모델 — JSON-RPC 외부 노출 시 두 호출 규약을 어떻게 매핑하는지 별도 확인 필요(추가 작업 항목).

### 2.4 Phase별 구현 완성도 (커밋·코드 근거)

| Phase | 항목 | 완성도 | 근거 |
|---|---|---|---|
| Phase 1 — Input Guard | scan_input RPC | 🟡 ~70% | RPC 노출 완료. `enforce_prompt_input`이 verdict/score/action/prompt_hash 반환. 단 — **50종 인젝션 코퍼스 fixture 미발견**, V1-2/V1-3 정밀도·리콜 기준 검증 불가. p95 ≤ 5ms 벤치(V1-4) 미작성. |
| Phase 2 — Audit Trail | JSONL writer + RPC | 🟡 ~60% | `audit.rs` 581줄에 writer·schema·redact 혼재. RPC 3개 노출. 단 — **agent bus pre/post hook 통합 여부 미확정**, PII redaction 코퍼스 테스트 미발견(V2-3), 비차단 성능(V2-2) 측정 안 됨, 일별 로테이션 테스트(V2-4) 부재. 최근 `feat(security): audit local ai model calls` 커밋이 hook 시도. |
| Phase 3 — Cost Guard | 예산 한도 + RPC | 🔴 ~10% | `cost/tracker.rs`(406줄)에 토큰 집계만 존재. **`cost/schemas.rs`가 빈 vec** → RPC 노출 0건. `Budget` 구조체, `budgets`/`cost_ledger` SQLite 테이블, scope(daily/weekly/monthly) 경계 계산, fail-safe check_quota — **모두 부재**. |
| Phase 4 — Permissions | manifest SHA-256 + deny-all | 🔴 ~15% | `security/policy.rs`(816줄)에 정책 모델만. 매니페스트 무결성·`Permission` enum·실행 시점 gate 검사 코드는 미확인. `verify_skill` RPC 미구현. |
| Phase 5 — UI | React 설정 화면 | ⚪ 0% | `app/src/components/settings/security/` 디렉토리 미생성 (확인 필요). 핵심 컴포넌트 3종(BudgetGauge / AuditView / PermissionDialog) 미구현. |
| Phase 6 — Docs | `gitbooks/developing/security.md` | ⚪ 0% | 미작성. |
| Phase 7 — Discord (선택) | 슬래시 명령 표준화 | ⚪ — | 범위 외 / 선택 단계. |
| Phase 8 — RC | 통합 회귀 | ⚪ — | 전 단계 미완. |

### 2.5 플랜과의 실질적 불일치 우선순위

| # | 항목 | 영향도 | 권장 조치 |
|---|---|---|---|
| 1 | `cost/schemas.rs` 빈 vec — Phase 3 RPC 전면 미구현 | High | TDD 사이클로 set_budget/get_usage/check_quota 신설. SQLite ledger 마이그레이션 작성. |
| 2 | 500-line 룰 위반 4건 + CI 게이트 부재 | Medium | `policy.rs` / `policy_tests.rs` / `audit.rs` / `secrets_tests.rs` 분리 + `scripts/check-file-lines.sh` 추가. |
| 3 | `prompt_injection`이 `security/` 밖에 있음 | Medium | 위치 통일 또는 facade 도입. 플랜 v1.2에 실제 구조 반영. |
| 4 | Phase 1/2 검증 게이트(V1-2~10, V2-2~9) 미실행 | Medium | 명시적 fixture(`tests/fixtures/injection_corpus.json` 등) 추가 후 게이트 재실행. |
| 5 | RPC 메서드 네이밍 규약 차이 (`openhuman.security_*` ↔ `security.<name>`) | Low | 플랜 표기를 실제와 일치시키거나, JSON-RPC 외부 매핑 레이어 명시. |
| 6 | `billing/` ↔ `cost/` 경계 불명확 | Medium | 책임 분리 문서화: billing=결제·잔액, cost=토큰·예산 한도. |

---

## 3. 다음 단계 권장 (우선순위)

### 즉시 (사용자 결정 필요)

- [ ] **Rust 툴체인 설치** (rustup) — 위 §1 복구 절차 1단계. ~3–5분.
- [ ] **`pnpm install`** — 위 §1 복구 절차 2단계. ~2–5분.
- [ ] Phase 0 게이트 V0-1 ~ V0-7 재실행, 기준선 메트릭 기록.
- [ ] `cargo run --bin openhuman -- list-rpc`(또는 등가) 동작 여부 확인 후 V0-5 스냅샷 저장.

### Phase 0 통과 후 (4가지 선택지)

A. **Phase 3 신규 구현** — cost_guard. 가장 큰 갭. 플랜 §2.2/§2.3 그대로 적용 가능. ~3–4일.
B. **Phase 1/2 검증 게이트 보강** — 인젝션 코퍼스 fixture, redaction 코퍼스, agent bus hook 통합 테스트, p95 벤치. ~2일.
C. **500-line 룰 강제 + 기존 파일 분리** — 4개 파일 모듈화. 회귀 리스크 있음. ~1일.
D. **플랜 v1.2 작성** — 실제 코드 구조를 반영한 갱신본. ~0.5일.

> 갭이 가장 큰 Phase 3(cost_guard)는 다른 Phase에 종속성 없음 → 병렬 진행 가능.
> Phase 1/2는 이미 RPC가 외부로 노출된 상태라 검증 게이트 보강(B)을 먼저 끝내야 RC 단계에서 회귀 부담이 작음.

---

## 4. 이번 검증의 가치

본 검증은 두 가지 사실을 표면화함:

1. **Phase 0이 환경 차원에서 여전히 미충족** — 차단 원인이 시스템(libclang) → 사용자 환경(rustup/node_modules)으로 이동했을 뿐, **단 한 줄의 코드 변경 전에 환경 표준화가 필요**하다는 플랜 원칙은 여전히 유효.
2. **Phase 0이 코드 차원에서도 미충족** — 이미 진행된 Phase 1/2 작업은 검증 게이트 일부(단위 테스트 컴파일 성공)만 통과했을 뿐, V1-2~V1-10·V2-2~V2-10 다수가 미실행. 즉 **플랜이 정의한 EXIT 조건을 만족하지 못한 채 Phase 1/2 머지가 진행됨**.

→ Phase 0 → Phase 3 진행 전, 플랜 v1.2에서 본 갭을 반영해 EXIT 조건을 재정의하거나, Phase 1/2의 누락 게이트를 회수해야 함.
