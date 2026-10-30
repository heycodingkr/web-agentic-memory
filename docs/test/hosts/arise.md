# 호스트: Arise 브라우저

형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿이다. 직접 실행해 확인한 사실만 적고, 확인하지 못한 항목은 비워 둔다. 아래 CDP는 격리 테스트 하네스가 fixture 브라우저를 제어할 때 쓰는 transport다. Arise 제품의 브라우저 MCP는 `agent_browser --mcp-server`와 앱의 로컬 제어 채널을 사용하며, WAM 연동에 CDP가 필요하다는 뜻은 아니다.

```
호스트: Arise 0.1.54 (/Applications/Arise.app, 번들 ID ai.meshcode.agent-browser), 2026-10-27 확인
설치/등록 명령: browser MCP는 앱에 내장. WAM stdio/remote MCP 구현과 웹 가이드 제공. 아래 선택형 bridge를 native macOS에서 검증
설정 파일 경로와 형식: $XDG_CONFIG_HOME/agent-browser/settings.toml (없으면 ~/Library/Application Support/agent-browser/settings.toml).
  키: language, auto_update_check, telemetry_enabled, web_agentic_memory_enabled, web_workflow_contribution,
  browser_memory_auto_recall, agent_workspace (바이너리 문자열과 기존 하네스로 확인)
환경변수 상속 여부: 앱이 읽는 변수를 바이너리에서 확인했고, 아래 격리 실행에서 쓴 것이 모두 동작했다
비대화형 실행 방법: python3 -B tests/env/arise_check.py  (설치된 바이너리를 제자리에서 격리 실행, CDP로 구동)
SMK 결과: 2026-10-29 native browser MCP bridge의 도구 발견·소유 lease·로컬 URL 정규화·recall·자체 탭 정리 통과. 기존 하네스 HARN-01 32/32, HARN-04 통과
BEH 결과: 미실행
알려진 이슈: 아래
```

## WAM 연동 계약

- WAM 공개 `/guide`가 만든 remote MCP 또는 local WAM stdio 설정을 Arise의 **Settings → MCP 허브**에 추가한다. remote MCP는 Arise 허브가 연결하고, local stdio는 `wam` helper를 실행한다.
- 같은 agent session에서 Arise browser 도구와 외부 MCP 도구 `mcp_tool_search` / `mcp_call_tool`이 보여야 한다. WAM 도구 검색 결과로 `web_memory_recall`을 선택하고, 이후 browser 작업을 진행한다.
- WAM 익명 read와 WAM 계정 OAuth/device link는 별도다. Arise 계정, 웹 로그인, 사이트 쿠키, 앱 제어 소켓은 WAM 인증이 아니다.
- Arise 브라우저 MCP의 `agent_browser --mcp-server` 프로세스는 앱 소유 로컬 제어 채널을 사용한다. Unix 계열에서는 Unix domain socket이고, Windows transport는 별도다. WAM 서버는 이 내부 socket에 직접 접속하지 않고 Arise MCP 허브 또는 공개된 versioned adapter를 사용한다.
- 선택형 `wam browser-mcp`는 공개 browser MCP를 지속 child로 연결하고 `web_memory_recall_current_tab`을 추가한다. 같은 등록에서 browser tool과 recall tool을 사용하며, selected automation target의 session/client/tab/window/lease 만료/ready를 검증한다. URL은 기기에서 정규화하고 recall 직전에 target이 같은지 다시 확인한다. 일반 external MCP call에 모델이 URL을 넣는 경로와 달리 이 tool에는 URL 입력 필드가 없다. Background 자동 recall은 제공하지 않는다.

## Native browser MCP bridge 검증 (2026-10-29)

```sh
cargo build -p wam-cli
python3 -B tests/env/check_browser_mcp_bridge.py
python3 -B -m unittest tests/lib/selftest/test_tab_adapter.py -v
```

첫 명령은 설치된 `/Applications/Arise.app/Contents/MacOS/agent_browser --mcp-server`를 제자리에서 실행한다. 기존 browser 도구 39개와 추가 recall 도구 1개를 발견하고, 새 고유 session의 자체 background example.com 탭만 열어 실제 lease proof를 얻었다. query/fragment가 local capture API 요청과 WAM 응답에서 제거됐음을 확인했고 같은 연결로 자체 탭을 닫았다. 실제 계정·모델·WAM 운영 서버는 사용하지 않았다. 자동 회귀는 외부/만료/누락 lease, 변경된 target, private/userinfo URL에서 recall이 전송되지 않는지도 검증한다.

