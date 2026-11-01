# reference/ — 기존 구현 스냅샷 (읽기 전용 참고용)

새 레포에서 재사용하거나 비교할 기존 코드/문서를 복사해 둔 것이다. **여기서 수정하지 않는다.** 원본이 바뀌면 다시 복사하고 아래 출처를 갱신한다.

복사 시점: 2026-10-27, `agent-browser` HEAD `a4f260c` (작업 트리 기준, 미커밋 변경 포함 가능), `meshcode` 작업 트리 기준.
`target/`, 빌드 산출물, 개인 프로필/로그는 포함하지 않았다. 비밀값 패턴 스캔 결과: 테스트용 가짜 센티널(`memory_store.rs`)만 있음.

## agent-browser

| 경로 | 용도 |
|---|---|
| `docs/specs/web-agentic-memory.md` | 서버 카탈로그, 기여 동의, 구현 증거 (설계 기준 문서) |
| `docs/specs/browser-agent-memory-evaluation.md` | 2026-10-16 파일럿 측정 원자료와 하네스 방법 (README 수치 출처) |
| `docs/specs/browser-memory.md` | 개인 브라우저 메모리 설계 |
| `docs/specs/rich-text-browser-input.md`, `mcp-browser-ownership.md` | 리치 에디터 입력, MCP 브라우저 소유권 관련 |
| `scripts/probe_agent_memory_live.py` | 에이전트 도구 선택 A/B 하네스 (fixture, 트레이스, oracle) → `docs/test/benchmarks.md`의 기반 |
| `scripts/probe_agent_site_live.py` | 실사이트 검사/채팅 실행 (Medium, Naver) |
| `scripts/probe_web_agentic_memory.py`, `probe_web_memory_ui.py` | 웹 메모리 MCP 스모크, 관리자 UI 스모크 |
| `scripts/probe_blog_draft_live.py`, `probe_google_live.py`, `probe_mcp_chat_live.py` | 블로그 초안, Google 사이트, MCP 채팅 프로브 (Slides/Flow 벤치 참고) |
| `scripts/test-browser-memory.sh` | 메모리 크레이트 테스트 실행 |
| `assets/` (`agent_editor_fixture.html`, `mock_se_editor.html`, `medium_draft_memory.json`, `naver_draft_memory.json`, `web_agentic_memory.json`) | 위 스크립트와 크레이트가 읽는 로컬 에디터 fixture와 메모리 JSON. `probe_agent_memory_live.py`는 fixture 템플릿을, `browser_memory/src/web.rs`와 테스트는 `web_agentic_memory.json`을 `include_bytes!`로 컴파일에 포함한다. 처음 복사에서 빠져 있어 같은 날(2026-10-27) 같은 HEAD `a4f260c`에서 추가로 복사했다 (참조되지 않는 `icons/`는 제외) |
| `crates/browser_memory/` | Rust 구현: 카탈로그 캐시(`src/web.rs`), 도구, 워크플로 리포트, 테스트 (`wam-core`/`wam-mcp` 설계 시 재사용 후보) |

## meshcode

| 경로 | 용도 |
|---|---|
| `packages/billing/src/web-memory/` | 서버측 카탈로그/리포트 API, 서비스, 시드 (`seed.json`) — `wam-server` 이관 시 참고. 이 디렉터리의 서버 테스트 파일은 복사하지 않았다(결과 기록만 아래 문서에 있다) |
| `apps/admin/src/routes/web-memory/+page.svelte` | 관리자 화면 (candidate → 증거 → 게시) |
| `docs/2026-10-17-web-agentic-memory.md` | 서버 테스트 결과 |

## 주의

- 이 코드는 원래 레포의 의존성(workspace crate, Prisma 등)을 전제로 하므로 **그대로 빌드되지 않을 수 있다.** 참고/이식용이다.
- 하네스 스크립트는 원 레포의 앱 바이너리와 경로를 전제로 한다. 새 레포의 호스트 어댑터(`docs/plan/benchmark-readiness.md` W3-1)로 옮길 때 경로를 일반화한다.
- 이 스냅샷이 새 저장소의 라이선스에 자동으로 포함되는 것은 아니다. 공개 전에 원본 저장소의 라이선스와 재배포 권한을 파일 단위로 확인하고, 허용되지 않거나 출처가 불명확한 파일은 제거하거나 원본 링크로 바꾼다. 저장소 루트의 Apache License 2.0(2026-11-01)은 이 디렉터리에 적용되지 않는다. [NOTICE](../NOTICE)를 참고한다.
- 스냅샷에는 원본 저장소의 작업 트리 변경이 포함됐을 수 있다. 공개할 파일을 원본 커밋과 비교해 다시 검토한다.
