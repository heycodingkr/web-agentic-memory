# 도구 형태별 테스트 시나리오 (MCP · CLI · 스니펫)

상태: 시나리오 정의 111개. 구현과 실행 결과는 없다. 사람들이 Claude Code, Codex CLI에서 쉽게 설치하고 쓰는 **도구 형태**(원격 MCP, 로컬 MCP, CLI, instruction 스니펫)를 형태, 호스트, 단계로 분류해 모은 시나리오 모음이다. 요구사항 원본은 [`../../spec/install.md`](../../spec/install.md)와 [`../../spec/cli.md`](../../spec/cli.md)다.

## 1. 다른 테스트 문서와의 관계

- 이 디렉터리는 `INST`, `MCPF`, `CLI`, `SNIP` ID만 새로 정의한다. 기존 ID(`SMK`, `HOST`, `BEH`는 [`../host-smoke.md`](../host-smoke.md), `MCP`, `LINK`, `API`, `SCORE`는 [`../functional-tests.md`](../functional-tests.md), `FLOW`는 [`../user-flows.md`](../user-flows.md), `ACC`는 [`../feature-acceptance.md`](../feature-acceptance.md), `RCL`, `MEM`은 [`../functional-tests.md`](../functional-tests.md)와 [`../../spec/`](../../spec/README.md))는 다시 정의하지 않고 각 표의 "추적" 칸에서 연결한다.
- 케이스 ID는 문서에서 먼저 합의하고 테스트 코드가 새 ID를 만들지 않는다 ([`../../../tests/README.md`](../../../tests/README.md)). 이 디렉터리의 ID가 합의 대상이다.
- 호스트별 확인된 설치 명령, 설정 형식, 한도는 [`../hosts/claude-code.md`](../hosts/claude-code.md), [`../hosts/codex.md`](../hosts/codex.md)에 있다. 구현 위치는 `tests/smoke/`(호스트 스모크)를 제안한다.

## 2. 분류 체계

| 축 | 값 | 설명 |
|---|---|---|
| 형태 | `MCPR`, `MCPL`, `CLI`, `SNIP`, `전체` | 원격 MCP, 로컬 stdio MCP, `wam` CLI, instruction 스니펫. `전체`는 형태와 무관한 시나리오 ([`../../spec/install.md`](../../spec/install.md) 2절) |
| 호스트 | `CC`, `CX`, `GEN`, `-` | Claude Code, Codex CLI, 범용 MCP 클라이언트(MCP Inspector 또는 스크립트), 호스트 무관. Arise는 HOST-05, Meta Muse CLI는 SNIP-07로 다룬다 |
| 단계 | 설치, 등록, 인증, 사용, 자동화 계약, 수명주기, 진단·장애, 보안, 프로토콜 준수 | 설치(바이너리 채널, 플랫폼), 등록(호스트 등록, 범위, 설정 위생), 인증, 사용(도구, 호출), 자동화 계약(종료 코드, `--json`), 수명주기(업그레이드, 제거, 재시작, 캐시), 진단과 장애, 보안, 프로토콜 준수 |
| 계층 | `L2`, `L3`, `L4`, `L5` | [`../README.md`](../README.md) 2절의 테스트 계층이다 (계약, 웹과 템플릿, 호스트 스모크, 보안). 메모리 레이어 L0~L2와 다르다 |
| 자동화 | `자동`, `반자동`, `수동` | CI 또는 야간 스크립트, 사람의 승인 1단계가 필요(OAuth 승인 등), 실제 계정과 OS 확인이 필요 |
| 우선 | `P0`, `P1`, `P2`, `M` | 필수 통과, 권장, 낮음, 측정값(합격과 불합격을 가르지 않는다) |

각 시나리오 표의 "분류" 칸은 `형태 · 호스트 · 계층 자동화 · 우선` 순서다. 형태가 둘 이상이면 `/`로 잇는다 (예: `MCPR/MCPL`).

## 3. 파일과 ID

| 파일 | 형태 | ID | 시나리오 수 | 단계 |
|---|---|---|---:|---|
| [`install.md`](install.md) | 설치, 등록, 수명주기 (형태 공통) | INST-01~48 | 48 | 설치, 등록, 인증, 사용, 수명주기, 진단·장애 |
| [`mcp-form.md`](mcp-form.md) | 원격·로컬 MCP | MCPF-01~27 | 27 | 인증, 사용, 수명주기, 진단·장애, 보안, 프로토콜 준수 |
| [`cli-form.md`](cli-form.md) | CLI | CLI-01~33 | 33 | 인증, 사용, 자동화 계약, 수명주기, 진단·장애, 보안 |
| [`snippet-form.md`](snippet-form.md) | instruction 스니펫 | SNIP-01~10 | 10 | 등록, 사용 |

## 4. 분류 매트릭스 (형태 × 단계)

시나리오 파일의 분류 칸에서 만든 표다. 시나리오를 추가하거나 바꾸면 이 표를 갱신한다.

