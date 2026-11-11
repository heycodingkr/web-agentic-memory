# MCP · CLI 개선 4090 배포

2026-11-04, issues #297–302. 조회 입력의 기기 내 개인정보 경계, stdio 응답성, MCP 탐색·출력 계약, CLI 자동화와 기여 진행 표시를 개선해 배포했다.

| 항목 | 결과 |
| --- | --- |
| 소스 커밋 | `d86d526ddf329351c2d8ce38d5539c302459e803` (origin/main) |
| 운영 릴리스 | `20261104T061806Z-main-d86d526` |
| 이전 릴리스 | `20261104T055411Z-main-fd3b35c` |
| API SHA-256 | `84f96425b878a6e8f6b8a011abb20a10aa298c1e41beaead0d3b428782ad7b09` |
| DB 백업 | `/var/backups/web-agentic-memory/20261104T061806Z-main-d86d526/production-before.dump` |
| 운영 보존 검사 | production-only source 0개, 운영 source commit이 배포 커밋의 조상임을 확인 |
| 활성화 결과 | 공유 잠금·DB 백업·API/admin/review·공식 작성자·정적 페이지 검사 통과, rollback 없음 |
| 공개 상태 | HTTPS health 정상, 공식 메모리 38개·공식 점수 880 유지 |

## 변경과 재현

| 이슈 | 원인과 변경 | 재현 입력 · 기대 결과 |
| --- | --- | --- |
| #297 | 공용 client가 URL만 정제하고 원래 keyword를 전송하던 경로에 공개 어휘 조회·core 정규화/중복 제거를 추가했다. cache key와 HTTP body 전에 적용한다. | `["  ＢＬＯＧ  ","private-review-sentinel"]`, 공개 어휘 `["blog"]` → CLI/stdio 실제 HTTP에는 `["blog"]`만 전송. 어휘 장애 때 raw fallback 없음. |
| #298 | 순차 stdin 처리와 무시하던 취소를 bounded async 수신/실행으로 교체하고 진단 참조를 요청별로 분리했다. | 1.2초 지연 recall 중 취소+ping → ping 선응답, 취소된 요청의 성공 응답 없음. 64 KiB 초과 frame 거부 후 다음 ping 정상. |
| #299 | JSON 모드 실패도 stderr 문자열만 내보내던 경로와 JSON 옵션이 없던 cache 명령을 통일했다. | 429/Retry-After 7 → exit 6, stdout의 단일 schema-versioned error/code/retry 문서. cache status/clear JSON도 서버·credential 조회 없이 정상. |
| #300 | progress/reflection을 JSON에만 보존하던 기본 기여 목록에 진행 정보와 정확한 반영 버전을 추가했다. | 별개 queued/published fixture → 대기 이유·다음 시도와 `memory @ version (availability)` 표시. 과거 응답의 필드 생략 허용. |
| #301 | 초기 지침과 어휘 탐색 경로를 제공했다. status의 선택적 vocabulary를 사용해 도구 수는 유지했다. | initialize 지침에 untrusted/skip/pinning/used-version 규칙, status에 공개 어휘. 실제 공개 catalog는 도구 10개, 지침 포함 3,196 UTF-8 bytes. |
| #302 | 성공 DTO를 legacy text JSON과 동일한 structuredContent로 제공하고 선택적 schema/behavior hints를 추가했다. | 최신 버전은 text/structured 동등. status는 readOnlyHint, withdraw는 destructiveHint/outputSchema. 2024-11-05/2025-03-26 및 HTTP 헤더 생략의 legacy 출력 유지. |

