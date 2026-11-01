# INST: 설치와 호스트 등록 (Claude Code · Codex CLI)

상태: local 등록 도우미 구현. source checkout의 wam/wam-mcp 바이너리와 remote HTTP MCP가 있으며, wam install/uninstall은 공통 guides/install-templates.json을 사용해 호스트 자체 CLI를 호출한다. dry-run, 멱등 등록, 충돌 보존, 범위 선택과 대칭 제거를 검증했다. Codex는 현재 user scope, Claude는 user/local/project다. 실제 두 CLI를 일회용 HOME에서 검사했고 remote OAuth discovery/DCR/PKCE와 격리 native Codex OAuth login도 합성 계정으로 검증했다. `wam guide print --write`는 현재 프로젝트의 host instruction 파일에 WAM 마커만 갱신한다. signed release와 live GitHub·전체 host acceptance는 남아 있다. [검증 범위](../test/cli-link.md).
영역 / 테스트 ID 접두사: `INST` (시나리오 원본: [`../test/tool-forms/install.md`](../test/tool-forms/install.md))
기반 설계: [`../design/architecture.md`](../design/architecture.md) 3·4·8절, [`mcp.md`](mcp.md), [`link.md`](link.md), [`recall.md`](recall.md)

## 1. 범위

사람이 Claude Code 또는 Codex CLI에서 **복사한 명령 한두 개로 메모리를 설치·등록·확인·제거**할 수 있어야 한다. 이 명세는 설치와 등록의 요구를 정한다. `wam` 명령은 [`cli.md`](cli.md), MCP 도구 계약은 [`mcp.md`](mcp.md), 호출 규칙은 [`recall.md`](recall.md)가 정한다. 형태별 시나리오 분류는 [`../test/tool-forms/README.md`](../test/tool-forms/README.md)다.

책임지지 않는 것: 브라우저 MCP(ChromeBridge, Playwright 등) 설치([`mcp.md`](mcp.md)는 공존 확인만 요구), Arise 내장 연동(HOST-05), 서버와 웹 배포.

## 2. 사용 형태

| 형태 | 코드 | 로컬 설치 | 로그인 | 설명 |
|---|---|---|---|---|
| 원격 MCP | `MCPR` | 없음 | 공개 read는 로그인 불필요; 쓰기는 호스트 OAuth | `/mcp` endpoint 하나를 호스트에 등록한다. 가장 쉬운 기본 경로다 ([`mcp.md`](mcp.md)) |
| 로컬 stdio MCP | `MCPL` | `wam`과 `wam-mcp` 바이너리 | 공개 read는 익명; 계정 기능은 `wam link` (device flow) | OAuth를 지원하지 않는 호스트, 로컬 캐시와 오프라인이 필요할 때 |
| CLI | `CLI` | `wam` 바이너리 | 공개 read는 익명; 계정 기능은 `wam link` | 사람이 직접, 또는 에이전트가 셸 도구로 `wam recall`을 호출한다 ([`cli.md`](cli.md)) |
| instruction 스니펫 | `SNIP` | 없음 또는 `wam` | 익명 또는 `wam link` | `AGENTS.md`, `CLAUDE.md`에 붙이는 지침. MCP를 쓰지 않는 호스트용 |

### 현재 제공 범위와 남은 수용 검증

| 항목 | 현재 제공 | 남은 범위 |
|---|---|---|
| 원격 공개 MCP | endpoint 등록 한 줄, 도구 10개, 익명 recall/expand | 실제 배포 주소와 광고하는 host에서 연결·인증 확인 |
| 로컬 CLI/stdio | Cargo 소스에서 `wam`/`wam-mcp` 각각 설치, `wam install ... --local` | 사전 빌드된 서명 artifact와 설치 script/패키지 매니저 배포 |
| 등록 scope | Codex helper는 `user`, Claude는 `user/local/project` | Codex 프로젝트 scope 자동 등록은 제공하지 않음 |
| 플랫폼 | Linux x64/ARM64, macOS Intel/ARM64, Windows x64를 package/installer가 정의 | 각 OS의 실제 native 시작·secure store·제거 증거 |

