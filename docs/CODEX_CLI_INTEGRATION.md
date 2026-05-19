# Codex CLI 통합 가이드

> 다른 프로젝트에서 LLM 처리를 **외부 `codex` CLI 호출**로 일관되게 구현하기 위한 패턴 문서. memoria 프로젝트에서 검증된 방식을 그대로 이식할 수 있도록 정리.

## 왜 CLI인가 (HTTP API 대신)

| 축 | CLI (`spawnSync('codex')`) | HTTP API (`fetch(/v1/...)`) |
|---|---|---|
| 인증 | codex 로그인 세션 재사용 — API 키 관리 불필요 | API 키 직접 관리, 회전 정책 필요 |
| Rate limit | CLI가 자체 처리 | 직접 backoff 구현 |
| Sandbox/도구 호출 | `--sandbox` 등 codex 내장 보안 모드 사용 | 직접 구현 |
| 비용 | 사용자가 codex 결제로 통합 | API 결제 별도 |
| 호출 오버헤드 | spawn ~500ms/회 | TLS 핸드셰이크 1회 (keep-alive 시 0) |
| 격리 | 자식 프로세스 → 호출 실패가 서버 죽이지 않음 | 라이브러리 예외 처리 필요 |
| 외부 의존 | `codex` 바이너리 사전 설치 필수 | 없음 |

**권장 사용처 (CLI)**: 사용자 인증·세션이 필요한 합성/요약/대화 작업
**권장 사용처 (HTTP)**: 임베딩·rerank 같은 백엔드 서비스 작업

---

## 사전 조건

### 1) codex CLI 설치 확인

```bash
which codex
# /usr/local/bin/codex (또는 npm global)
codex --version
```

미설치 시 OpenAI Codex CLI를 설치 (프로젝트 README의 안내 참고).

### 2) Node.js ≥ 18 (`spawnSync`, `tmpdir`, `mkdtempSync` 사용)

---

## 최소 통합 — 단일 헬퍼 모듈

`lib/codex-headless.ts` 파일 하나를 다른 프로젝트에 복사하면 됩니다.

```ts
import { spawnSync } from 'child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

export interface CodexResult {
  ok: boolean;
  content?: string;
  error?: string;
}

const DEFAULT_TIMEOUT_MS = 45_000;

function envEnabled(name: string): boolean {
  return ['1', 'true', 'yes', 'on'].includes(
    String(process.env[name] ?? '').toLowerCase()
  );
}

/** 환경변수로 기능 전체를 끄거나 켜는 게이트. 호출 측에서 옵션 점검. */
export function codexEnabled(): boolean {
  return envEnabled('CODEX_ENABLED'); // 프로젝트마다 이름 변경 가능
}

/** prompt를 codex exec stdin으로 전달하고 마지막 메시지를 반환. */
export function runCodexHeadless(
  prompt: string,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): CodexResult {
  const dir = mkdtempSync(join(tmpdir(), 'codex-headless-'));
  const outputFile = join(dir, 'last-message.txt');
  try {
    const result = spawnSync('codex', [
      'exec',
      '--model', process.env['CODEX_MODEL'] ?? 'gpt-5.4-mini',
      '--sandbox', 'read-only',          // 파일 시스템 쓰기 차단
      '--ignore-rules',                  // 프로젝트 .codex/rules 무시 (헤드리스 단발 호출에 적합)
      '--skip-git-repo-check',           // git 디렉토리 아니어도 동작
      '--cd', process.env['CODEX_CWD'] ?? process.cwd(),
      '--output-last-message', outputFile,
      '-',                               // stdin으로 prompt 수신
    ], {
      input: prompt,
      encoding: 'utf8',
      timeout: timeoutMs,
      shell: process.platform === 'win32',  // Windows에서 .cmd shim 호출 위해
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    if (result.error) {
      return { ok: false, error: result.error.message };
    }
    if (result.status !== 0) {
      const stderr = result.stderr?.trim();
      const stdout = result.stdout?.trim();
      return { ok: false, error: stderr || stdout || `codex exited with ${result.status}` };
    }

    const content = existsSync(outputFile)
      ? readFileSync(outputFile, 'utf8').trim()
      : result.stdout.trim();
    return content ? { ok: true, content } : { ok: false, error: 'empty codex output' };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
```

### 플래그 의미