Codex와 Claude의 일회용 HOME/설정 디렉터리에서 `wam-browser` stdio 등록·조회·삭제가 성공했다. 사용자 host 설정은 바꾸지 않았다. 에이전트 모델이 작업 중 적절히 recall하는 BEH 검증과 Arise **MCP 허브의 remote OAuth**는 이 결과에 포함하지 않는다.

한 번 실행하고 종료하는 browser MCP로는 기존 연결의 lease proof를 재사용할 수 없다. 같은 session/client 이름과 `is_mine` 표시만으로 권한을 복구했다고 판단하면 실제 도구가 permission denied를 반환한다. 따라서 child process를 host session 동안 유지한다. 또한 `ready: true`는 원하는 URL이 로드됐다는 뜻이 아니므로 acceptance는 실제 target URL도 기다린다.

수용 테스트는 [`../customer-paths.md`](../customer-paths.md)의 `CUST-05~06`, [`../../README.md`](../../README.md)의 host 구분, [`../../spec/mcp.md`](../../spec/mcp.md)의 Arise MCP 허브 계약을 따른다. 아래 CDP 레시피는 isolated test fixture 운전 전용이다.

## 격리 실행 레시피

`tests/lib/wamtest/arise.py`의 `LocalArise`가 구현한다. 사용자가 실행 중인 Arise, 프로필, 설정은 건드리지 않는다.

| 환경변수 | 값 | 목적 |
|---|---|---|
| `HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, `TMPDIR` | `/tmp/wam-<hex>/…` | 모든 앱 데이터와 소켓 경로를 일회용 루트로 (`TMPDIR`는 짧게) |
| `AGENT_BROWSER_SESSIONS_DIR`, `AGENT_BROWSER_UI_STATE`, `AGENT_BROWSER_PROFILES_DIR` | 같은 루트 | 세션, UI 상태, 프로필 격리 |
| `ARRISE_CEF_CACHE` | 같은 루트 | CEF 캐시(프로세스 싱글턴 잠금 포함) 격리 |
| `ARRISE_CEF_URL` | fixture URL | 시작 페이지 |
| `ARRISE_CEF_DEBUG_PORT` | 빈 포트 | DevTools 프로토콜 포트 (`/json/list`에서 page 선택) |
| `AGENT_BROWSER_NO_KEYCHAIN` | `1` | 키체인 프롬프트 방지 |
| `ARISE_NO_UPDATE_CHECK`, `ARRISE_CHAT_AGENT_LOOP` | `1`, `0` | 업데이트 확인과 채팅 에이전트 끔 |

설정 파일은 웹 메모리, 텔레메트리, 기여, 자동 recall, 업데이트 확인을 끈 최소값으로 쓴다. 사용자 세션 변수(`ARRISE_BROWSER_SESSION_ID`, `ARRISE_CONTROL_SOCK` 등)는 물려주지 않는다.

## 알려진 이슈와 함정

1. **서명된 릴리스 바이너리를 번들 밖으로 복사하면 시작 직후 `SIGKILL`(-9)로 종료된다.** 기존 하네스의 스켈레톤 번들 복사는 서명 안 된 dev 빌드용이다. 설치된 바이너리를 제자리에서 실행하고 데이터 경로만 환경변수로 격리한다 (`LocalArise.prepare(skeleton=False)`가 기본).
2. **격리 인스턴스의 창은 `visibilityState: hidden`, `document.hasFocus(): false`로 시작하고, 이 상태에서는 네이티브 클릭과 타이핑이 페이지에 전달되지 않는다.** `Emulation.setFocusEmulationEnabled`와 `Page.bringToFront`를 적용하면 전달된다 (`Page.ensure_focus()`).
3. 탭이 하나만 노출되므로 시나리오마다 같은 탭을 `Page.navigate`한다. 탭을 닫으면 창이 닫힌다.
4. 실행하는 동안 격리 인스턴스의 창이 잠깐 데스크톱에 나타난다.
5. 사용자의 Arise는 실행 중이어도 된다. `LSMultipleInstancesProhibited`가 없고 데이터 경로가 모두 다르다. 실행 전후 기존 Arise 프로세스 pid가 같음을 스크립트가 검사한다.
6. 위 `LocalArise` 하네스는 CDP로 직접 구동한다. 별도 `check_browser_mcp_bridge.py`는 공개 stdio MCP만 사용하며 CDP나 private socket에 직접 연결하지 않는다.