아래 최대 2명령/설치 시간은 제품 목표다. 현재 source 설치에는 package 두 개를 각각 설치하고 호스트를 등록하는 단계가 필요하며, 이 경로가 사전 빌드 바이너리의 설치 편의 기준을 충족했다고 표시하지 않는다. 현재 실행법은 [루트 README](../../README.ko.md), [서명 archive 검증 범위](../test/releases.md)를 따른다.

## 3. "쉬운 설치"의 기준

1. 경로당 사용자가 실행하는 명령은 **최대 2개**다. 원격 MCP는 등록 명령 1개로 익명 read를 시작하고, 계정 기능은 별도 host 인증을 한다. 로컬은 설치 명령 1개와 `wam install <host>` 1개다.
2. 로컬 설치가 필요 없는 경로(원격 MCP)가 있다.
3. 사용자는 비밀값을 붙여넣지 않는다. 로그인은 OAuth 또는 `wam link`로 한다.
4. 설치 확인은 호스트 명령(`claude mcp get`, `codex mcp list`) 또는 `wam doctor` 하나로 끝난다.
5. 제거는 설치와 대칭이다.
6. 실패하면 원인과 다음 행동을 알려 준다.

시간 목표(설치 시작부터 첫 recall까지)는 [`../test/metrics.md`](../test/metrics.md)에서 정한다. 이 문서에서 새 수치를 정하지 않는다.

## 4. 확인된 호스트 사실 (2026-10-27)

자세한 명령, 설정, 한도는 [`../test/hosts/claude-code.md`](../test/hosts/claude-code.md)와 [`../test/hosts/codex.md`](../test/hosts/codex.md)에 있다. 아래는 설치 설계에 영향을 주는 것만 요약한다.

| 항목 | Claude Code 2.1.310 | Codex CLI 0.164.0 |
|---|---|---|
| 원격 한 줄 등록 | `claude mcp add --transport http <name> <url>` | `codex mcp add <name> --url <URL>` |
| 로컬 등록 | `claude mcp add <name> -- <command> [args...]` | `codex mcp add <name> -- <command>...` |
| 설정 위치 | scope `local`(기본), `user`, `project`(`.mcp.json`, 승인 필요) | `~/.codex/config.toml`, 신뢰된 프로젝트의 `.codex/config.toml` |
| 확인과 제거 | `claude mcp get`, `list`, `remove` (원격 제거 시 OAuth 토큰도 삭제) | `codex mcp list --json`, `get`, `logout`, `remove` |
| OAuth 클라이언트 등록 | DCR 기본, CIMD 자동 탐지, 임의 loopback 포트 | CIMD 또는 DCR, 가변 포트 loopback(RFC 8252), `iss` 검증 |
| 도구 노출 | 기본 지연 로드(tool search). `alwaysLoad: true`는 `add-json` 또는 `.mcp.json`으로만 | `enabled_tools`, `disabled_tools`, 승인 모드 `writes`는 읽기 전용 표시가 없는 도구에 승인 요구 |
| instructions 한도 | 서버 `instructions`와 도구 설명 각각 4,096자 초과분 잘림 | 앞 512자를 자체 완결로 쓰라고 권고 |
| 시간 한도 | `alwaysLoad` 서버 시작 대기 5초, 출력 경고 10,000 토큰(기본 최대 25,000) | `startup_timeout_sec` 기본 10초, `tool_timeout_sec` 기본 60초 |
| stdio 종료 | SIGINT, 100ms 후 SIGTERM, 400ms 후 SIGKILL. Windows는 stdin 닫기 후 300ms | 미확인 |

이 표는 호스트 업데이트로 바뀐다. 설치 템플릿을 갱신할 때와 SMK를 실행할 때마다 버전과 확인일을 갱신한다.

