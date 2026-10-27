# 호스트 실행 문서: Claude Code

확인일: 2026-10-27. 버전: Claude Code 2.1.310 (로컬 `claude --version`).
출처: 로컬 `claude mcp --help`, `claude mcp add --help`, `claude mcp remove --help`, `claude --help`, 공식 문서 [Connect Claude Code to tools via MCP](https://code.claude.com/docs/en/mcp).

형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿을 따른다. **확인한 값만** 적고, 확인하지 못한 항목은 "미확인"으로 둔다. 호스트가 업데이트되면 버전과 확인일을 갱신한다. 이 문서를 쓰는 시점에는 서버와 `wam` 바이너리가 없어 SMK, BEH를 실행하지 않았다.

## 설치·등록 명령 (확인됨)

| 목적 | 명령 | 비고 |
|---|---|---|
| 원격 MCP 등록 | `claude mcp add --transport http <name> <url>` | 문서가 원격 서버에 권장하는 방식. 성공하면 `Added ...`가 출력되고, 연결 확인은 `claude mcp get <name>` |
| 로컬 stdio 등록 | `claude mcp add <name> [-e KEY=VAL] -- <command> [args...]` | `--` 뒤는 전부 서버 실행 명령이다 |
| JSON으로 등록 | `claude mcp add-json <name> '<json>'` | `url`이 있으면 `"type": "http"`가 필수다. 없으면 stdio 항목으로 읽혀 건너뛴다. `alwaysLoad` 같은 서버 옵션은 이 방식이나 `.mcp.json`으로만 줄 수 있다 (`claude mcp add` 옵션에는 없음) |
| 확인 | `claude mcp get <name>`, `claude mcp list` | 상태: ✔ Connected, ! Needs authentication, ✘ Failed to connect, ⏸ Pending approval |
| 제거 | `claude mcp remove <name> [-s <scope>]` | scope를 생략하면 있는 scope에서 제거한다. 원격 서버를 제거하면 저장된 OAuth 토큰과 client registration도 삭제된다 |

`claude mcp add` 옵션 (`--help` 확인): `-s/--scope`(local, user, project, 기본 local), `-t/--transport`(stdio, sse, http, 기본 stdio), `-e/--env`, `-H/--header`, `--callback-port`, `--client-id`, `--client-secret`.

## 설정 범위와 파일 (확인됨)

| scope | 저장 위치 | 비고 |
|---|---|---|
| `local` (기본) | `~/.claude.json` 안의 해당 프로젝트 경로 | 개인용, 버전 관리 밖 |
| `user` | `~/.claude.json` 최상위 | 모든 프로젝트 |
| `project` | 프로젝트 루트 `.mcp.json` | 팀과 공유. 사용 전 승인이 필요하다 (⏸ Pending approval). 신뢰하지 않은 폴더에서는 저장소가 스스로 승인할 수 없다 |

구성 디렉터리는 `~/.claude`이고 `CLAUDE_CONFIG_DIR`로 바꿀 수 있다 (문서).

## OAuth 로그인 (확인됨)

- 세션 안 `/mcp`에서 인증한다. 콜백 포트는 기본적으로 사용 가능한 임의 포트이고 `--callback-port`로 고정할 수 있다.
- 클라이언트 등록은 Dynamic Client Registration(DCR)이 기본이다. Client ID Metadata Document(CIMD)도 자동으로 찾는다. DCR을 지원하지 않는 서버는 `Incompatible auth server: does not support dynamic client registration` 오류가 나고, 이때만 `--client-id` 같은 사전 등록 자격이 필요하다.
- 따라서 복사 한 줄 설치가 되려면 우리 authorization server가 **사전 등록 없이 DCR 또는 CIMD를 받아야** 한다.

## 도구 노출과 크기·시간 한도 (확인됨)

- MCP 도구는 기본적으로 **지연 로드(tool search)** 된다. 서버 설정 `alwaysLoad: true`를 주면 컨텍스트에 항상 올라간다. 이 옵션은 `claude mcp add` 플래그에 없다.
- 서버 `instructions`와 업프런트 도구 설명은 각각 4,096자를 넘으면 잘린다. tool search로 불러오는 도구 설명의 한도는 16,384자다. 핵심 문장을 앞에 둔다.
- MCP 도구 출력은 10,000 토큰을 넘으면 경고하고, 기본 최대는 25,000 토큰이다 (`MAX_MCP_OUTPUT_TOKENS`).
- 시간: 도구 호출 전체 제한은 `MCP_TOOL_TIMEOUT`(미설정이면 약 28시간)이다. HTTP·SSE 서버에는 요청 단위 타이머가 따로 있고 60초, 서버 도구 제한, `MCP_TIMEOUT` 중 가장 큰 값이다. `alwaysLoad` 서버는 시작할 때 표준 5초 연결 대기까지만 기다린다.
- stdio 서버 종료: macOS와 Linux는 SIGINT, 100ms 뒤 SIGTERM, 다시 400ms 뒤 SIGKILL이다. Windows는 stdin을 닫고 300ms 뒤 프로세스 트리를 종료한다.
- 설정 경고: 값 앞뒤의 숨은 공백(붙여넣기 줄바꿈), 같은 이름이 여러 scope에서 서로 다른 endpoint로 정의된 경우, 예약 이름(`workspace`, `claude-in-chrome`, `computer-use`, `Claude Preview`, `Claude Browser`), 없는 `${VAR}`. 서버 이름은 문자·숫자·하이픈·밑줄만 쓴다.

## 환경변수 상속, 비대화형 실행, 격리

| 항목 | 값 | 상태 |
|---|---|---|
| 비대화형 | `claude -p "<prompt>"`, `--output-format`(`--print`와 함께) | 확인 (`--help`) |
| 일회용 MCP 구성 | `--mcp-config <file>` + `--strict-mcp-config` (이 파일의 서버만 사용) | 확인 (`--help`). 사용자 설정을 건드리지 않고 서버를 붙이는 방법 |
| 서버 로드 실패 감지 | `--output-format stream-json`의 `system/init` 이벤트 `mcp_server_errors` (v2.1.219 이상) | 문서 확인 |
| 설정 소스 제한 | `--setting-sources`, `--settings <file-or-json>` | 확인 (`--help`) |
| 도구 허용 | `--allowedTools`, `--permission-mode` | 확인 (`--help`) |
| 구성 디렉터리 격리 | `CLAUDE_CONFIG_DIR` | 문서에 언급됨. 격리 동작 자체는 **미확인** (스모크에서 확인) |
| MCP 서브프로세스 환경변수 상속 | 미확인 | SMK 사전 점검으로 측정 |
| `CLAUDE_PROJECT_DIR` | stdio 서버 환경에 프로젝트 루트가 설정된다 | 문서 확인 |

## SMK, BEH 결과

미실행. 실행 후 [`../host-smoke.md`](../host-smoke.md) 6절 표 형식으로 기록한다. 이 호스트에 해당하는 시나리오는 [`../tool-forms/README.md`](../tool-forms/README.md)의 호스트 `CC` 항목이다.

## 알려진 이슈와 위험

- (위험) 한 endpoint가 익명 읽기와 OAuth 쓰기를 함께 제공할 때, 연결 단계에서 401이 없으면 Claude Code가 OAuth 인증을 시작하는지 **미확인**이다. `! Needs authentication`이 나오지 않을 수 있다. 시나리오 MCPF-15가 확인한다.
- (위험) 기본 tool search 때문에 `web_memory_recall`이 에이전트에게 바로 보이지 않아 자발적 recall 비율이 낮을 수 있다. 시나리오 MCPF-03이 `alwaysLoad` 유무를 비교한다.