어휘는 기존 no-store 정책을 유지한다. 키워드나 task가 있는 client 요청은 cache 조회 전 어휘 endpoint에 연결할 수 있어야 하며 실패하면 안전한 오류로 끝난다. URL-only 익명 fresh/stale cache는 유지한다. 원격 MCP caller가 자신의 기기에서 전송 전 정제해야 하는 경계는 그대로다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 수정 전 focused red | 공개 어휘 전송 경계 3건, 초기 지침/구조화 출력, CLI JSON/cache, 기여 진행, stdio 취소·frame·legacy, HTTP 기본 버전 회귀 확인 |
| 독립 macOS target의 Rust workspace | 75개 요약, 520 통과·실패 0·외부 환경용 ignored 32 |
| 정적 검사와 native build | workspace/all-targets Clippy warnings-as-errors, rustfmt, diff check 및 CLI/MCP build 통과 |
| 실제 CLI/stdio black-box | 같은 합성 입력 7개 모두 개선 기대치 통과 |
| web | unit 80 통과, production build 성공, check 오류 0·기존 SetupGuide 경고 2 |
| task-owned Linux server target | 59개 요약, 441 통과·실패 0·default ignored 31; 별도 disposable PostgreSQL 검사와 release build 통과 |
| task-owned Linux CLI/MCP | 18개 요약, 99 통과·실패 0·secure-store ignored 1; 두 release 실행 파일 build 통과 |
| 공개 HTTPS · 실제 native 연결 | health, 지침/catalog, status 어휘/구조화, 명시/default legacy, vocab no-store, 익명 recall, 갱신한 로컬 CLI/stdio→운영의 8개 검사 통과 |

#296의 오류 추적 커밋을 먼저 통합했다. server trace 보존, JSON/SSE 진단 범주와 응답 extension, CLI/stdio의 안전한 사용자 참조를 보존하고 새 JSON 오류의 trace도 같은 진단 event와 대조했다. 원시 입력·키워드·토큰의 진단 수집은 추가하지 않았다.

최종 검증은 다른 worktree와 공유하지 않는 Cargo target에서 수행했다. Linux도 기존 공용 artifact symlink 대신 새 target을 만들어 빌드했다. 도구 catalog의 byte 검사는 고정 UTF-8/4 proxy이며 실제 host tokenizer 측정은 아니다. 공개 운영 검사에는 정상 read만 사용했다.

## 산출물과 복구

서버 `/srv/web-agentic-memory/builds/20261104T061806Z-main-d86d526/`에 고정 source/web archive, source manifest, 검증·활성화 script, server 및 Linux CLI/MCP 바이너리와 해시가 있다. 로컬 `/private/tmp/wam-mcp-cli-review-20261104-p37580/`에는 사전 명세, 전후 합성 관찰, 공개 smoke 결과와 native 설치 해시가 있다.

공유 checkout을 fast-forward하고 프로젝트 `target/debug/wam`, `target/debug/wam-mcp`를 검증된 바이너리로 원자 교체했다. 기존 README/데모 작업의 pending patch는 전후 동일하다. 운영 서비스는 API/정적 웹이며 별도의 signed native 공개 릴리스나 다른 사용자의 설치본 자동 업데이트는 추가하지 않았다.

DB schema와 private 설정을 변경하지 않았다. 문제가 생기면 [운영 릴리스 절차](../../deploy/4090/release/README.md)에 따라 current를 이전 release로 돌리고 API를 재시작한다. 이전 release와 DB 백업은 보존했다.

## 2026-11-11 전체 도구 annotations 운영 배포