## 5. 요구사항

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| `F-INST-01` | 원격 MCP 한 줄 등록 | Claude Code와 Codex에서 문서화된 명령 1개로 등록되고, 호스트의 확인 명령에 서버가 보이며, tools/list에 도구 10개가 나온다. 가입 없이 익명 recall이 되고, 기여·outcome에는 명시적 host 인증이 필요하다 | INST-13, INST-14, INST-35, HOST-01, HOST-02, CUST-01~02 |
| `F-INST-02` | 로컬 설치 채널 | `wam` 바이너리를 sudo 없이 사용자 디렉터리에 설치한다. 현재 채널은 `cargo install` 소스 설치다. 설치 스크립트·서명 릴리스 아카이브는 배포 전 별도 수용 검증이 필요한 목표다. 체크섬이 틀리면 설치가 중단되고 부분 설치가 남지 않는다 | INST-01~09 |
| `F-INST-03` | 등록 도우미 | `wam install <host>`가 호스트 자신의 CLI(`claude mcp add`, `codex mcp add`)를 호출해 등록한다. 멱등이고 `--dry-run`이 있으며 다른 서버를 보존한다. 호스트 CLI가 없으면 설정 파일을 직접 고치지 않고 실행할 명령을 출력한다 | INST-17~21 |
| `F-INST-04` | 복사 계약 | 설치 문자열은 호스트·형태별 **하나의 템플릿**에서 생성되고 `/guide`, `wam install`, 문서가 같은 값을 쓴다. 테스트는 복사한 문자열을 가공 없이 실행한다. 설치 경로별 사용자가 실행하는 기본 명령은 2개 이하다. 인자, 로그, 설정 파일에 토큰, 키, 개인 ID가 없고 앞뒤 공백이 없다 | INST-22~25, INST-48, SEC-H02, SMK-14 |
| `F-INST-05` | 설정 범위 | Claude Code는 `local`, `user`, `project`, 현재 Codex helper는 `user` 범위를 지원하며 기본값과 차이를 안내한다. Codex 프로젝트 범위 자동 등록은 미구현으로 표시한다. 프로젝트 범위 파일은 비밀 없이 커밋할 수 있고 호스트의 승인·신뢰 절차를 우회하지 않는다 | INST-26~28 |
| `F-INST-06` | OAuth 호환 | 원격 MCP의 authorization server는 Claude Code와 Codex의 클라이언트 등록(DCR 또는 CIMD)을 사전 등록 없이 받고, loopback redirect의 가변 포트와 `iss` 검증을 지원한다. OAuth가 불가능한 환경은 local stdio + `wam link`로 연결한다 | INST-29~33, MCPF-15~17 |
| `F-INST-07` | 설치 확인 | 호스트 확인 명령 또는 `wam doctor`로 연결, 인증 상태, 도구 목록, 버전을 한 번에 확인한다. 비밀값은 출력하지 않는다 | INST-35, INST-36, CLI-25~27 |
| `F-INST-08` | 호스트 한도 적합 | 로컬 stdio 서버는 Codex `startup_timeout_sec` 안에 initialize를 끝낸다. 서버 `instructions`는 앞 512자가 자체 완결이고 각 필드가 4,096자 이하다. 도구 출력은 호스트 경고 임계보다 한참 작다 | INST-46, INST-47, MCPF-01, MCPF-02, MCPF-08, MCPF-19 |
| `F-INST-09` | 업그레이드 | 바이너리 교체 뒤 설정, 토큰, 캐시가 유지되고 도구 스키마가 바뀌면 사용자에게 알린다 | INST-37, INST-38, SMK-13 |
| `F-INST-10` | 대칭 제거 | 설치 명령 하나에 제거 명령 하나가 대응한다. 해당 host의 WAM 등록만 지우고 다른 서버를 보존한다. 여러 host가 공유하는 로컬 토큰·서버 device는 wam unlink, 캐시는 wam cache clear로 별도 제거하며 그 명령을 안내한다 | INST-39~42, HOST-06, SMK-10 |
| `F-INST-11` | 실패 진단 | PATH 부재, 권한, 네트워크와 프록시, TLS, 체크섬, OAuth 거절, 타임아웃을 구분해 원인과 다음 행동을 보여 준다 | INST-06~08, INST-43~45 |
| `F-INST-12` | 플랫폼 | macOS, Linux, Windows에서 설치, 등록, 제거가 된다. package/installer 대상은 Linux x64/ARM64, macOS Intel/ARM64, Windows x64이며 native runtime 검증은 별도다 | INST-01~03, INST-10~12 |
| `F-INST-13` | 에이전트의 자발적 사용 | 설치 뒤 에이전트가 사이트 작업 전에 recall을 호출하도록 호스트별 템플릿(서버 instructions, Claude Code `alwaysLoad`, 스니펫)을 정한다. 호출률은 합격 조건이 아니라 측정값으로 보고한다 | INST-34, MCPF-03, SNIP-04, SNIP-05, BEH-01 |
| `F-INST-14` | 공개 CLI 설치 카드 | 가입 전 `/guide`에서 OS별 검증된 `wam` 설치 명령과 첫 익명 recall 방법을 복사할 수 있다. 설치 명령과 첫 recall은 최대 2개 사용자 명령이며 checksum·버전·제거 안내가 있고 복사만으로 설치 완료를 표시하지 않는다 | INST-48, CUST-03 |

