# Figma·Stitch 디자인 워크플로우와 검증 기록

2026-11-02. 메모리 페이지의 디자인 분류에 넣을 예시다. 실제 계정/팀 이름, 파일 키, 프로젝트 ID, iframe RPC query와 인증 정보는 공개 메모리에 넣지 않는다.

## 작업 계획

| 워크플로우 | 결과물 | 확인할 종료 조건 |
| --- | --- | --- |
| Figma 반응형 웹 디자인 | desktop/mobile 프레임 | 두 프레임을 실제 canvas에서 확인하고 저장 후 다시 연다 |
| Figma 디자인 수정·handoff | 수정된 프레임/PNG | 요청한 수정 및 실제 다운로드를 각각 확인한다 |
| Stitch 웹 디자인 | 웹/모바일 레이아웃 | Web 선택, 정확한 입력, 생성된 화면과 프로젝트 재진입 |
| Stitch 모바일 앱 디자인 | 앱 홈/탐색 화면 | App 선택, 화면 생성, 저장 후 재진입 |
| Stitch 디자인 반복 수정 | 수정된 화면 | 변경 전·후 실제 내용 비교; AI 응답만으로 완료 판정하지 않는다 |

## 재실행에 제공할 관찰 기반 힌트

- Figma Recents의 회전하는 예시 placeholder에서 snapshot ref가 연속으로 stale이 됐다. 현재 live label/selector를 다시 찾고, New Design file → Agents → Prompt for AI agent 경로를 사용할 수 있다.
- 첫 생성 파일의 편집 탭에서 native frames가 비고 renderer 응답이 멈춘 사례가 있었다. 같은 **자신이 생성한 파일**을 새 소유 탭에 열고 선택하면 편집 UI가 읽혔다. agent-browser #224로 추적한다.
- Agents 입력은 rich editor다. 새 작업 전 기존 텍스트를 확인하고, 필요할 때 현재 편집기에 macOS Cmd+A / Windows·Linux Ctrl+A, Backspace를 적용한다. 비어 있음을 확인한 다음 입력한다. type 오류 뒤에는 부분 입력이 남을 수 있으므로 먼저 내용을 다시 확인한다.
- Figma의 Send/Stop 및 Processing 표시를 실제로 확인한다. 완료 응답 뒤에도 File → Layers와 canvas를 확인하고 저장된 파일을 다시 연다.
- Stitch는 top document의 snapshot만으로 편집기를 읽지 못한다. browser_frames에서 현재 앱 프레임을 찾아 매 호출에 현재 native frame token을 사용한다. token/RPC URL을 기억에 고정하지 않는다.
- Stitch의 프로젝트 목록이 길면 snapshot에 main composer가 나오지 않을 수 있다. 현재 프레임에서 visible role=textbox/contenteditable과 Generate designs를 확인한다.
- 선택한 탭과 browser pane에 포커스를 준 다음 iframe editor를 포커스한다. 기존 초안을 비우고 입력 문자열 일치를 확인한다. Generate designs 클릭은 완료 증거가 아니다.
- AI가 제품 통계·성공률·실행 횟수·지원 기능을 만들어 넣으면 실제 제품 정보에 맞게 수정한다. WAM은 웹사이트 작업 지침을 공유하는 메모리다. 생성된 디자인을 곧바로 실제 기능이나 접근성 검증 결과로 취급하지 않는다.

## URL 탐색 회귀 계약

같은 /memories 페이지에서 q fragment가 바뀌면 새 검색어와 기본 필터를 적용해야 한다. 새 URL을 서버로 전송하지 않는다. 일반 hash 변경과 SvelteKit client navigation을 모두 처리하고 구독을 정리한다. 새 디자인 바로가기 E2E가 수정 전 Stitch 검색이 남는 실패를 재현했다.

## A/B 기록 기준

Figma A와 B는 같은 브리프를 새 파일에 적용한다. A는 첫 탐색, B는 위 관찰 힌트를 제공한 순차 재실행이다. 페이지/파일은 분리하지만 같은 에이전트 문맥의 순서 효과가 있으므로 독립적인 성능 실험이 아니다. 생성·저장·재열기 성공 여부를 기록하고 속도 개선율·인과 효과는 주장하지 않는다. 외부 디자인 AI의 생성 대기시간도 WAM의 속도 효과로 계산하지 않는다.

## 실행 결과 (2026-11-02 KST)

서비스 조작은 Arise MCP로 수행했다. Computer Use나 별도 에이전트는 사용하지 않았다. Arise에 남아 있던 Google 계정 선택으로 Figma OAuth를 마쳤으며, Chrome 원본의 쿠키를 가져온 결과는 아니다.