| 플래그 | 효과 | 권장 |
|---|---|---|
| `exec` | 비대화형 실행 (대화 세션 X) | 헤드리스 호출에 필수 |
| `--model <id>` | 모델 지정 | 환경변수로 override 가능하게 |
| `--sandbox read-only` | 파일 쓰기 차단 | 합성/요약 작업 안전 기본값 |
| `--ignore-rules` | 프로젝트 `.codex/` 룰 무시 | 헤드리스 호출은 외부 입력에 의존하지 않아야 함 |
| `--skip-git-repo-check` | git 검사 우회 | 임시 디렉토리에서도 동작 |
| `--cd <dir>` | 작업 디렉토리 | codex가 파일 접근할 base |
| `--output-last-message <file>` | 마지막 응답을 파일로 기록 | stdout 파싱보다 안정적 |
| `-` (인자 끝) | stdin에서 prompt 읽기 | env 노출 회피, 길이 제한 X |

---

## 프롬프트 빌더 패턴

LLM 응답 품질은 프롬프트 구조에 좌우됩니다. memoria가 검증한 두 가지 템플릿:

### 1) 검색 합성 프롬프트 (RAG 결과 → 한국어 요약)

```ts
export function buildSearchPrompt(
  query: string,
  snippets: Array<{ id?: string; content: string; source?: string }>,
  sourceFiles?: Array<{ source: string; body: string }>,
): string {
  const memoryLines = snippets.map((s, i) =>
    `[${i + 1} id=${s.id ?? '?'}${s.source ? ` source=${s.source}` : ''}]\n${s.content}`
  );

  const sourceBlock = sourceFiles?.length
    ? [
        '<source-files>',
        ...sourceFiles.flatMap(f => [`<file path="${f.source}">`, f.body, '</file>']),
        '</source-files>',
      ].join('\n')
    : '';

  return [
    'You are a search synthesis assistant running in Codex headless mode.',
    'Answer the user query using the provided memory snippets and full source files (when attached).',
    'Treat <source-files> as authoritative when the snippets are summaries of those files.',
    'If both snippets and source files are insufficient, say what is missing.',
    'Return a concise Korean answer. When procedural steps are present, preserve the ordered list verbatim.',
    'Include a short "출처" section with memory ids and source paths when available.',
    'Do not claim a source path if it is not present in the snippet metadata or source files.',
    '',
    `Query: ${query}`,
    '',
    '<memory-snippets>',
    memoryLines.join('\n\n'),
    '</memory-snippets>',
    ...(sourceBlock ? ['', sourceBlock] : []),
  ].join('\n');
}
```

핵심 지시:
- **"only the provided…"** — LLM이 외부 지식으로 환각하지 못하게
- **"<source-files> as authoritative"** — 요약본 ≠ 원본일 때 원본 우선
- **"preserve the ordered list verbatim"** — 절차/단계 손실 방지
- **"Do not claim a source path if not present"** — 가짜 출처 차단

### 2) 지식 추출 프롬프트 (문서 chunk → 구조화 record)

```ts
export function buildKnowledgePrompt(filePath: string, chunk: string): string {
  return [
    'You are a knowledge ingestion assistant running in Codex headless mode.',
    'Convert the document chunk into a compact retrieval-ready Knowledge Record.',
    'First line must be: Categories: <comma-separated category ids>',
    'Choose 1-4 category ids only from: architecture, decision, procedure, troubleshooting, requirement, security, data, api, ui, testing, deployment, reference.',
    'After that, return exactly this Markdown schema:',
    '# Knowledge Record',
    '## Type',
    'decision | constraint | procedure | troubleshooting | api-contract | domain-knowledge | reference | test-verification | working-log',
    '## Topic',
    '<short topic>',
    '## Summary',
    '<1-3 sentences>',
    '## Facts',
    '- <facts grounded in the source>',
    '## Decisions',
    '- <decisions or None identified>',
    '## Procedures',
    '- <steps/commands/workflows or None identified>',
    '## Verification',
    '- <tests/checks/expected results or None identified>',
    '## Risks',
    '- <warnings/gaps/issues or None identified>',
    '## Source',
    `- filePath: ${filePath}`,
    'Do not invent information. Keep identifiers, paths, commands, and error messages exact.',
    '',
    `File: ${filePath}`,
    '',
    '<document-chunk>',
    chunk,
    '</document-chunk>',
  ].join('\n');
}
```

핵심:
- **첫 줄 Categories**: 구조화 파싱이 쉬움
- **고정 Markdown 스키마**: 모든 record 동일 포맷 → 후처리 단순
- **None identified** 명시: 빈 섹션도 패턴으로 인식 가능

---

## 입력 사이즈 가드 (안 하면 토큰 폭발)

