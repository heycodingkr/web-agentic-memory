# MCP recall 입력 거절의 원인 추적

2026-11-07, issue #371.

## 관찰과 원인

사용자 알림의 `7658e9093604ffc3b764c699c9dc3c55`를 운영 `wam_diagnostic_events`에서 읽기 전용으로 조회했다. `2026-11-07T05:25:42Z` (`14:25:42 KST`)에 `kind=request`, `source=mcp`, `operation=recall`, `error_code=invalid_input`, `method=POST`, `status_code=200`, `duration_ms=1`인 한 행이 있다. HTTP 200은 MCP 운송 성공을 뜻하며 도구 성공을 보장하지 않는다. 원격 MCP 도구 실패는 HTTP 200과 `isError=true`로 전달된다.

`application/catalog_recall.rs`는 URL·작업 코드·키워드·종류·limit 검증을 구분하지만, `mcp_transport.rs::api_error`가 이들을 단일 `ApiError::InvalidInput`으로 바꾼다. MCP JSON 역직렬화 실패도 같은 진단으로 기록된다. 이전 저장 문서에는 이 구분이 없으며, 원시 입력도 저장하지 않는다. 따라서 이 과거 사건에서 정확히 어떤 인수가 거절됐는지, 호출자가 누구였는지는 단정할 수 없다.

## 구현 전 명세와 순서

1. 부모: git 상태·기존 diff·MCP/검증/진단 코드·테스트와 운영 trace를 확인한다.
2. 독립 수집: `codex:gpt-6-luna`는 인접 agent-browser의 WAM 호출·오류 표시 경로만 읽고 file:line을 반환한다. 부모가 원문과 주장을 확인한다. 설계·운영 조회·빌드·테스트는 부모가 맡는다.
3. 부모: 아래 테스트를 먼저 실패시킨 뒤, 서버 생성 `error_reason` 고정 enum을 원격 recall 응답·저장 문서·알림에 연결한다. 원시 값이나 에러 문자열을 사유로 쓰지 않는다. client intake는 새 필드를 받지 않는다.
4. 독립 기계적 작업: 새 필드의 기본값 `None`을 세 기존 테스트 생성자에만 추가한다. 부모가 실제 diff를 검토한다.
5. 부모: 관련 테스트·fmt·Clippy와 최종 diff를 검증하고 결과를 기록한다.

## 입력과 기대 결과

각 원격 요청은 JSON 선호와 SSE 선호 Accept로 각각 확인한다.

| 입력 | 기대 `error_reason` |
| --- | --- |
| url 누락·타입 오류·허용하지 않은 필드 | `invalid_arguments` |
| file URL | `invalid_url` |
| 어휘에 없는 task_category | `invalid_task_category` |
| 서로 다른 승인 키워드 9개 | `invalid_keywords` |
| 알 수 없는 kind | `invalid_kind` |
| limit 0 또는 4 | `invalid_limit` |
| 공개 URL, limit 1 | 성공, 오류 사유 없음 |

실패는 HTTP 200/`isError=true`이고 `_meta.trace_id`=응답 헤더=저장 trace, `_meta.error_reason`=저장 사유여야 한다. 응답과 저장 문서에 비공개 sentinel이 없어야 한다. 기존 SSE 연결 지연·RPC parse 오류·타 도구 동작도 유지한다. 구형 문서는 계속 읽으며 알 수 없는 사유 enum은 거절한다. SQLite 재개 후에도 사유가 남아야 한다. 이메일은 사유 코드와 HTTP/도구 상태의 차이를 보여 준다.

## 검증 결과

수정 전 focused 테스트 세 건에서 응답 사유 누락, SQLite 재개 후 사유 누락, 메일 사유 누락을 각각 재현했다. 수정 후 JSON·finite SSE 20개 요청의 행렬과 기존 diagnostics 회귀 검사 13개, 이메일 사유 검사가 통과했다. 행렬의 각 사례는 익명 burst 제한과 독립된 admission bucket을 사용하며 운영 제한 정책은 바꾸지 않았다.

| 검사 | 결과 |
| --- | --- |
| `cargo check -p wam-server --lib` | 통과 |
| `cargo test -p wam-server` | 34개 요약, 268 통과, 실패 0, 외부 DB 등 환경 의존 ignored 25 |
| `cargo clippy -p wam-server --all-targets -- -D warnings` | 통과 |
| 변경 파일 `rustfmt --check`, `git diff --check` | 통과 |
| 최종 diff 검토 | 부모가 타입·MCP 전파·메일·테스트 원문 검토 |

상위 agent-browser 수집 작업은 예산 한도의 partial로 종료되어 부모가 `crates/browser_memory/src/tools.rs:97`·`web.rs:143`·`web.rs:203`, `crates/chat/src/browser_context.rs:53`, `crates/agent_browser/src/mcp.rs:1458`을 직접 읽었다. 이 내장 경로는 별도 서버의 `GET /web-memory/v1/catalog`와 로컬 매칭을 사용하며, 이번 원격 WAM `/mcp`의 `web_memory_recall`을 호출하는 경로가 아니다. 상위 프로젝트의 파일은 수정하지 않았다.

기본값 전파 작업자는 파일 수정 없이 종료되어 부모가 확인한 세 struct literal을 직접 수정했다. 실제 검증은 모두 부모가 실행했다. 로그는 gitignored `build/recall-alert-tests.log`와 `build/recall-alert-clippy.log`에 있다.

운영에서는 읽기 전용 trace 조회만 수행했다. 이 변경은 로컬 작업 트리에 있으며 커밋·push·운영 배포는 하지 않았다. 새 DB 열이나 마이그레이션은 없고 기존 JSON 문서에 optional 필드를 추가한다. 과거 사건에 사유를 추정해 채우지 않는다.
