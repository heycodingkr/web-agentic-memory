# 디자인 메모리·탐색 UI 운영 배포

2026-11-02 KST. 사용자가 웹 UI와 메모리 다섯 개의 배포를 요청했다. Arise 앱 배포는 범위에서 제외했다.

## 배포 범위

| 항목 | 기록 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/memories |
| 릴리스 | `20261101T233237Z-design-af8bda8` |
| 이전 릴리스 | `20261101T140647Z-telemetry` |
| 승인한 기능 커밋 | `af8bda8` |
| 운영 기준 소스 | 웹 `301c5a1` 및 기존 실행 API의 검증된 파일 222개 |
| 고정한 릴리스 소스 | `09d6eec10c0a9e4efce45ed3d87af4dc02c3debb` (`deploy/design-memories-20261102`) |
| 실행 API | `wam-api.service`, PID 3075110 |
| 메모리 수 | 29 → 34 |
| 새 예시 | Figma 반응형 프레임·PNG 내보내기, Stitch 웹·모바일 앱·반복 수정 |
| DB schema·운영 환경 파일 | 변경 없음 |

다른 세션의 미완료 변경을 배포하지 않았다. 현재 실행 API의 원본 해시를 대조해 진단 기능을 보존했고, 운영 웹 기준 파일 159개를 소스와 대조했다. 기존 작성자 표시와 공식 계정 게시 계약도 유지했다. 새 정적 파일은 이전 public 자산 위에 적용하여 기존 자산을 보존했다.

## 빌드·복구 검증

- 웹: 고정된 소스의 Node 시험 59개, typecheck 오류 0개 (기존 SetupGuide 경고 2개), production origin static build 통과.
- Linux: 기존 Rust builder digest를 사용했다. server/client/CLI/MCP 회귀와 disposable PostgreSQL 검사를 포함한 51개 suite 실행에서 355개 pass를 기록했다. 일반 실행에서 ignored인 PostgreSQL 시험은 별도 명령으로 실행했다.
- 운영 PostgreSQL을 custom dump로 백업했다. 네트워크를 차단한 일회용 PostgreSQL 컨테이너에 복원하여 21개 테이블을 확인했다. dump는 서버의 root 전용 백업 경로에 보관하며 공개 파일에 포함하지 않았다.
- 이전 릴리스가 그대로 활성 상태인지 재확인한 뒤 current 링크를 전환하고 API를 재시작했다. readiness·catalog·설정 검증 실패 시 이전 릴리스로 되돌리는 절차를 사용했다.
- 실제 전환 후 기존 29개 memory/version ID가 모두 유지됐고, 운영 환경 파일 해시와 공개 인증 설정도 같음을 확인했다.

Linux 시험 중 기존 일일 예산 시험이 UTC 23시 이후 실패했다 (#230). `start + 3600`이 다음 날인데 당일 Idle을 기대한 fixture 오류다. 기준을 미래 UTC 자정으로 고정하여 전체 시험을 다시 통과했다. 제품 예산 로직은 변경하지 않았다. 릴리스 소스의 수정은 `09d6eec`, main의 같은 수정은 `4034580`이다.

## 공개 주소 검증

- catalog 34개 및 새 예시 5개를 확인했다. 예시별 public recall에서 각각 해당 ID가 검색됐으며 다른 제품의 힌트가 섞이지 않았다. 위장 Stitch 도메인은 400으로 거절했다.
- 공개 remote MCP initialize와 10개 도구 목록이 정상 응답했다.
- 실제 운영 API를 사용한 1440/390/320px Chrome 시험이 통과했다. 랜딩 Stitch 바로가기, 같은 페이지 fragment 변경, Figma 2개·Stitch 3개 URL 검색, Design & media 11개 분류를 확인했다. 개인 URL sentinel의 네트워크 비전송, page error 없음, 가로 overflow 없음도 검증했다.
- Arise MCP에서도 실제 운영 메모리 34개, 분류 버튼, Figma 2개·Stitch 3개 검색 결과, 한국어 랜딩의 디자인 바로가기와 hydration 후 검색 활성화를 확인했다. 실제 캡처 이미지를 검사했다.
- 운영 1440/390px 결과 PNG도 직접 검사했다.

Arise의 한 탭에서는 랜딩 링크 이동 후 native frames가 비고 renderer 읽기가 timeout 됐다. 같은 공개 URL을 새 소유 탭에서 열자 frame 1개와 Stitch 결과 3개가 다시 읽혔다. Chrome의 실제 운영 링크 이동은 세 너비 모두 통과했다. 이 MCP 대상 상태 관측은 agent-browser #224에 추가했으며 웹 탐색 실패나 Arise 수정 완료로 표시하지 않는다.

증거·해시·Linux 로그·운영 API/MCP/UI 결과·PNG는 gitignored `target/deploy-design-20261102/`와 서버의 해당 build/release 경로에 보관한다. 서비스 로그는 `journalctl -u wam-api`에서 확인한다. 디자인 워크플로 관찰 기록은 [figma-stitch-workflows.ko.md](../design/figma-stitch-workflows.ko.md)에 있다.