2026-11-11 12:18:26 KST(03:18:26Z), [behavior hints 계약](../spec/mcp.md#behavior-hints-design-and-regression-contract-2026-11-11)을 기존 API·정적 웹 운영에 반영했다.

| 항목 | 결과 |
| --- | --- |
| 소스 커밋 | `53d17230ec1f98ca383614729fa83f710cc1e472` |
| 활성 릴리스 | `20261111T030635Z-main-53d1723` |
| 이전 릴리스 | `20261109T204458Z-main-ac64d9c` |
| API SHA-256 | `813e0d89eae8dfedf78577b27ff0b5520e1ebecd019fb6db09f9de611c6a702d` — 실행 중 프로세스의 바이너리와 일치 |
| DB 백업 | `/var/backups/web-agentic-memory/20261111T030635Z-main-53d1723/production-before.dump`, 권한 `0600` |
| 데이터·설정 보존 | 기존 메모리·버전 ID 38개, 공식 점수 880점, private/systemd/Caddy 설정 12개 해시·권한·소유자 동일 |

원인은 catalog의 일부 도구에만 부분적인 annotations를 넣고, 발견 테스트도 일부 필드만 확인한 데 있었다. 배포 전 공개 MCP에서 10개 도구 모두 네 boolean hints 중 하나 이상이 누락된 것을 재현했다. 배포 후 `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`가 모든 도구에서 실제 동작에 맞는 명시적 boolean이다. Recall은 사용량·새 영수증을 기록하므로 writable/non-idempotent이고, withdrawal은 destructive/non-idempotent, 동일 outcome 재보고는 idempotent다. Proposal은 외부 model/browser replay를 유발할 수 있어 open-world다.

| 검사 | 결과 |
| --- | --- |
| 격리·소스 무결성 | 깨끗한 pinned checkout, 운영 전용 미커밋 소스 0개. manifest 639개·source archive 472개를 커밋과 대조했고 업로드 9개 파일 SHA-256 통과 |
| Linux 서버·격리 PostgreSQL | 452 passed / 0 failed. 기본 ignored 31개 중 application 18개·email/admin 2개·diagnostics 2개를 별도 실행한 결과 포함. 릴리스 빌드 성공 |
| Linux client/CLI/MCP | 103 passed / 0 failed / secure-store 1 ignored, 두 release 실행 파일 빌드 성공. 공유 target 대신 이번 릴리스 전용 target 사용 |
| 웹 | 81 passed / 0 failed, production build 성공, Svelte check 0 errors·기존 SetupGuide 경고 2개 |
| 배포 전 실제 백업 복원 | 새 production dump를 네트워크가 차단된 일회용 PostgreSQL에 복원. 21개 테이블의 COPY 데이터와 sequence 상태가 dump와 일치하고 미검증 제약 0개. 일회용 컨테이너 제거 |
| 활성화 | 공유 잠금·expected-previous·DB 백업·API/admin/review 경계·공식 작성자·정적 페이지 검사 통과. 자동 rollback 없음, 이전 release와 백업 보존 |
| 공개 HTTPS MCP | 17개 묶음 검사와 명시적 legacy status 2개 통과. JSON/SSE 각각 2025-11-25·2025-06-18·2025-03-26·2024-11-05·HTTP 헤더 생략을 검증. 2024 annotations 생략, 2025-03 및 헤더 생략의 outputSchema/structuredContent 생략 유지 |
| 공개 실제 조회 | status 어휘·text/structured 동일성, positive anonymous recall, version-pinned expand, owner usage 401 challenge, apex/API endpoint 및 health·영문/한국어/guide 200 통과 |
| Catalog 크기 | 지침 포함 4,165 UTF-8 bytes, annotations 제외 3,120 bytes. 각각 4,400/3,200-byte 게이트 이내이며 실제 host token 수가 아님 |

운영 스키마와 private 설정은 그대로다. `caddy.env`는 배포 스크립트가 작성하고 systemd/Caddy가 DNS 인증서 challenge에 사용하는 필수 운영 파일이며, 메모리 도구 핸들러는 이를 읽지 않는다는 경계를 README에 문서화했다. Linux CLI/MCP 실행 파일은 이번 빌드의 검증 산출물이며, 운영 서비스는 API·정적 웹이다.

서버 `builds/20261111T030635Z-main-53d1723/`에 패키지·manifest·Linux 로그·바이너리와 해시·복원 검증을 보존한다. 로컬 Git 제외 `build/deploy-4090/mcp-annotations-20261111/`에는 사전 명세, 배포 전후 MCP 응답 검사, 운영 기준선, 활성화·실행 프로세스 검증과 원본 로그가 있다. 복구는 위 운영 릴리스 절차에 따라 이전 release로 전환한다. 기존 README·데모 미완료 작업은 유지했다.
