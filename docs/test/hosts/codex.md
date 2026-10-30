# 호스트 실행 문서: Codex CLI

확인일: 2026-10-29. 버전: codex-cli 0.165.0 (로컬 `codex --version`).
출처: 로컬 `codex mcp --help`, `codex mcp add --help`, `codex mcp list --help`, `codex mcp login --help`, `codex exec --help`, 공식 문서 [Model Context Protocol](https://developers.openai.com/codex/mcp).

형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿을 따른다. **확인한 값만** 적고, 확인하지 못한 항목은 "미확인"으로 둔다. 호스트가 업데이트되면 버전과 확인일을 갱신한다. 실제 모델 기반 SMK/BEH와 synthetic native CLI 검증 결과는 아래에 기록한다.

## 설치·등록 명령 (확인됨)

| 목적 | 명령 | 비고 |
|---|---|---|
| 원격 MCP 등록 | `codex mcp add <name> --url <URL>` | Streamable HTTP 서버. `--bearer-token-env-var <ENV>`는 HTTP 전용 |
| 로컬 stdio 등록 | `codex mcp add <name> [--env KEY=VALUE] -- <command>...` | `--env`는 stdio 전용 |
| 확인 | `codex mcp list [--json]`, `codex mcp get <name>` | |
| 로그인, 로그아웃 | `codex mcp login <name> [--no-browser]`, `codex mcp logout <name>` | `--no-browser`: 인증 URL을 출력하고 콜백 URL을 입력받는다 (헤드리스, SSH) |
| 제거 | `codex mcp remove <name>` | |
| 사전 등록 OAuth | `--oauth-client-id <ID>`, `--oauth-client-secret <SECRET>` | 시크릿은 `config.toml`에 저장된다. 우리 템플릿에는 쓰지 않는다 |

## 설정 파일 (확인됨)

- 사용자 설정은 `~/.codex/config.toml`, 프로젝트 설정은 `.codex/config.toml`이며 프로젝트 설정은 신뢰된 프로젝트에서만 적용된다. ChatGPT 데스크톱 앱, Codex CLI, IDE 확장이 같은 설정을 공유한다.
- 항목은 `[mcp_servers.<name>]` 표다. 로컬은 `command`, `args`, `env`, `env_vars`(전달 허용 목록), `cwd`. 원격은 `url`, `auth`(기본 `oauth`), `bearer_token_env_var`, `http_headers`, `env_http_headers`, `http_headers_helper`.
- 공통 옵션: `startup_timeout_sec`(기본 10), `tool_timeout_sec`(기본 60), `enabled`, `required`, `enabled_tools`, `disabled_tools`, `default_tools_approval_mode`(`auto`, `prompt`, `writes`, `approve`), `tools.<tool>.approval_mode`, `tools.<tool>.output_token_limit`.
- `writes` 모드는 읽기 전용으로 표시되지 않은 도구에 승인을 요구한다. recall, expand, status를 읽기 전용으로 표시해야 승인 프롬프트가 없다.
- Codex는 initialize 응답의 `instructions`를 서버 전체 지침으로 읽는다. 문서는 앞 512자를 자체 완결로 쓰라고 권한다.

## OAuth (확인됨)

- CIMD와 DCR을 지원한다. 서버가 CIMD를 광고하면(`client_id_metadata_document_supported: true`, token endpoint 인증 방식에 `none` 포함) CIMD를 우선하고, 아니면 DCR을 쓴다. 설정된 client ID가 있으면 등록 단계를 건너뛴다.
- 콜백은 loopback(`http://127.0.0.1/callback/<id>` 형태)이고 포트는 로그인 때 정해진다. authorization server는 호스트와 경로가 정확히 맞는 한 **가변 포트를 허용**해야 한다 (RFC 8252 7.3).
- 응답에 `iss`가 있으면 검증한다. 서버가 `authorization_response_iss_parameter_supported: true`를 광고하면 `iss`가 없는 응답도 거부한다.
- 로그인 때 등록 방식을 고르려면 `codex mcp login <name> --oauth-client-registration cimd` 또는 `dcr` (기본 auto).

## 환경변수 상속, 비대화형 실행, 격리

| 항목 | 값 | 상태 |
|---|---|---|
| 비대화형 | `codex exec [PROMPT]` | 확인 (`--help`) |
| 설정 덮어쓰기 | `-c key=value` (TOML 값), 예: `-c mcp_servers.<name>.url="..."` | 확인 (`--help`). 파일을 쓰지 않고 서버를 붙이는 방법 후보이며 동작은 SMK에서 확인 |
| 설정 디렉터리 격리 | `CODEX_HOME=<tmp>` | 부분 확인: 임시 디렉터리를 지정하면 Codex가 그 아래에 파일을 만들고 `codex mcp list`가 "No MCP servers configured yet"을 출력했다. 전체 격리는 SMK에서 재확인 |
| MCP 서브프로세스 환경변수 | 걸러질 수 있다 → `env`, `env_vars`로 명시 | 기존 기록([`../host-smoke.md`](../host-smoke.md) 1절)과 Codex 문서의 `env_vars` 허용 목록 |

## SMK, BEH 결과

실제 모델을 사용하는 SMK/BEH는 미실행이다. `python3 -B tests/env/check_host_traces.py --host codex`는 설치된 CLI를 loopback synthetic Responses API와 owned stdio MCP에 연결해 도구 실행·독립 저장 oracle·JSONL 정규화·종료를 검증한다. 2026-10-29 Codex 0.165.0에서 통과했으며 유료 모델/실제 계정을 사용하지 않았다. [`tests/bench/README.md`](../../../tests/bench/README.md#native-cli-and-generic-mcp-hosts)에 실행기와 증거를 정리했다. 실제 모델 검증 후 [`../host-smoke.md`](../host-smoke.md) 6절 표 형식으로 기록한다. 이 호스트의 시나리오는 [`../tool-forms/README.md`](../tool-forms/README.md)의 `CX` 항목이다.

## 알려진 이슈와 위험

- (기록) MCP 서브프로세스가 환경변수를 걸러 설정 디렉터리가 어긋나면서 기존 벤치마크가 무효가 된 사례가 있다. 서버가 실제로 읽은 설정 디렉터리를 `status`로 확인하고, 어긋나면 그 측정은 버린다.
- (위험) 한 endpoint가 익명 읽기와 OAuth 쓰기를 함께 제공할 때, 연결 단계에서 401이 없으면 Codex가 OAuth를 시작하는지 **미확인**이다. 시나리오 MCPF-15가 확인한다.
- (위험) 도구 승인 모드 `writes`에서 recall, expand, status가 읽기 전용으로 표시되지 않으면 읽기 호출에도 승인 프롬프트가 뜬다. 시나리오 MCPF-04가 확인한다.
