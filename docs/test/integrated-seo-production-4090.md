# SEO 메모리·검색 최적화 통합 배포

2026-11-02 13:43 KST, 사용자 요청으로 브랜치를 통합하고 웹·API를 4090 운영 서버에 배포했다. 추적 이슈는 #245다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/memories |
| 릴리스 | `20261102T044008Z-main-6db8464` |
| 고정한 배포 소스 | `6db8464ae896b1e3874a0723fa042624f2e5ca3c` |
| 이전 릴리스 | `20261101T233237Z-design-af8bda8` |
| 실행 API | `wam-api.service`, PID 545105 |
| 공개 메모리 | 34 → 37, 기존 34개 memory/version ID 유지 |
| 추가 예시 | Search Console 도메인 인증·sitemap, Bing Webmaster 사이트 등록·CNAME 인증, Cloudflare DNS 검증 레코드 |
| 검색 변경 | BM25 토큰 빈도 사전 계산과 목록 정렬의 제품 분류 중복 계산 제거 |
| DB schema·운영 환경 파일 | 변경 없음 |

공식 작성자·관리자 인증과 메일 outbox·진단 메일·기기 사용 시각·모델 예산 처리를 포함한 기존 운영 코드를 보존했다. 기존 예시를 refresh하거나 운영 비밀 설정을 바꾸지 않았다. 공식 작성자 점수는 새 예시 게시 후 860이다.

## 소스 통합과 보존

공유 작업 트리의 709개 파일 해시와 원래 index를 먼저 로컬 증거 디렉터리에 보존했다. 미커밋 변경은 `2d31e31`에 저장하고 별도 checkout에서 로컬 `main`, `origin/main`(`26a3f9b`), 현재 운영 소스(`09d6eec`), Notion R9 사전등록 보존 브랜치를 병합했다. 디자인과 SEO의 seed·제품 탐색을 모두 유지하도록 충돌을 해결했다.

운영 `source-hashes.json` 704개를 대조해 어느 커밋에도 없는 운영 전용 파일이 0개임을 확인했다. 이전 브라우저 배포의 보존용 snapshot은 후속 main에서 이미 대체된 내용이므로 `archive/browser-download-release-20261101` 태그로 이력을 보존한다. 병합한 배포·사전등록·작업 트리 보존 브랜치는 main 동기화 후 제거한다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 웹 단위 | 65/65 |
| 웹 타입 검사 | 오류 0개, 기존 SetupGuide 경고 2개 |
| production 정적 빌드·Rust fmt | 통과 |
| 고정 Linux builder | workspace·PostgreSQL을 포함한 75개 suite에서 516 pass, release 빌드 통과 |
| 실제 disposable PostgreSQL ignored 검사 | application 17개·diagnostics 2개·email auth 2개 통과 |
| 임시 자원 정리 Python 회귀 | 15/15 |
| production 웹 빌드의 Chrome 탐색 회귀 | 9개 case 확인: 최초 8개 통과, 테스트 route 수정 후 나머지 1개 통과 |
| 운영 DB 백업 복원 | 네트워크 없는 일회용 PostgreSQL에 복원, 테이블 21개 |
| 공개 recall | 새 SEO 3개와 기존 Stitch 메모리 확인 |
| 공개 MCP | initialize·도구 10개 확인 |
| 인증 경계 | 비인가 관리자 session·목록 401, 기존 review 403, 인증 설정 유지 |
| 공개 HTTPS | apex·api·admin의 대표 경로 8개 200 |
| 운영 Arise UI | 37개 목록, SEO 각 URL 검색 결과 1개, 개인 URL sentinel 네트워크 비전송, 가로 overflow 없음, 실제 캡처 이미지 검사 |

SEO 회귀 시험이 추출된 BM25 모듈을 읽지 못하는 실패를 먼저 재현하고 공용 TypeScript loader로 수정했다(`d655a26`). 클라이언트 준비 지연 시험은 광범위한 `route.continue()`가 기존 API mock을 건너뛰어 502를 받았다. 이 실패를 재현한 뒤 `route.fallback()`으로 수정하여 통과했다(`4e30262`). 후자는 배포 소스 고정 뒤의 테스트 파일 한 줄 변경이며 제품 소스와 빌드에는 변경이 없다.

## 증거와 복구

로그·소스/웹 archive·해시·운영 API/MCP/UI 결과는 gitignored `target/deploy-integrated-20261102T043659Z/`와 서버의 `builds/20261102T044008Z-main-6db8464/`에 보관한다. 백업은 서버의 root 전용 `/var/backups/web-agentic-memory/20261102T044008Z-main-6db8464/production.dump`다. 이전 release도 보존했다.

활성화는 기존 두 배포 잠금을 모두 잡고 이전 릴리스와 source/archive/binary 해시를 확인한 뒤 진행했다. readiness·기존 메모리 버전·새 메모리·인증 설정 검사 실패 시 이전 release로 자동 복귀하도록 실행했다. 실제 전환과 후속 공개 검사는 성공했다. 기여 worker의 실계정 전체 여정(#159)이나 검색 엔진의 색인 완료는 이 배포의 수용 범위가 아니다.
