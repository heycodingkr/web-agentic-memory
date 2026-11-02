# URL 기반 공유 메모리 소개·히어로 배포

2026-11-02 14:07 KST, 한·영 README와 랜딩 소개를 정리하고 4090 운영 서버에 배포했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/ · https://webagenticmemory.com/ko |
| 릴리스 | `20261102T050515Z-main-6c32d42` |
| 배포 소스 | `6c32d42016bdd9a5967dcf4e42f1589048b2f0fb` |
| 이전 릴리스 | `20261102T044008Z-main-6db8464` |
| 한국어 히어로 | 작업 노하우를 위한 최고의 인덱스, URL. |
| 영어 히어로 | Shared know-how, best indexed by URL. |
| 서비스 정의 | 웹 작업을 위한 URL 기반 공유 메모리. 여러 사용자와 에이전트가 기여한 노하우를 검증해 공유하고, LLM 에이전트가 재사용해 더 효율적으로 일하도록 돕는다 |
| 공개 메모리·공식 점수 | 37개·860점 유지. 기존 memory/version ID 전부 유지 |
| DB schema·운영 환경 설정 | 변경 없음. 공개 인증 설정의 전후 응답 일치 |

제품 변경은 README와 한·영 랜딩 카피이며, 검색·소셜 미리보기 설명에도 URL 기반 공유 메모리라는 정의를 반영했다. 고정된 커밋의 깨끗한 checkout에서 production origin으로 정적 웹을 만들고, 기존 릴리스 절차에 따라 Linux API도 빌드·검증했다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 운영 소스 보존 | 운영 전용 미커밋 파일 0개 |
| 웹 단위 검사 | 65/65 |
| 웹 타입 검사 | 오류 0개, 기존 SetupGuide 경고 2개 |
| production 정적 빌드 | 통과. 실제 한·영 H1, 서비스 origin, MCP·WAM CLI 문구 확인 |
| 한·영 랜딩 Chrome 검사 | 1440·390·320px, 6/6 |
| 고정 Linux builder | 58개 suite, 432 pass, 0 fail. 일회용 PostgreSQL application·email auth 검사와 release 빌드 포함 |
| 업로드 무결성 | 소스·웹 archive와 manifest·활성화 스크립트의 SHA-256 전부 일치 |
| 공개 artifact 비밀값 검사 | 운영에 설정된 서버 비밀값 3개가 정적 파일에 없음 |
| 공개 HTTPS | 한·영 H1이 최종 카피와 일치. API health 200 |
| 운영 Chrome 검사 | 영어 1440px·한국어 320px, 2/2. 이미지 검사로 최종 줄바꿈 확인 |
| 운영 보존·경계 검사 | 37개 메모리 버전·인증 설정 유지, 관리자 session·목록 401과 기존 review 403 |

Chrome 검사는 비로그인 UI를 위해 인증·텔레메트리 응답을 모의 처리하며 미디어·FAQ·가이드 이동을 확인했다. 실제 운영 API의 인증 설정과 비인가 경계는 별도 HTTP 검사로 확인했다.

## 증거와 복구

소스·웹 archive, 해시, 빌드·공개 검사 결과와 운영 캡처는 gitignored `target/deploy-url-memory-production-20261102/` 및 서버 `builds/20261102T050515Z-main-6c32d42/`에 보관한다. 운영 DB 백업은 root 전용 `/var/backups/web-agentic-memory/20261102T050515Z-main-6c32d42/production-before.dump`다.

활성화는 두 배포 잠금(`.deploy.lock`, `deployment.lock`)을 잡고 예상한 이전 릴리스와 artifact 해시를 확인한 뒤 실행했다. 기존 정적 immutable asset과 이전 릴리스를 보존했고, readiness·인증 경계·공식 작성자·정적 페이지 검사 실패 시 이전 릴리스로 자동 복귀하는 절차를 사용했다. 실제 활성화와 후속 공개 검사는 모두 성공했다.

복구가 필요하면 [운영 릴리스 절차](../../deploy/4090/release/README.md)에 따라 이전 릴리스로 `current`를 전환하고 API를 재시작한다.
