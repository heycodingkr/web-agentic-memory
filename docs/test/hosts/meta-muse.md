# 호스트 실행 문서: Meta Muse (Muse Code, Meta Model API)

확인일: 2026-11-01. 확인한 버전: Muse Code 1.4.14(`1.4.14-R7309.1`), Claude Code 2.1.316, Codex CLI 0.166.0. 모델은 모두 `muse-spark-1.3`이고 키는 계정의 Meta Model API 키다.
출처: 로컬 실행(`muse --help`, `muse mcp --help`, `muse exec --json`), 공식 문서 [Muse Code 개요](https://dev.meta.ai/docs/muse-code), [확장·자동화(MCP)](https://dev.meta.ai/docs/muse-code/extending), [설정](https://dev.meta.ai/docs/muse-code/configuration), [코딩 에이전트에 Model API 쓰기](https://dev.meta.ai/docs/coding-agents).

형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿을 따른다. **실행해서 확인한 값만** 적고 확인하지 못한 항목은 "미확인"으로 둔다. 이 문서는 계획서 W0-3(Meta 코딩 에이전트의 MCP 지원 조사)과 W1-3의 결과이며, 아래 표는 "Muse 모델을 쓰는 에이전트가 WAM MCP를 실제로 호출하는가"를 경로별로 답한다.

## 결과 요약

| 경로 | 모델 연결 | WAM 연결 | 결과 |
|---|---|---|---|
| **Muse Code** (Meta의 터미널 코딩 에이전트) | 로그인 또는 `META_API_KEY` | `settings.json`의 `mcpServers.web-memory`, `streamable_http` | ✅ 로컬 서버와 **공개 운영 주소** 모두에서 `web_memory_status`와 `web_memory_recall` 성공 |
| **Claude Code** + Meta Model API (Messages) | `ANTHROPIC_BASE_URL=https://api.meta.ai` 등 환경변수 | `claude mcp add --transport http`와 같은 HTTP MCP | ✅ 성공. **tool search를 끄면** 3턴, 켜면 7턴 |
| **Codex** + Meta Model API (Responses) | `[model_providers.meta]`, `wire_api="responses"` | `codex mcp add web-memory --url …` | ❌ 모델이 도구를 불렀지만 Codex가 `unsupported call`로 거부. 원인 아래 |
| Arise 브라우저 | Meta Model API 키(API 키 provider) | Arise MCP Hub | agent-browser 브랜치 `feat/muse-provider`(커밋 `5b3a425`, `release/0.1.62` 기준)에 구현·커밋. `llm`·`host` 테스트는 통과했고 **`chat`(GPUI)은 컴파일하지 못했다**(빌드 머신에서 `cargo test -p chat` 필요). 키 입력 필드는 없고 env `AGENT_BROWSER_MUSE_API_KEY` 또는 settings.toml `[muse] api_key`로 넣는다. 워크스페이스 이슈 #170 |

공통 시험 요청은 웹 가이드의 첫 요청과 같다: `web_memory_status`를 호출한 뒤 `web_memory_recall`을 `{"url":"https://docs.google.com/document/","task_category":"compose-draft","keywords":["document"],"limit":3}`로 호출하고, 읽기 전용으로 아무것도 열거나 편집하지 않는다.

## Muse Code

| 항목 | 값 | 상태 |
|---|---|---|
| 설치 | macOS·Linux `curl -fsSL https://dev.meta.ai/install.sh | sh`, Windows `irm https://dev.meta.ai/install.ps1 | iex` | macOS에서 확인. 스크립트가 `muse` 런처를 `~/.local/bin`에 두고 PATH를 셸 프로필에 추가한다. 런처가 릴리스 매니페스트의 SHA-256을 검증한 뒤 바이너리(약 400MB)를 같은 폴더에 둔다 |
| 설치 격리 | `MUSE_INSTALL_DIR`, `MUSE_NO_MODIFY_PATH=1`, `MUSE_NO_AUTO_UPDATE=1`, `MUSE_AUTH_PATH`, `XDG_CONFIG_HOME` | 확인. 이 값들로 임시 폴더에만 설치해 시험했다 |
| 인증 | 첫 실행의 브라우저 로그인 또는 API 키, 비대화형은 `META_API_KEY` | `META_API_KEY`로 확인. 계정의 `MODEL_API_KEY`와 같은 키 값을 넣었다 |
| 설정 파일 | `~/.config/muse/settings.json`(`$XDG_CONFIG_HOME/muse/settings.json`) | 확인. **`schema_version`이 필수**다. 없으면 `malformed settings file … missing field schema_version`으로 실행이 거부된다. 값 `1`로 동작했다 |
| MCP 서버 항목 | `mcpServers.<이름>`에 `transport`, `url` | `{"schema_version":1,"mcpServers":{"web-memory":{"transport":"streamable_http","url":"https://api.webagenticmemory.com/mcp"}}}`로 동작 확인 |
| 항목 이름 | 문서 본문은 `mcp_servers`, CLI 도움말은 `mcpServers` | `mcpServers`만 시험했다. `mcp_servers`는 미확인 |
| OAuth | `muse mcp login <server> [--headless] [--callback-port <PORT>]`, `muse mcp logout <server>` | 명령 존재만 확인. WAM OAuth와의 실제 로그인은 **미확인** |
| 비대화형 | `muse exec --json --disable-approval --max-model-steps 10 --model muse-spark-1.3 --reasoning-effort low "<prompt>"` | 확인. 종료 코드 0 |
| 호출되는 도구 이름 | `mcp__web_memory__web_memory_status`, `mcp__web_memory__web_memory_recall` | 확인(서버 이름 `web-memory`가 `web_memory`로 정규화된다) |
| 다른 호스트에서 가져오기 | `/migrate`가 Claude Code·Codex의 MCP 서버를 가져온다 | 문서에만 있음. 미확인 |

시험 결과(`run.usage`): 로컬 서버에서 모델 호출 3회, 입력 85,771토큰(캐시 56,418), 출력 783토큰. 공개 주소에서 3회, 입력 85,372(캐시 56,546), 출력 523. 콘솔 정가(1M 토큰당 입력 $1.25, 캐시 입력 $0.15, 출력 $4.25)로 환산하면 각각 약 $0.048, $0.047이다. 비용의 대부분은 호스트의 시스템 프롬프트와 도구 정의다.

## Claude Code + Meta Model API

`ANTHROPIC_BASE_URL=https://api.meta.ai`, `ANTHROPIC_AUTH_TOKEN`(Bearer), `ANTHROPIC_MODEL`과 opus·sonnet·haiku 별칭, `CLAUDE_CODE_SUBAGENT_MODEL`을 모두 `muse-spark-1.3`으로 고정하고, 설정 폴더는 `CLAUDE_CONFIG_DIR`로 임시 폴더에 격리했다. WAM은 `--mcp-config`의 `{"type":"http","url":…}`로 붙였다(`claude mcp add --transport http`와 같은 HTTP MCP).

| 설정 | 결과 |
|---|---|
| `ENABLE_TOOL_SEARCH=true` (Meta 문서의 권장값) | 성공했지만 **7턴**. 모델이 `ToolSearch`를 4번 반복했다. 호출마다 `tool_reference`가 돌아오는데 모델은 "Tool schema load came back empty"라고 말하며 다시 찾았고, 마지막에 이름으로 직접 호출해 성공했다. 입력 52,666토큰(캐시 32,211), 출력 1,709. 정가 환산 약 $0.038 |
| `ENABLE_TOOL_SEARCH=false` | 성공, **3턴**. `web_memory_status`와 `web_memory_recall`을 바로 호출. 입력 57,366토큰, 출력 549. 정가 환산 약 $0.074(캐시가 없었다) |

- WAM은 도구가 10개뿐이므로 tool search가 필요 없다. **끄는 것을 권장한다.**
- `false` 실행에서 모델이 지시에 없던 `kind: "gotcha"` 인수를 recall에 붙였고 결과가 `hits: []`였다. 같은 요청의 다른 실행(`true`)은 인수를 그대로 써서 힌트 3개를 받았다. 모델마다 선택 인수를 임의로 채울 수 있으므로 첫 요청에 **"이 인수 그대로, 다른 필드 없이"**를 넣는다.
- Claude Code가 표시한 비용(`total_cost_usd` 약 $0.30~0.32)은 Claude Code가 모르는 모델에 적용한 자체 단가다. 위의 정가 환산 값을 쓴다.

## Codex + Meta Model API: 동작하지 않는다

결과: `unsupported call: mcp__web_memory.web_memory_status`. 모델은 도구를 올바르게 골랐지만 Codex가 호출을 라우팅하지 못해 두 호출 모두 실패했고, 모델은 "web_memory 도구가 켜져 있지 않은 것 같다"고 보고했다.

원인을 두 단계로 분리해 확인했다.

1. Codex 0.166이 Meta로 보내는 요청의 `tools[]`에서 MCP 서버는 `{"type":"namespace","name":"mcp__web_memory","tools":[…]}` 한 항목이다. 로컬 로깅 프록시로 가로채서 확인했다.
2. 이 `namespace` 도구를 그대로 Meta의 `POST /v1/responses`에 보내면 응답의 `function_call`이 `"name":"mcp__web_memory.web_memory_status"`이고 **`namespace` 필드가 없다**(네임스페이스가 이름 앞에 점으로 붙는다). 같은 도구를 평면 `function`으로 주면 `"name":"web_memory_status"`로 정상이다. Codex는 `namespace`와 `name`을 따로 받아야 라우팅하므로 이 이름을 처리하지 못한다.

시도한 우회: `--enable non_prefixed_mcp_tool_names`는 네임스페이스 이름만 `web_memory`로 바꾸고 여전히 `namespace` 타입이라 효과가 없었다. 맞춤 `model_catalog_json`으로 도구 형식을 바꾸는 방법은 **미시도**다.

해석: Codex의 namespace 도구와 Meta Responses API 사이의 호환성 문제로 보이며 WAM 쪽 문제가 아니다. Meta에 보고할 항목이다(Discord `#questions`, `#office-hours`). Codex를 기본 모델(OpenAI)로 쓰는 기존 WAM 경로에는 영향이 없다. 가이드는 Codex에 Muse 모델을 쓰는 구성을 안내하지 않는다.

## 공개 운영 주소 (2026-11-01)

`https://api.webagenticmemory.com/mcp`에 Muse Code로 읽기 전용 호출을 했다.

- `web_memory_status`: `{"auth":"anonymous","catalog_version":1,"consent":"not_linked","endpoint":"https://api.webagenticmemory.com/mcp","status":"ok"}`
- `web_memory_recall`(위 시험 요청): `{"hits":[],"reason":"ok"}`. **운영 카탈로그가 비어 있다**(#159와 일치). 영상에서 이 주소의 recall로 힌트를 보여 주려면 검증한 메모리를 먼저 승인해야 한다.

## 알려진 이슈와 위험

- (확인) Muse Code 설정 파일은 `schema_version`이 없으면 실행이 거부된다. 문서에 이 필드 설명이 없다.
- (확인) 모델이 선택 인수를 임의로 채울 수 있다. 첫 요청에 인수를 고정한다.
- (확인) Claude Code + Meta의 tool search는 켜면 도구를 불러오는 데 턴을 더 쓴다.
- (확인) 호스트의 시스템 프롬프트가 입력 토큰의 대부분이다. 한 번의 짧은 시험이 약 $0.05~0.07이다.
- (주의) Meta 콘솔은 무료 크레딧을 쓰는 동안 입출력이 제품 개선에 쓰일 수 있다고 표시한다([Meta 모델 사용](../../design/meta-model-usage.ko.md)). 시험 프롬프트에는 공개 URL 패턴만 넣었다.
- (주의) Muse Code 설치 스크립트는 기본적으로 셸 프로필(`.zshrc` 등)에 PATH 줄을 추가하고 자동 업데이트를 켠다. 시험은 격리 환경변수로 막았다.

## 미확인

- `muse mcp login web-memory`로 WAM OAuth(DCR·PKCE) 로그인과 계정 도구(`web_memory_my_rank` 등).
- 로컬 stdio(`wam-mcp`)를 Muse Code에 붙이는 방법. `mcp_servers` 키 이름, `-c mcpServers=…` 한 줄 설정.
- Muse Code에서 브라우저 도구와 WAM의 공존, 실제 브라우저 작업 완료(BEH)와 시간·호출 측정.
- Windows·Linux, Muse Wonder, 오픈 웨이트 모델.
- Codex 우회(맞춤 model catalog).

## 재현

키는 환경변수로만 넘기고 출력하지 않는다. 임시 폴더에서 실행해 사용자 설정을 건드리지 않는다.

```sh
# 0) 로컬 WAM (격리된 데이터 폴더)
WAM_BIND=127.0.0.1:3917 WAM_DATA_DIR=/tmp/wam-muse/data target/debug/wam-server &

# 1) Muse Code
export MUSE_INSTALL_DIR=/tmp/wam-muse/bin MUSE_NO_MODIFY_PATH=1 MUSE_NO_AUTO_UPDATE=1
export HOME=/tmp/wam-muse/home XDG_CONFIG_HOME=/tmp/wam-muse/xdg MUSE_AUTH_PATH=/tmp/wam-muse/xdg/muse/auth.json
curl -fsSL https://dev.meta.ai/install.sh -o install.sh && bash install.sh      # 먼저 스크립트를 읽어 본다
mkdir -p "$XDG_CONFIG_HOME/muse" && cat > "$XDG_CONFIG_HOME/muse/settings.json" <<'JSON'
{ "schema_version": 1,
  "mcpServers": { "web-memory": { "transport": "streamable_http", "url": "http://127.0.0.1:3917/mcp" } } }
JSON
META_API_KEY="$MODEL_API_KEY" /tmp/wam-muse/bin/muse exec --json --disable-approval --max-model-steps 10 \
  --model muse-spark-1.3 --reasoning-effort low "<첫 요청>"

# 2) Claude Code (설정 폴더 격리, tool search 끔)
CLAUDE_CONFIG_DIR=/tmp/wam-muse/claude ANTHROPIC_BASE_URL=https://api.meta.ai ANTHROPIC_AUTH_TOKEN="$MODEL_API_KEY" \
ANTHROPIC_MODEL=muse-spark-1.3 ANTHROPIC_DEFAULT_OPUS_MODEL=muse-spark-1.3 ANTHROPIC_DEFAULT_SONNET_MODEL=muse-spark-1.3 \
ANTHROPIC_DEFAULT_HAIKU_MODEL=muse-spark-1.3 CLAUDE_CODE_SUBAGENT_MODEL=muse-spark-1.3 ENABLE_TOOL_SEARCH=false \
claude -p "<첫 요청>" --mcp-config mcp.json --strict-mcp-config \
  --allowedTools "mcp__web-memory__web_memory_status,mcp__web-memory__web_memory_recall" --output-format stream-json --verbose
```