| 형태 \ 단계 | 설치 | 등록 | 인증 | 사용 | 자동화 계약 | 수명주기 | 진단·장애 | 보안 | 프로토콜 준수 |
|---|---|---|---|---|---|---|---|---|---|
| MCPR | - | INST-13~14, INST-25~28 | INST-29~32, MCPF-15~17, MCPF-26 | INST-35, MCPF-01~14, MCPF-25 | - | INST-39~40, MCPF-18 | INST-44~45, MCPF-21 | MCPF-22, MCPF-27 | MCPF-24 |
| MCPL | INST-01~12 | INST-15~16, INST-26~28 | INST-33, MCPF-17, MCPF-26 | INST-35, MCPF-01~02, MCPF-04~14, MCPF-25 | - | INST-37~40, MCPF-18~20 | INST-43, INST-45~47 | MCPF-22 | MCPF-23~24 |
| CLI | INST-01~12, INST-48 | INST-17~21 | INST-32~33, CLI-01~03, CLI-31 | INST-36, MCPF-09, CLI-04~18, CLI-32~33 | CLI-19~23 | INST-37, INST-41, CLI-24 | INST-45, CLI-25~27 | CLI-28~30, CLI-31~33 | - |
| SNIP | - | SNIP-01~03 | - | SNIP-04~10 | - | - | - | - | - |
| 전체 | - | INST-22~24 | - | INST-34 | - | INST-42 | - | - | - |

## 5. 호스트별 필수 세트

| 호스트 | P0 필수 | P1 권장 | P2 | M 측정값 | 전체 |
|---|---:|---:|---:|---:|---:|
| CC | 39 | 11 | 0 | 5 | 55 |
| CX | 39 | 9 | 0 | 4 | 52 |
| GEN | 7 | 0 | 0 | 0 | 7 |
| - | 24 | 16 | 2 | 0 | 42 |

| 호스트 | P0 필수 시나리오 |
|---|---|
| CC | INST-13, INST-15, INST-17~20, INST-22~24, INST-29, INST-33, INST-35~37, INST-39, INST-41, INST-43~44, INST-47~48, MCPF-05~07, MCPF-10~13, MCPF-15~17, MCPF-19~22, CLI-25, SNIP-01~02, SNIP-08~09 |
| CX | INST-14, INST-16~20, INST-22~24, INST-30, INST-33, INST-35~37, INST-40~41, INST-43~44, INST-46~48, MCPF-05~07, MCPF-10~13, MCPF-15~17, MCPF-20~22, CLI-25, SNIP-01~02, SNIP-08~09 |
| GEN | INST-31, MCPF-01~02, MCPF-08~09, MCPF-23~24 |
| 호스트 무관 (`-`) | INST-01~03, INST-06, CLI-01~03, CLI-05, CLI-07~08, CLI-11~21, CLI-26~28 |

측정값(`M`)인 시나리오: INST-34, MCPF-03, CLI-23, SNIP-04~05, SNIP-10. 호스트별로 보고하고 합격 조건으로 쓰지 않는다 ([`../host-smoke.md`](../host-smoke.md)의 SMK-06 방침과 같다).

## 6. 합격 기준 (초안, 합의 필요)

- 필수 호스트(`CC`, `CX`)마다 `P0` 시나리오가 통과해야 한다. `반자동`과 `수동`은 릴리스 전에 실행한 증거(트레이스, 화면 기록, 로그)로 갈음할 수 있다.
- 최소 2개 호스트가 통과해야 M3 게이트를 넘는다 ([`../host-smoke.md`](../host-smoke.md) 5절). 호스트 실행은 같은 커밋과 같은 서버 빌드로 하고 호스트 버전을 기록한다.
- `P1`은 릴리스 전에 실행하고 실패하면 이슈로 등록한다. `P2`는 시간이 될 때 실행한다.
- MCPF-15(익명 읽기와 인증 쓰기를 같은 endpoint에 둘 때 host OAuth 시작 여부)는 위험 항목이다. 구현 전에 두 호스트에서 확인하고, 자동 시작이 안 되면 명시적 host login 또는 local stdio + `wam link`로 전환할 수 있는지 확인한다.

## 7. 실행 규약

- 모든 실행은 일회용 HOME, XDG 설정, TMPDIR에서 하고 사용자의 실제 설정, 프로필, 키를 건드리지 않는다. 자기 프로세스만 정리한다.
- Claude Code는 `claude -p ... --mcp-config <파일> --strict-mcp-config`로, Codex는 `CODEX_HOME=<임시 디렉터리> codex exec ...`로 격리한다 (확인 상태는 호스트 문서 참고). macOS에서 TMPDIR는 `/tmp/wam-*`처럼 짧게 둔다.
- 설치 문자열은 템플릿에서 생성된 것을 **그대로** 실행한다 (INST-22). 손으로 다시 쓴 명령으로 통과시키지 않는다.
- 완료는 명시적 Done 또는 Failed 이벤트로 판정한다. 로그가 조용한 것은 증거가 아니다. 증거는 `target/e2e-logs/smoke/<run-id>/`에 남긴다.
- 호스트 결과는 [`../host-smoke.md`](../host-smoke.md) 6절 표 형식으로 기록하고 호스트 버전을 함께 적는다.

## 8. 열린 질문

[`../../spec/install.md`](../../spec/install.md) 8절과 [`../../spec/cli.md`](../../spec/cli.md) 7절을 따른다. 시나리오와 직접 연결되는 것은 다음이다.

1. 한 endpoint의 익명 read와 인증 write가 Claude/Codex에서 어떤 OAuth UX를 보이는가, fallback이 `/guide`에서 충분히 안내되는가 (MCPF-15).
2. Claude Code 템플릿에 `alwaysLoad: true`가 필요한가 (MCPF-03).
3. 헤드리스 Linux의 토큰 저장 방식 (CLI-30).
4. 지원 OS와 아키텍처, 패키지 매니저 제공 여부 (INST-01~12).
5. Meta Muse CLI의 MCP 지원 여부 (SNIP-07).
