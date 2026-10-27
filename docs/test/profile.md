# PROFILE 테스트 명세

상태: PROFILE-01~05 core 단위 테스트와 PROFILE-06~08의 로컬 API 소유권/handle/opt-out 부분이 구현됐다. PROFILE-11은 파일 복원·손상·저장 실패·HTTP restart 회귀 테스트로 검증한다. PostgreSQL 동시성, 계정 비활성화와 PROFILE-09~10 전체 브라우저 여정은 별도 검증이 필요하다.

모든 계정·점수·URL은 합성 fixture를 사용한다. 공개 경계는 API 응답, DB projection, 브라우저 DOM을 각각 oracle로 확인한다.

| ID | 계층 | 검증 | 합격 기준 |
|---|---|---|---|
| PROFILE-01 | L0 | handle 공백/대소문자, 길이, 허용 문자, 예약 경로 | canonical 소문자로 정규화되고 유효하지 않은 값/예약 경로는 안정 오류 코드로 거부된다. 입력 원문은 오류에 들어가지 않는다. |
| PROFILE-02 | L0 | 문단, H2/H3, bold/italic, 목록, inline 링크의 bio schema와 상한 | 허용 노드만 생성되고 빈 문단, H1, 제어문자, 1,200자/20블록/8링크 초과는 거부된다. |
| PROFILE-03 | L0 | 외부 URL scheme, host, userinfo, 민감 query 검사 | HTTPS 및 public-looking DNS host 문법만 통과하며 DNS 조회는 하지 않는다. javascript/http/IP/loopback/internal/userinfo/token query는 거부되며 네트워크 요청은 0건이다. |
| PROFILE-04 | L0 | 서비스 링크 label, 최대 개수, 중복 URL | label은 1~48자, 링크는 5개 이하이며 같은 목적지 중복을 저장하지 않는다. |
| PROFILE-05 | L0 | 개인·공개 순위 projection, 동점, 음수/0점, 정렬 | 개인 순위는 서버 순위 자격이 유효한 양수 점수 계정 기준이며 ID를 반환하지 않는다. 공개 순위는 opt-in+handle 계정만 포함하고 동점은 1,1,3 방식이다. |
| PROFILE-06 | L1/DB | 대소문자만 다른 handle 동시 등록 | 대소문자 비구분 unique 제약에서 하나만 성공하고 나머지는 409이며 기존 소유권이 바뀌지 않는다. |
| PROFILE-07 | L1/API | 다른 계정 ID를 넣은 프로필 저장, 비공개 프로필 열기 | 소유자는 인증 session에서 정한다. 타인 프로필 변경은 거부되고 비공개/없는 프로필은 동일 404다. |
| PROFILE-08 | L1/API/DB | opt-in/off, 계정 비활성화, 캐시 갱신 | opt-out 뒤 공개 목록·프로필·캐시에서 제거된다. 응답에 email, 내부 ID, hidden 계정 점수/순위가 없다. |
| PROFILE-09 | L3/Web | Rich-text editor 서식, 붙여넣기, 키보드/낭독기 사용, 저장 후 다시 열기 | 서식이 schema v1에 맞게 round-trip된다. 붙여넣은 script/HTML은 실행되지 않고 text 또는 허용된 노드로 변환된다. 컨트롤에 접근 가능한 이름과 오류 안내가 있다. |
| PROFILE-10 | L3/Web | 리더보드 handle 클릭, 프로필 외부 링크 클릭 | handle은 올바른 `/u/{handle}`로 이동한다. 링크는 HTTPS 목적지로 열리고 새 탭에는 `noopener noreferrer`가 있다. 비공개 프로필은 DOM/응답에 없다. |
| PROFILE-11 | L1/API/File | profile 편집·opt-out·신규 GitHub 계정 저장 뒤 재시작, 중복 handle·손상 파일·저장 실패 | rich bio와 링크, 공개 동의, handle 소유권이 복원된다. 손상 파일은 seed로 초기화하지 않고 시작 실패한다. 저장 실패는 5xx이고 기존 상태는 유지된다. |

## 구현 상태

- PROFILE-01~05: `crates/wam-core/tests/profile.rs`, `crates/wam-core/tests/leaderboard.rs`.
- PROFILE-06~08: `crates/wam-server/tests/api.rs`와 `profile_storage.rs`가 로컬 소유권, handle 고유성, opt-out을 검증한다. DB 제약/경쟁 검증은 남아 있다.
- PROFILE-11: `profile_storage.rs` 4개와 `api.rs`의 profile restart/write-failure 테스트 2개. 저장 실패 503, 이전 상태 유지, 손상 파일 비초기화, 신규 계정 복원, 고유 handle 및 파일 권한을 검사한다.
- PROFILE-09~10: 실행 가능한 SvelteKit UI가 있다. Arise에서 handle 변경·공개 해제·새로고침 및 API 재시작 복원과 공개 404/leaderboard 제외를 확인했다. rich bio의 구조화 round-trip은 HTTP restart 테스트로 검증했으며 editor 전체 서식·붙여넣기 여정은 아직 검증하지 않았다.

관련 명세: [`../spec/profile.md`](../spec/profile.md), [`user-flows.md`](user-flows.md)의 FLOW-08, [`feature-acceptance.md`](feature-acceptance.md)의 ACC-PROFILE-01.
