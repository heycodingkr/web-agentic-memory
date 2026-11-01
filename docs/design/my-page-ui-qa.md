# My Page UI와 UX 검증

2026-11-01 로컬 My Page를 더미 API로 실제 렌더링하고, ImageGen 디자인 참고 시안과 데스크톱·모바일 캡처를 비교해 구현했다. 로그인 후 홈은 `/dashboard`다. 기여 상태·총점·순위를 먼저 보여주고 본인 API 사용량을 하단에 접어 두었다. 기능 계약은 [기능 명세](../spec/my-page.md), 전체 수용 범위는 [테스트 명세](../test/my-page.md)를 따른다.

## 미리보기와 데이터

실행 중인 주소는 [My Page 로컬 미리보기](http://127.0.0.1:5199/dashboard)다. 상단 Sample data 영역에서 정상 목록·기여 없는 계정·분석 장애·로그아웃 상태를 바꿀 수 있다. 로그아웃했으면 Reset으로 샘플 계정을 복원한다.

이 서버는 실제 이메일 인증이나 운영 계정 데이터 없이 화면만 검사한다. 더미 데이터에는 제출 9건, 본인 작성 버전 3개, 현재 게시 메모리 3개, pending 4건, 조치 필요 1건, 총점 246점과 전체 순위 18위를 넣었다. 목록의 게시 2건은 제출 목록 중 게시 결과가 있는 건수이며 요약의 고유 게시 메모리 3개와 범위가 다르다. 최근 원장 +40점과 이전 206점이 총점 246점에 맞는다.

`web/dev/my-page-data.ts`와 `my-page-preview.ts`는 명시적으로 켠 Vite dev 서버에서만 사용한다. 실제 UI는 기존 API를 호출하며 fixture를 가져오지 않는다. production build의 샘플 ID·preview header 부재도 검사했다.

```sh
WAM_UI_PREVIEW=1 VITE_WAM_API_URL=/__preview pnpm --dir web exec vite dev --host 127.0.0.1 --port 5199 --strictPort
```

현재 서버는 MeshCode detached shell로 시작해 턴 종료 후에도 유지된다. 2026-11-01 확인한 listener PID는 `55562`, 로그는 `/var/folders/hn/4d_172w53_58g8kg255mlw_w0000gn/T/meshcode-detached-98074-1793513902588.log`다. 이미 실행 중이면 재사용한다. 이전 화면 비교용 5200 서버는 캡처 후 종료했다.

## 실제 화면 비교

변경 전은 같은 샘플 API와 저장소 HEAD의 기존 dashboard/layout으로 별도 로컬 렌더링했다. 변경 후는 현재 소스다. PC는 1440×1080, 모바일은 390×844다. 브라우저 스크린샷을 이미지 검사 도구로 직접 확인했다.

| 화면 | 변경 전 | 변경 후 |
|---|---|---|
| PC 첫 화면 | [이전 PC](assets/before-desktop.png) | [현재 PC](assets/after-desktop.png) |
| 모바일 첫 화면 | [이전 모바일](assets/before-mobile.png) | [현재 모바일](assets/after-mobile.png) |
| 모바일 기여 목록 | 기존에는 페이지 아래에 숨음 | [모바일 목록](assets/after-mobile-contributions.png) |
| 기여 상세 | 별도 상세 화면 없음 | [재시도 상세](assets/after-detail.png) |
| 다른 계정의 호출 통계 | 긴 본인 API quota 영역 다음 | [Memory impact](assets/after-impact.png) |
| 상태별 화면 | 통합 실패 시 정상 정보도 제거됨 | [빈 계정](assets/after-empty.png), [분석 장애](assets/after-error.png), [로그아웃](assets/after-signed-out.png) |

PC 첫 화면에서 기여 목록과 상태가 보이고, 모바일 요약은 2개/3개 카드로 압축했다. 모바일 기여 행은 제목·서비스·상태·상태 설명을 쌓아 표시한다. 탭은 한 줄 가로 스크롤과 방향키 이동을 지원한다. 글자 대비를 높였고 상태는 텍스트·아이콘·색을 함께 사용한다.

## ImageGen 참고 시안

[built-in ImageGen 참고 이미지](assets/my-page-concept-v1.png)를 생성했다. 입력은 기존 PC 화면이며, [실제 사용 프롬프트](my-page-imagegen-prompt.txt)를 보존했다. 생성 도구는 built-in ImageGen이며 CLI fallback은 사용하지 않았다.

시안의 크림색·짙은 녹색, 간결한 헤더, 5개 요약 카드, 목록과 안내 사이드바, 하단 성과 영역을 실제 Svelte 컴포넌트로 구현했다. 생성 이미지를 앱 화면 배경으로 붙이지 않았다. 시안에 나타난 외부 Calendar 계정 접근 동의 문구는 채택하지 않고 워크플로 공유 동의로 수정했다. 기존 메모리 보강을 신규 게시 점수로 표시하지 않았으며 샘플 게시 수는 실제 fixture에 맞춰 3개로 조정했다.

## 동작과 오류 처리

- 기여 검색·All/Pending/Published/Supported/Needs action 필터, 6행 페이지 이동, owner 상세 조회를 제공한다.
- My memories·Memory impact·Point history 탭, 7/30/90일과 day/week, 차트의 표 대안, 원장 더 보기를 제공한다.
- 다른 계정의 recall·상세 조회·보고 성공과 익명 요청을 구분한다. 익명 요청은 고유 인원이나 확실한 타인의 사용이 아니다.
- 기존 `approved`만 있는 제출은 Result unconfirmed로 표시한다. 최근 처리 시각이 없으면 제출 시각임을 명시한다.
- 요약·기여·통계·원장·usage가 각각 로딩/오류를 처리한다. analytics 503이나 usage 429/503에서도 정상 데이터는 유지한다.
- 현재 인증의 401은 모든 개인 정보와 다이얼로그를 지운다. 교체된 요청의 늦은 401은 무시한다. 429는 Retry-After 동안 재시도 버튼과 요청을 제한한다.
- 화면이 보이고 pending이 있을 때만 30초 간격으로 기여·요약을 갱신한다.

부분 장애는 먼저 analytics 503을 주는 테스트에서 점수 카드가 사라지는 실패를 재현했다. 원인은 하나의 Promise.all 실패가 전체 초기 상태를 버리는 구조였다. 패널별 요청·오류·last-known 값과 generation 검사를 유지하도록 수정한 후 같은 테스트가 통과했다. legacy 접수 시각을 Updated로 오해할 수 있는 문제도 선행 실패를 확인하고 Submitted 표시로 수정했다. 선행 로그는 `/tmp/wam-my-page-red.log`, `/tmp/wam-my-page-timestamp-red.log`다.

## 실행 결과

| 검사 | 결과 | 범위 |
|---|---|---|
| My Page 브라우저 | 12건 통과 | Chrome PC/Pixel 7, 정상/빈 계정/비로그인/legacy/503, 필터·검색·페이지·상세·탭·기간·원장 |
| dashboard 회귀 | 12건 통과 | Chrome PC/Pixel 7, 인증 만료·이전 요청 401·429/503·접근성 |
| 이메일 인증 회귀 | 12건 통과 | mock 코드 검증·My Page 이동·CSRF 로그아웃·MCP/CLI 내부 복귀·외부 redirect 방지 |
| 자동 접근성 | 위 브라우저 검사에 포함 | 네 탭 WCAG A/AA axe 위반 0, 가로 넘침 없음, 키보드 탭 전환 |
| 프런트엔드 unit | 39건 통과 | 현재 저장소의 기존 단위/계약 검사 |
| Svelte/TypeScript | 오류 0·경고 0 | `pnpm --dir web check` |
| production build | 통과 | `pnpm --dir web build`, adapter-static output |

```sh
pnpm --dir web exec playwright test --config playwright.my-page.config.ts
pnpm --dir web exec playwright test --config playwright.my-page-regression.config.ts
pnpm --dir web exec playwright test --config playwright.email-auth.config.ts
pnpm --dir web test
pnpm --dir web check
pnpm --dir web build
```

My Page 검사는 5199의 opt-in 샘플 서버를, 회귀 두 묶음은 별도로 실행 중인 일반 dev 서버 5197을 재사용했다. 테스트 설정에서 개발 서버를 새로 띄우거나 종료하지 않는다. 결과 로그는 `/tmp/wam-my-page-tests.log`, `/tmp/wam-my-page-regression.log`, `/tmp/wam-email-auth-regression.log`에 있다.

## 남은 서버 연동과 운영 검증

실제 owner contribution 응답에 처리 단계·안전한 사유·결과 memory/version 연결이 아직 없다. UI의 optional progress/reflection 표시를 실제 서버 결과로 채우는 작업은 #162다. 서버 요약 집계가 없으면 Published memories 대신 Authored versions를 표시하고, 완전한 제출 목록일 때만 pending/action 합계를 계산한다.

전용 owner 메모리 목록·현재 공개/과거 버전 상태·서버 cursor/filter·worker 상태 전이·A/B 계정의 실제 호출/보고와 점수 대조는 전체 수용 전 확인해야 한다. 더미 memory 링크를 클릭한 결과를 실제 게시 확인 증거로 사용하지 않는다.

후속 직접 배포에서 4090 서버에 production UI/API를 올리고 실메일 가입·기존 계정 재로그인·세션 유지·로그아웃을 확인했다. 현재 DNS는 Tunnel 없이 서버로 직접 연결한다. 실제 운영 캡처와 범위는 [4090 직접 배포 검증](../test/deployment-4090.md)을 따른다. 위 더미 데이터의 상세 진행 결과는 여전히 #162의 실제 서버 read model과 구분한다. 실제 Safari·보조기기 수용도 별도다.