| 실행 | 실제 확인한 결과 | 범위·제한 |
| --- | --- | --- |
| Figma A — 첫 탐색 | 반응형 디자인 생성, Desktop/Mobile 레이어와 canvas 검사, 파일 이름 지정, 새 탭 재열기 후 두 프레임 유지 | 새 파일의 첫 탭이 응답하지 않아 같은 자신 소유 파일을 새 탭에 열었다 (#224) |
| Figma B — 관찰 힌트 재실행 | A와 정확히 같은 브리프를 새 파일에 입력해 확인 후 전송. 두 프레임 생성·canvas 검사·이름 지정·재열기 확인 | 동일 에이전트 문맥의 순차 실행. 완료 후 계정의 일일 AI 한도 소진, 추가 유료 작업 없음 |
| Figma PNG handoff | A의 Desktop 프레임을 PNG로 내보냄. browser_downloads의 complete/file_exists 및 182,572-byte 파일 확인, 로컬 이미지를 직접 검사 | 디자인에 생성된 표본 통계·예상 시간은 실제 제품 수치가 아님 |
| Stitch Web | Web 디자인과 반응형 화면 생성, 후속 제품 문구 수정, 저장된 프로젝트를 새 탭에서 다시 열어 canvas 확인 | 첫 초안에 이전 입력이 붙었으므로 시간 비교 기준에서 제외. overview 검사는 세부 카피·접근성 전체 검수가 아님 |
| Stitch App·수정 | 빈 편집기에 정확한 모바일 브리프를 확인하고 App 생성. 후속 수정 뒤 재열기하여 Read Memory·Copy Steps와 세 예시 카드가 있는 실제 모바일 화면 확인 | 생성 UI는 프로토타입이며 실행 기능·제품 지원 증거가 아님. 부분 입력/포커스 오류는 #227에서 추적 |

Figma A/B는 기능 재실행 비교다. 독립된 모델·반복 표본이 없으므로 속도 향상률, 유의성 또는 인과 효과를 기록하지 않는다. Stitch는 생성·수정·저장 동작 검증이며 유효한 A/B 성능 실험으로 표시하지 않는다.

## 공개 예시와 탐색 UI

`crates/wam-server/seed/design_memories.json`에 아래 다섯 예시를 추가하고 기존 예시 loader에 연결했다.

- `web:example-figma-agent-responsive`: 반응형 웹·모바일 프레임 생성
- `web:example-figma-frame-png`: 선택 프레임 PNG 내보내기·다운로드 확인
- `web:example-stitch-web-layout`: 반응형 웹 랜딩 생성
- `web:example-stitch-mobile-app`: 모바일 앱 화면 생성
- `web:example-stitch-refine-layout`: 생성된 화면의 제품 문구·기능 표현 수정

메모리는 관찰한 macOS 실행 날짜·조건을 명시한 Example이다. Windows/Linux의 선택 전체 단축키도 안내하되 해당 OS에서 서비스 자동화까지 실행한 것으로 표시하지 않는다. 문서·디자인·계획/데이터·커뮤니케이션·리서치 등의 대분류와 제품 필터는 URL 검색과 교집합으로 동작한다. 랜딩에는 Figma·Stitch 바로가기와 디자인 작업 미리보기를 추가했다. 검색 URL은 fragment로 넘기고 서버에 전송하지 않는다.

## 검증과 적용 상태

- 서버 예시 validator/loader 8개와 productivity/실제 recall endpoint 7개 통과. Figma/Stitch URL의 예시 노출과 위장 도메인 거절을 확인했다.
- 커밋 범위만 분리한 소스에서 전체 웹 Node 시험 59개 통과 (새 디자인·카테고리 6개 포함). 공유 작업본의 검색 리팩터링·작성자 표시는 포함하지 않았다. 공개 seed에 계정 이메일·파일 키·프로젝트 ID·native frame token·인증 query·속도 개선 수치를 넣지 않았다.
- Discovery Playwright 9개 통과: 1440/390/320px URL 검색, hydration 전 제출 방지, 제품/카테고리 교집합, Figma/Stitch 바로가기, hash-only 검색 변경, 빈 상태·재시도, 개인 URL의 네트워크 비전송. 해당 화면의 axe WCAG A/AA 자동 점검과 가로 overflow 검사를 포함한다.
- `pnpm check` 오류 0개 (기존 SetupGuide 경고 2개), `pnpm build` 통과. 최종 desktop/mobile 결과 이미지를 직접 검사했다.
- #228은 기존 onMount에서만 읽던 q fragment를 afterNavigate/hashchange에 동기화하고 구독을 정리해 RED→GREEN을 확인했다.
- 커밋 전 Figma/Canva 아이콘 도메인 정규식의 과도한 escape를 발견했다. 정확한 host와 위장 host 시험의 RED를 확인하고 수정해 GREEN을 기록했다. 새 Node 시험은 다른 세션의 미커밋 helper에 의존하지 않는다.

2026-11-02 KST, 사용자 배포 요청에 따라 웹 UI와 다섯 seed를 운영에 배포했다. 릴리스는 `20261101T233237Z-design-af8bda8`이며 공개 메모리는 29개에서 34개로 늘었다. 기존 29개 버전·작성자 표시·진단 기능·로그인 설정을 보존했고, 새 메모리 5개 모두 공개 recall을 확인했다. 상세 증거는 [운영 배포 기록](../test/design-memories-production-4090.md)에 둔다. Arise 앱 배포는 이 작업에 포함하지 않는다. AI 디자인의 임의 통계·추정 시간·지원 기능을 운영 제품 정보로 사용하지 않는다.

재현 증거는 gitignored `target/e2e-logs/design-workflows/`의 summary와 로그에 둔다. 실제 파일/프로젝트 링크는 같은 폴더의 private artifact 목록에만 보관하고 공개 문서·seed에 넣지 않는다.
