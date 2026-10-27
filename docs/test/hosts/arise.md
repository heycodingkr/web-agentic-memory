# 호스트: Arise 브라우저

형식은 [`../host-smoke.md`](../host-smoke.md) 4절 템플릿이다. 직접 실행해 확인한 사실만 적고, 확인하지 못한 항목은 비워 둔다.

```
호스트: Arise 0.1.54 (/Applications/Arise.app, 번들 ID ai.meshcode.agent-browser), 2026-10-27 확인
설치/등록 명령: 앱에 내장. 새 메모리 MCP(wam-mcp)는 아직 없어 등록 절차는 미확인
설정 파일 경로와 형식: $XDG_CONFIG_HOME/agent-browser/settings.toml (없으면 ~/Library/Application Support/agent-browser/settings.toml).
  키: language, auto_update_check, telemetry_enabled, web_agentic_memory_enabled, web_workflow_contribution,
  browser_memory_auto_recall, agent_workspace (바이너리 문자열과 기존 하네스로 확인)
환경변수 상속 여부: 앱이 읽는 변수를 바이너리에서 확인했고, 아래 격리 실행에서 쓴 것이 모두 동작했다
비대화형 실행 방법: python3 -B tests/env/arise_check.py  (설치된 바이너리를 제자리에서 격리 실행, CDP로 구동)
SMK 결과: 해당 없음 (wam-mcp 미구현). 하네스 검증만: HARN-01 32/32, HARN-04 통과
BEH 결과: 미실행
알려진 이슈: 아래
```

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
6. 앱 안의 에이전트가 쓰는 MCP 도구(`browser_*`)로 구동하는 경로는 이 문서의 범위가 아니다. 이 하네스는 CDP로 직접 구동한다.