## 6. 인터페이스와 데이터

설치 템플릿은 호스트와 형태마다 하나다. 각 템플릿은 다음을 가진다 ([`mcp.md`](mcp.md)의 설치 카드와 같다).

- 호스트 이름과 지원 transport, 템플릿 버전, 마지막 확인일
- 복사할 명령 또는 붙여넣을 설정 조각과 놓을 위치
- 인증 방식과 로그인 단계, 재시작·재인증 필요 여부
- 확인 명령, 도구 목록, 테스트 호출
- 제거 명령과 연결 해지 방법

서버 이름의 기본값은 `web-memory`다. 문자, 숫자, 하이픈, 밑줄만 쓰고 호스트의 예약 이름과 겹치지 않는다. endpoint는 배포 설정에서 채우고 소스에 하드코딩하지 않는다. 웹 `/guide`, 문서, `wam install`, 테스트는 같은 템플릿 파일을 읽는다.

## 7. 비기능과 보안 제약

- 설치 명령과 로그에 비밀값이 없다 (SEC-H02). 토큰은 호스트의 저장소 또는 OS secure store에만 둔다.
- 설치 스크립트는 HTTPS로만 받고 체크섬을 검증한다. 파일로 내려받아 확인한 뒤 실행하는 경로도 문서화한다.
- 설치와 제거는 sudo 없이, 원자적으로 동작한다. 실패해도 이전 상태가 남는다.
- 테스트는 일회용 HOME, XDG, `CODEX_HOME`에서 실행하고 사용자의 실제 설정을 건드리지 않는다 ([`../test/host-smoke.md`](../test/host-smoke.md)).

## 8. 열린 질문

1. 현재 `wam`과 `wam-mcp`는 별도 바이너리다. 향후 단일 설치물로 묶는다면 명령·제거·서명 수용 기준도 함께 갱신한다.
2. 패키지 매니저(Homebrew, winget, npm 래퍼)를 제공할지.
3. 지원 OS와 아키텍처 목록. 후보는 macOS arm64와 x64, Linux x64와 arm64, Windows x64다.
4. 한 endpoint가 익명 읽기와 인증 쓰기를 함께 제공할 때 각 호스트가 쓰기 시 OAuth를 시작하는지 (MCPF-15). 자동 시작이 안 되면 명시적 host login 안내를 제공하고, OAuth 미지원 host에는 local stdio + `wam link` 경로를 제시한다. 실제 호스트 결과 없이 설치 완료로 표시하지 않는다.
5. Claude Code 템플릿에 `alwaysLoad: true`를 쓸지. 쓰면 템플릿은 `claude mcp add-json` 1개 명령이 된다. MCPF-03의 측정 뒤에 정한다.
6. 설치 스크립트의 무결성 방식(체크섬만, 또는 서명 포함).
7. 헤드리스 Linux에서 토큰을 어디에 저장할지 ([`cli.md`](cli.md)의 열린 질문).
8. ~~Meta Muse CLI가 MCP를 지원하는지.~~ 확인했다(2026-11-01): Muse Code 1.4.14는 `settings.json`의 `mcpServers`로 streamable HTTP MCP를 지원하고 WAM의 상태·recall 호출에 성공했다. 스니펫 경로(SNIP-07)는 필요 없다. OAuth 로그인과 로컬 stdio는 미확인이다. [`docs/test/hosts/meta-muse.md`](../test/hosts/meta-muse.md)를 본다.
