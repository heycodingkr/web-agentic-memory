# 미처리 브라우저 오류의 진단 문맥

2026-11-20, 이슈 #657. 실제 거부 원인은 미확정이며, 이 변경은 재발 시 진단에 필요한 분류를 보존한다.

## 운영에서 확인한 사실

- 대상 trace: `08a35e974f5820abfab8818a758717b9`.
- PostgreSQL을 읽기 전용으로 조회했다. 2026-11-19 15:44:55 UTC에 `web/unknown/unhandled_rejection` 한 행과 같은 trace의 telemetry POST 202 한 행만 있었다. 연결된 API 요청은 없었다.
- 확인 당시 활성 릴리스는 `20261111T030635Z-main-53d1723`이었다.
- 활성 HTML이 참조한 `app.C7ovJ_8W.js`는 `event.reason`과 `window.location.pathname`을 reporter에 전달했다. 파일 SHA-256: `0c00459b0206b645f4c0d413b9b702c9d05ae34ff61fd65e6bfe76b492bede9f`.
- 사용자는 알림 메일만 받았다. 재현할 페이지·동작이나 오류 원문은 없다. 과거 이벤트에는 메시지·스택·페이지가 저장되지 않았으므로 원래 거부된 promise를 특정하거나 복원할 수 없다.

## 확인된 진단 공백과 설계

reporter는 API 문맥이 없는 오류의 안전한 종류를 보존하지 않았다. 홈·설치 안내 등의 페이지에서는 작업 분류도 `unknown`이므로 서로 다른 오류가 같은 알림 형태가 됐다. 이 공백은 합성 TypeError·NotAllowedError 입력으로 자동 재현했다. 이는 원래 운영 오류를 재현했다는 뜻이 아니다.

선택 필드 `web_context={page,error_type}`를 클라이언트 이벤트, 저장 JSON, 알림 메일에 전달한다. 페이지는 정해진 종류로 정규화하고, 오류 이름은 허용 목록에 있는 값만 분류한다. 사용자 정의 이름과 읽을 수 없는 속성은 `unknown`, null/undefined를 제외한 원시 거부 값은 `non_error`로 남긴다. 기존 API trace·상태·경과 시간과 알림 대상 판정은 유지한다.

서버는 이 필드를 web 클라이언트 오류에만 허용한다. 추가 중첩 필드와 임의 enum 값은 반사 없이 거부한다. 메시지·스택·URL·쿼리·실제 경로는 수집하지 않는다. SQL 스키마는 바꾸지 않는다. 세부 계약은 [diagnostics 명세](../spec/diagnostics.md)를 따른다.

새 알림의 예:

```text
source=web operation=unknown error=unhandled_rejection status=none ... web_page=home web_error_type=type_error
```

## 테스트 입력과 기대 결과

| 입력 | 기대 결과 |
| --- | --- |
| mock window에 등록된 실제 훅 콜백에 TypeError, 현재 페이지 / | home/type_error, actionable 유지 |
| 비공개 메시지·쿼리·fragment를 포함한 홈·가이드 오류 | 고정 분류만 포함, PRIVATE sentinel 없음 |
| API 오류와 원래 trace·상태·경과 시간 | 원래 API 문맥 유지, 페이지·오류 종류 추가 |
| 사용자 정의 오류 이름·throwing getter·원시 거부 값 | unknown/non_error, reporter 자체는 throw하지 않음 |
| 유효한 web_context를 HTTP intake에 두 번 전송 | 202, 이벤트 하나로 중복 제거, SQLite 재개방 뒤 정확한 문맥 유지 |
| 임의 enum·추가 중첩 필드·cli/mcp/server 출처 | 400, 비공개 sentinel 반사·저장 없음 |
| web_context가 없는 기존 클라이언트·저장 JSON | 기존 동작 유지 |
| web 문맥을 넣은 server request 저장 객체 | 유효성 검사 거부 |
| 브라우저 오류를 알림 formatter에 전달 | web_page와 web_error_type 포함, MCP error_reason 형식 유지 |

수정 전 JS 검사에서 4건, Rust intake/저장 검사에서 2건, 알림 검사에서 1건이 의도한 이유로 실패했다. intake는 새 필드를 400으로 거부했고, 브라우저 reporter는 문맥을 누락했다.

## 검증과 검토

부모 세션에서 다음 검증을 직접 실행했다. 원자료는 gitignored `build/browser-context/`에 있다.

| 검사 | 결과 |
| --- | --- |
| `pnpm --dir web test` | 84 통과, 0 실패 |
| `pnpm --dir web check` | 오류 0, 기존 SetupGuide 경고 2 |
| `pnpm --dir web build` | static production build 통과 |
| `cargo test -p wam-server --jobs 2` | 34개 요약, 278 통과, 0 실패, 25 ignored |
| `cargo clippy -p wam-server --all-targets --locked --jobs 2 -- -D warnings` | 통과 |
| `cargo fmt --all -- --check`, `git diff --check` | 통과 |

외부 PostgreSQL·Redis용 25개 ignored 검사는 일회용 서비스 URL을 설정하지 않아 실행하지 않았다. 새 문맥의 저장·재개방은 SQLite로 검증했다. 서버 전체 검사의 첫 실행은 120초 컴파일 제한을 초과해, 제한을 늘리고 두 작업으로 다시 실행해 통과했다. 웹 런타임 테스트는 mock window/fetch를 사용하며 실제 운영 브라우저에서 원래 오류를 재현한 검사는 아니다.

반복 기본값 추가만 작업자에 맡겼고, 부모 세션이 실제 파일·diff를 열어 확인했다. 부모 검토에서 허용 enum과 원문 제외, web 전용 검증, 기존 문서 호환성, API trace 보존, 알림 판정 불변을 확인했다. 서버와 웹은 함께 적용해야 한다. 별도 웹 배포에서 이전 intake에 새 필드를 보내면 400이 발생한다.

## 운영 적용 범위

이 변경은 로컬 패치다. 운영 설정·데이터·릴리스를 수정하거나 테스트 알림 메일을 보내지 않았다. 새 필드를 수용하는 서버를 웹 번들보다 먼저 또는 함께 배포해야 한다. 이전 서버는 추가 필드를 거부한다.

기존 기록은 새 분류로 보강할 수 없다. 배포 이후의 진단 문맥도 원인에 대한 단서이며, 특정 런타임 결함의 수정 증거는 아니다. 원래 오류가 해결됐다고 판정하지 않고 이슈를 검토 대기로 둔다.
