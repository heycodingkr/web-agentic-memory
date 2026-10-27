# PROFILE: 공개 프로필, bio 편집, 개인·공개 순위

상태: 로컬 profile API와 영속화 구현. `wam-core` 검증, opt-in leaderboard, 공개 프로필, rich-text editor, 개인 rank와 본인 점수 내역 화면이 있다. 개발 서버는 handle/bio/service links/공개 동의와 신규 GitHub 계정 profile을 `.wam-data/profiles.json`에 저장한다. 승인 점수 +20, PostgreSQL transaction, 실계정 OAuth smoke는 남아 있다.

## 목표

기여자는 자기 누적 점수와 순위를 로그인 뒤 확인한다. 공개를 선택한 사용자는 리더보드에서 handle로 소개되고, handle을 누르면 bio와 서비스 링크가 있는 프로필을 연다. 공개 프로필은 순위 참여 동의에 묶인다.

## 프로필과 공개 범위

- 프로필은 내부 계정에 귀속한다. 공개 handle은 이메일이나 내부 user ID가 아니라 사용자가 고르는 별칭이다.
- handle은 대소문자를 구분하지 않는 고유값이며 3~24자의 ASCII 영문자·숫자·`_`·`-`만 허용한다. 첫/끝은 영문자 또는 숫자이며 시스템 경로 이름은 예약한다.
- 계정 소유자는 handle, rich-text bio, 최대 5개의 이름 붙은 서비스 링크를 편집할 수 있다. 프로필 미작성·비공개 상태에서도 점수 dashboard를 쓸 수 있다.
- leaderboard opt-in은 handle, 점수 합계, 공개 프로필 페이지를 함께 노출하는 동의다. opt-out이면 공개 목록과 `/u/{handle}` 응답에서 제외하며, API는 프로필 존재 여부를 구분하지 않는 404를 반환한다.
- handle이 리더보드에 표시되면 클릭 가능한 링크로 `/u/{handle}` 프로필을 연다. 다른 사용자에게 편집 기능이나 내부 계정 식별자를 반환하지 않는다.
- opt-out/계정 비활성화가 적용되면 공개 목록과 프로필 캐시에서 즉시 제외한다. 본인 dashboard와 프로필 응답은 `Cache-Control: no-store`를 쓰며, opt-out 뒤 오래된 공개 프로필을 보여주는 캐시는 허용하지 않는다.

## 로컬 저장과 복원

`LocalRepository::open`은 catalog는 seed로 두고 account/profile만 단일 프로세스 파일 저장소에 기록한다. 저장은 검증한 다음 임시 파일·fsync·원자적 rename 순으로 수행하고, 성공 후 메모리 상태를 바꾼다. 저장 실패는 503이며 이전 profile과 공개 설정을 유지한다. 재시작 때도 rich bio, service links, opt-out, 신규 계정과 handle 소유권을 복원한다. 손상 JSON·중복 ID/handle·유효하지 않은 profile·symlink 경로는 시작 실패로 처리하며 seed로 덮어쓰지 않는다. Unix에서 파일은 0600, 새 전용 디렉터리는 0700이다. 다중 프로세스 동시성과 PostgreSQL 제약을 대신하지 않는다. PROFILE-11이 회귀 기준이다.

## 점수와 순위

운영 점수의 원천은 immutable score event 합계이며 클라이언트는 점수나 순위를 제출하지 않는다. 현재 local demo는 seeded account의 예시 기초 점수에 성공 사용 원장 합계를 더해 순위를 보여준다. 승인 event와 production repository가 연결되기 전까지 이 기초 점수는 실제 기여 보상을 뜻하지 않는다.