```ts
const MAX_QUERY_CHARS = 500;
const MAX_SNIPPETS = 5;
const MAX_SNIPPET_CHARS = 1200;
const MAX_SOURCE_FILES = 3;
const MAX_SOURCE_FILE_CHARS = 8000;
const MAX_SOURCE_FILE_TOTAL_CHARS = 18000;

// 호출 측에서 사전 truncate
const safeQuery = query.slice(0, MAX_QUERY_CHARS);
const safeSnippets = snippets.slice(0, MAX_SNIPPETS).map(s => ({
  ...s,
  content: s.content.slice(0, MAX_SNIPPET_CHARS),
}));
```

---

## 원본 파일 fallback 패턴 (RAG 품질 ↑)

검색 인덱스가 손실 요약본을 갖고 있을 때, snippet에 포함된 `sourcePath`의 원본 파일을 읽어 LLM 컨텍스트에 추가로 첨부.

```ts
import { isAbsolute, join, normalize, resolve, sep } from 'path';
import { readFileSync, statSync } from 'fs';

const ALLOWED_EXT = /\.(md|txt|rst|json|yaml|yml|toml)$/i;
const MAX_SIZE_BYTES = 8000 * 4;

/** 프로젝트 루트를 cwd부터 위로 찾아가는 휴리스틱.
 *  package.json의 name이 PROJECT_NAME과 일치하면 그곳을 root로. */
function findProjectRoot(start: string, projectName: string): string | null {
  let dir = resolve(start);
  for (let i = 0; i < 8; i++) {
    try {
      const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
      if (pkg.name === projectName) return dir;
    } catch { /* ignore */ }
    const parent = resolve(dir, '..');
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

export function resolveSourceFile(
  sourcePath: string,
  projectName: string,
  rootOverride?: string,
): string | null {
  if (!sourcePath) return null;
  const baseRoot =
    rootOverride ??
    process.env['SOURCE_ROOT'] ??
    findProjectRoot(process.cwd(), projectName) ??
    process.cwd();
  const root = resolve(baseRoot);
  const input = sourcePath.replace(/\\/g, '/');          // Windows path 호환
  const candidate = isAbsolute(input) ? resolve(input) : resolve(root, input);
  // path traversal 가드 — root 밖으로 못 나가게
  if (candidate !== root && !candidate.startsWith(root + sep)) return null;
  if (!ALLOWED_EXT.test(candidate)) return null;
  try {
    const stat = statSync(candidate);
    if (!stat.isFile() || stat.size > MAX_SIZE_BYTES) return null;
    return candidate;
  } catch {
    return null;
  }
}
```

`next dev` 처럼 하위 디렉토리에서 서버가 실행될 때도 자동으로 root를 찾아 올라갑니다.

---

## 에러 처리 — Graceful Degradation

CLI 호출 실패 시 서비스 전체가 죽지 않도록:

```ts
// 검색 라우트에서
const synthesis = runCodexHeadless(buildSearchPrompt(query, snippets, sourceFiles));
return NextResponse.json({
  results,                                    // 검색 결과 자체는 항상 반환
  ...(synthesis.ok && synthesis.content ? { answer: synthesis.content } : {}),
  ...(synthesis.error ? { answerError: synthesis.error } : {}),
  codexHeadless: synthesis.ok,
});
```

UI는 `answer`가 있으면 합성 응답을, 없으면 raw 결과만 표시. **codex 미설치 환경에서도 검색은 정상 동작**.

---

## 테스트 (vitest 기준)

```ts
import { describe, expect, it, vi } from 'vitest';

vi.mock('child_process', () => ({
  spawnSync: vi.fn().mockReturnValue({ status: 0, stdout: 'mocked output', stderr: '' }),
}));

describe('codex-headless', () => {
  it('runs codex exec with stdin prompt', async () => {
    const { spawnSync } = await import('child_process');
    const { runCodexHeadless } = await import('../lib/codex-headless');
    const result = runCodexHeadless('summarize');
    expect(result).toEqual({ ok: true, content: 'mocked output' });
    expect(spawnSync).toHaveBeenCalledWith(
      'codex',
      expect.arrayContaining(['exec', '--model', '--sandbox', 'read-only', '--skip-git-repo-check', '-']),
      expect.objectContaining({ input: 'summarize' }),
    );
  });
});
```

`spawnSync` mock으로 CI에서도 외부 binary 없이 통과.

---

## 환경변수 표 (다른 프로젝트에서 그대로 채용 권장)

| 변수 | 기본값 | 의미 |
|---|---|---|
| `CODEX_ENABLED` | `false` | 기능 자체를 켜는 마스터 스위치 |
| `CODEX_MODEL` | `gpt-5.4-mini` | 모델 (예: `gpt-5.3-codex-spark`) |
| `CODEX_CWD` | `process.cwd()` | codex 작업 디렉토리 |
| `SOURCE_ROOT` | auto-detect | 원본 파일 fallback 루트 |

