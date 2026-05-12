# Phase 0 — 사전 정합성 검증 결과

> **실행일**: 2026-05-12  
> **실행자**: Claude Code (Opus 4.7)  
> **결과**: ⛔ **EXIT 차단** — V0-1/V0-4/V0-5 시스템 의존성 누락(libclang)

---

## 결과 요약

| Gate | 항목 | 결과 | 상세 | 로그 |
|---|---|---|---|---|
| V0-0 | pnpm install (부트스트랩) | ✅ PASS | 1100 패키지 설치, 29.8s | `target/debug-logs/bootstrap-pnpm-install.log` |
| V0-1 | cargo check --workspace | ❌ FAIL | `whisper-rs-sys` 빌드 시 libclang 누락 | `target/debug-logs/v0-1-cargo-check.log` |
| V0-2 | pnpm build (frontend) | ✅ PASS | 1139 modules, 5.32s, 2.4MB bundle | `target/debug-logs/v0-2-build.log` |
| V0-3 | pnpm test:unit | ✅ PASS | **2062 테스트 PASS**, 3 skipped, 218 파일, 752s | `target/debug-logs/v0-3-test-unit.log` |
| V0-4 | pnpm test:rust | ⏸️ BLOCKED | V0-1 차단 — cargo 미통과 | — |
| V0-5 | RPC 메서드 스냅샷 | ⏸️ BLOCKED | V0-1 차단 — `cargo run` 미가능 | — |
| V0-6 | pnpm typecheck | ✅ PASS | `tsc --noEmit` 0 error | `target/debug-logs/v0-6-typecheck.log` |
| V0-7 | pnpm lint | ✅ PASS | 0 error, 39 warning (기존) | `target/debug-logs/v0-7-lint.log` |

**PASS: 5/8 · FAIL: 1/8 · BLOCKED: 2/8**

---

## 차단 원인 — libclang 누락

```
error: failed to run custom build command for `whisper-rs-sys v0.15.0`
Unable to find libclang: "couldn't find any valid shared libraries matching:
['clang.dll', 'libclang.dll'], set the `LIBCLANG_PATH` environment variable
to a path where one of these files can be found (invalid: [])"
```

**원인**: `whisper-rs-sys` 크레이트가 C 헤더 바인딩 생성을 위해 `bindgen`을 호출하며, `bindgen`은 시스템에 LLVM/Clang이 설치되어 있어야 동작합니다. 현재 머신에는 LLVM이 설치되어 있지 않습니다.

**영향 범위**: Rust 워크스페이스 전체. cargo check / build / test 모두 동일한 빌드 의존성 그래프를 통과하므로 LLVM 없이는 Rust 코어 개발 불가.

---

## 해결 방법

### Option A — LLVM 시스템 설치 (권장)

```powershell
# winget 사용 (관리자 권한 필요할 수 있음)
winget install LLVM.LLVM

# 또는 직접 다운로드
# https://github.com/llvm/llvm-project/releases (LLVM-x.y.z-win64.exe)
```

설치 후 환경변수 설정 (PowerShell 영구):
```powershell
[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

새 셸 세션에서:
```powershell
cargo check --manifest-path D:\Git\openhuman\Cargo.toml
```

### Option B — whisper 기능 비활성화 (대안, 비권장)

`Cargo.toml`에서 whisper feature를 default에서 제외. 단, 음성 기능(STT)이 비활성화되며 PR에서 회귀로 분류될 수 있어 권장하지 않습니다.

---

## 기준선 메트릭 (Baseline) — Frontend

Phase 1 이후 회귀 비교 기준으로 사용할 수치들:

| 지표 | 값 |
|---|---|
| 단위 테스트 파일 수 | 219 (218 PASS + 1 skipped) |
| 단위 테스트 수 | 2065 (2062 PASS + 3 skipped) |
| 단위 테스트 실행 시간 | 752.29s |
| 빌드 시간 | 5.32s |
| 빌드 산출물 크기 | 2,446.86 kB (gzip 688.63 kB) |
| 변환 모듈 수 | 1139 |
| TypeScript 에러 | 0 |
| ESLint 에러 | 0 |
| ESLint 경고 | 39 |

---

## 다음 단계 권장

### 즉시
1. **LLVM 설치** (위 Option A) → `LIBCLANG_PATH` 설정
2. **V0-1 재실행** — `cargo check --manifest-path Cargo.toml`
3. **V0-4 실행** — `pnpm test:rust`
4. **V0-5 실행** — RPC 메서드 스냅샷 저장

### Phase 0 완전 통과 후
- Phase 1 (Input Guard) 착수 — `src/openhuman/security/input_guard/` 모듈 생성
- Red → Green → Refactor 사이클로 진행
- 각 단계 검증 게이트 V1-1 ~ V1-10 통과 필수

---

## 검증 절차의 가치

본 Phase 0는 PRD에 명시된 검증 절차의 첫 번째 게이트로, 그 목적은 **"실제 개발이 시작되기 전에 환경 결함을 발견하는 것"**입니다. 이번 실행은 그 목적을 달성했습니다 — 코드 한 줄도 작성하기 전에 LLVM 누락이라는 진짜 차단 요소를 표면화했습니다. 이는 검증 게이트 없이 Phase 1을 시작했다면 Red 테스트 작성 직후 직면했을 문제입니다.

**Phase 0 EXIT 조건 (V0-1 ~ V0-7 전부 PASS) 미충족 → Phase 1 진행 불가**