- 개인 dashboard의 **전체 순위**는 서버 순위 자격이 유효하고 양수 순점수를 가진 계정을 대상으로 계산하며, opt-out 계정도 순위 집합에 포함한다. 이 값과 전체 참여자 수는 본인 인증 API에서만 제공한다.
- 순점수가 0 이하인 계정은 아직 순위가 없다고 표시한다.
- 같은 점수는 같은 경쟁 순위로 표시한다. 예: 1위, 2위, 2위, 4위.
- 공개 leaderboard는 서버 순위 자격이 유효하고 opt-in 및 handle 설정을 마친 양수 점수 계정만 대상으로 별도 순위를 매긴다. dashboard는 전체 순위와 공개 순위를 서로 다른 이름으로 표시한다.
- 공개 API는 opt-in handle, 공개 순위, 점수 합계만 반환한다. 이메일, 내부 ID, 비공개 bio·링크, 숨은 계정의 점수·순위를 반환하지 않는다.

## Rich-text bio와 링크

bio editor 문서 schema v1은 구조화 블록이다. 허용 형식은 문단, H2/H3 제목, 굵게, 기울임, 순서/비순서 목록, 제목이 있는 inline 링크다. 임의 HTML, script, iframe, 이미지 embed, CSS는 지원하지 않는다.

- 서버는 모든 저장 입력을 core validation에 통과시킨다. bio는 최대 1,200자, 20개 블록, inline 링크 8개다. 서비스 링크는 최대 5개, 각 표시 이름은 최대 48자다.
- editor는 문자/블록 수, 미리보기, 저장 오류를 보여주고 키보드와 화면 낭독기로 조작 가능해야 한다. 붙여넣은 HTML은 제한된 서식으로 변환하거나 일반 텍스트로 넣는다. 렌더러는 텍스트를 HTML로 해석하지 않는다.
- bio와 서비스 링크는 HTTPS 및 public-looking DNS host 문법만 허용한다. 등록된 DNS인지 확인하지 않으며, userinfo, IP/loopback/internal 호스트, 자격 증명으로 쓰이는 query key는 거부한다. 서버는 프로필 URL을 fetch하거나 미리보기를 만들지 않는다.
- 링크는 사용자가 누르면 이동한다. 새 탭을 쓸 경우 `target="_blank"`와 `rel="noopener noreferrer"`를 함께 적용하고, 화면에는 사용자가 지정한 이름과 실제 destination host를 함께 알아볼 수 있게 한다.

## HTTP 계약

- `GET /v1/me/profile`: 본인 프로필/공개 설정 조회. 인증 필수.
- `PUT /v1/me/profile`: 본인 handle, versioned bio 문서, 서비스 링크, 공개 설정 갱신. 계정 ID는 body에서 받지 않고 로그인 세션에서 결정한다. handle 중복은 409.
- `GET /v1/profiles/{handle}`: 공개 opt-in 상태의 프로필만 조회. `Cache-Control: no-store`; 비공개/없는 handle은 동일한 404.
- `GET /v1/me/dashboard`: 본인 점수와 전체 순위, 공개 참여 중이면 공개 순위도 반환.
- `GET /v1/leaderboard`: 공개 참여자만 대상으로 페이지 단위 목록 반환. `Cache-Control: no-store`; 각 handle은 프로필 경로를 갖는다.

순위 계산은 score ledger snapshot을 사용한다. handle 변경, opt-out, 계정 비활성화는 profile owner 데이터에만 적용하고, 공개 목록·프로필 projection 무효화를 감사 가능한 단일 변경으로 반영한다.

## 요구사항과 테스트

| 요구사항 | 기준 |
|---|---|
| F-PROFILE-01 | handle 정규화·고유성·예약 경로, 본인 소유 편집 |
| F-PROFILE-02 | 허용 서식만 담는 크기 제한 rich-text bio |
| F-PROFILE-03 | 안전한 HTTPS 링크와 사용자 클릭 탐색 |
| F-PROFILE-04 | 전체 개인 순위와 opt-in 공개 순위의 분리 |
| F-PROFILE-05 | 리더보드 handle에서 공개 프로필로 이동 |
| F-PROFILE-06 | opt-out 시 프로필·순위 노출과 캐시 제거 |
| F-PROFILE-07 | 서버 재시작 후 profile/공개 동의/handle 소유권 복원, 저장 실패 시 성공 응답 금지 |

기능 테스트는 [`../test/profile.md`](../test/profile.md)의 PROFILE-01~11, 사용자 여정은 [`../test/user-flows.md`](../test/user-flows.md)의 FLOW-08이다.