---

## 호출 흐름 다이어그램

```
[사용자 요청]
      │
      ▼
[API 핸들러]   ← 입력 검증, env 게이트 (CODEX_ENABLED)
      │
      ├──▶ [메모리/DB 조회] → snippets
      │
      ├──▶ [resolveSourceFile + readFileSync] → sourceFileBlocks (선택)
      │
      ├──▶ [buildSearchPrompt / buildKnowledgePrompt]
      │           │
      │           ▼
      │      [runCodexHeadless]
      │           │ spawnSync('codex exec ... --output-last-message ...')
      │           │ stdin ← prompt
      │           ▼
      │      [readFileSync(outputFile)]
      │           │
      │           ▼
      └─◀ { ok, content | error }
            │
            ▼
[클라이언트 응답]   ← graceful: codex 실패해도 raw 결과는 반환
```

---

## 트러블슈팅

| 증상 | 원인 / 조치 |
|---|---|
| `spawn codex ENOENT` | codex 미설치. PATH 확인 또는 `which codex` |
| `empty codex output` | output 파일에 아무것도 안 쓰임. 모델명 오타 / sandbox 위반 가능 |
| 응답이 매우 느림 | spawn ~500ms + LLM 실제 시간. 호출 빈도 낮추거나 `synthesize:false` 토글 제공 |
| 한글이 깨짐 | `encoding: 'utf8'` 누락. spawnSync 옵션 재확인 |
| Windows에서 동작 안 함 | `shell: process.platform === 'win32'` 추가 (npm shim `.cmd` 호출용) |
| 원본 파일이 첨부 안 됨 | `process.cwd()`가 예상과 다름 (예: Next.js apps/web 디렉토리). `findProjectRoot` 또는 `SOURCE_ROOT` env 사용 |
| 토큰 한도 초과 | 위 "입력 사이즈 가드" 섹션의 cap 적용 |

---

## 변형 — 다른 LLM CLI도 동일 패턴

memoria에는 `claude -p` CLI도 같은 spawnSync 패턴으로 호출합니다 (git push 훅에서 `memory.md` 재작성용):

```ts
spawnSync('claude', ['-p', prompt, '--model', 'claude-haiku-4-5-20251001'], {
  input: '', encoding: 'utf8', timeout: 60_000,
});
```

상호 교체 가능한 추상화:

```ts
type CliProvider = 'codex' | 'claude';

export function runCli(provider: CliProvider, prompt: string): CodexResult {
  if (provider === 'codex') return runCodexHeadless(prompt);
  if (provider === 'claude') return runClaudeHeadless(prompt);
  throw new Error(`unknown provider: ${provider}`);
}
```

---

## 체크리스트 — 다른 프로젝트 도입 시

- [ ] `which codex` 로 binary 확인
- [ ] `lib/codex-headless.ts` 복사 (위 헬퍼)
- [ ] 프로젝트 이름에 맞춰 env 변수 prefix 변경 (`CODEX_*` → `MYAPP_CODEX_*`)
- [ ] 프롬프트 빌더 작성 (검색용 / 지식추출용 / 또는 도메인 특화)
- [ ] 입력 사이즈 cap 적용
- [ ] graceful degradation — CLI 실패해도 기본 동작 보장
- [ ] vitest mock 테스트 추가 (CI 외부 binary 의존성 회피)
- [ ] `--sandbox read-only` 명시 (쓰기 작업이 진짜 필요하면 `workspace-write`로 변경)
- [ ] Windows 지원 필요 시 `shell: process.platform === 'win32'`
- [ ] env 변수 표를 README에 명시

---

## 참고 — memoria 원본 위치

| 파일 | 역할 |
|---|---|
| `memoria/apps/web/lib/codex-headless.ts` | runCodexHeadless, buildSearchPromptWithGraph, buildKnowledgePrompt, resolveSourceFile, collectSourceFileBlocks |
| `memoria/apps/web/app/api/search/route.ts` | 검색 합성 호출 예 |
| `memoria/apps/web/app/api/ingest/route.ts` | 지식 추출 호출 예 |
| `memoria/scripts/push-observer.mjs` | `claude -p` CLI 호출 예 (git push 훅) |
| `memoria/apps/web/__tests__/codex-headless.test.ts` | vitest mock 테스트 패턴 |

```
이 문서는 memoria 프로젝트 (https://github.com/HAA02/ha02/tree/develop/memoria)
의 검증된 통합 패턴을 추출한 것. 업데이트는 원본 추적 후 반영.
```
