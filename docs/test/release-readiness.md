# 출시 전 수용 기준과 현재 공백

확인일: 2026-10-30. 이 문서는 공개 client 배포와 hosted service 출시의 통과 조건을 고정한다. 코드가 있거나 synthetic fixture가 통과했다는 사실만으로 실계정·플랫폼·운영 검증을 완료 처리하지 않는다. 현재 checkout은 여러 세션의 미커밋 작업이 포함된 개발 트리이며, 아직 고정된 release candidate가 아니다.

운영 환경 변수·정적 웹/API 라우팅·OAuth 승인 페이지·SSE·worker/replay·배포 후 확인과 롤백은 [운영 배포 절차](../operations/deployment.md)에 모았다. 이 문서의 수용 게이트와 함께 확인한다.

출시 직전 기능/명세 보완: 익명 MCP 첫 연결→도구 발견→status→recall→pinned expand가 과도한 write 비용으로 429에 걸리던 #94를 동작별 quota로 수정했다. 실패 회귀를 먼저 확인했고 API/MCP/limiter 33개 및 대기·자동 재시도 없는 실제 HTTP·stdio·CLI 점검 10개를 통과했다. 쓰기·알 수 없는/잘못된 envelope·64 KiB 초과 body는 기존 제한을 유지한다. 설치/host 테스트의 도구 수를 10개로 맞추고 존재하지 않는 `wam mcp` 대신 별도 `wam-mcp`와 현재 local 등록 도우미를 명시했다. 이 결과는 로컬/합성 데이터 검증이며 live OAuth·운영 배포 수용 결과가 아니다.

## 감사에서 수정한 항목

| 이슈 | 재현한 공백 | 구현·검증 |
|---|---|---|
| #73 | analytics 401 뒤 기존 owner 데이터 유지; 요청 교체 뒤 analytics busy 잔류; 모든 503을 OAuth 설정 오류로 표시 | 공통 owner-state 초기화, generation 경계와 503 메시지 보완. WEB-30~32 Playwright 실패를 재현 후 통과 |
| #74 | SIGTERM이 SIGINT와 달리 SSE shutdown/diagnostics flush 경로를 건너뛰고 signal 15로 종료 | Unix SIGTERM을 같은 정상 종료 경로에 연결. 실제 일회용 API + idle SSE 테스트 실패 후 통과 |
| #75 | 필수 Linux ARM64가 release/installer/platform CI 대상에 없음 | 다섯 native target의 package/installer 매핑과 누락 거부를 검증. GitHub Actions는 사용자 요청으로 제거했으며 native runtime 검증은 별도 |
| #80 | 10개 MCP catalog의 설명+스키마가 기존 추정 800 token budget을 초과 | 4092 byte 실패를 먼저 재현. 필드/검증/동의/owner/outcome 의미를 보존하며 중복 설명을 축약하고 고정 UTF-8/4 proxy 회귀 검사 추가 |
| #81 | scanner만 있고 실제 transport/sink 수집·coverage 판정이 없음; JSON escape된 multiline secret 누락 | 500개 합성 sentinel·8종 로컬 sink 수집, hash/누락/변조/scan-error 판정 및 500개 양성 대조군. JSON/nested-JSON 형태 회귀 실패 후 수정 |
| #82 | Chrome에 한정된 guide/dashboard 검사, mobile/keyboard/a11y 범위 부족 | Playwright 1.64.1, Chrome/Firefox/WebKit 각 21개 + mobile Chrome 7개. 로컬 browser 설치·실행으로 검증; macOS Firefox는 일회용 app-data home과 teardown으로 격리 |
| #83 | axum Json 기본 오류가 비공개 field 이름·enum 값을 응답에 반사 | 공통 extractor에서 상태 의미를 유지하고 고정 JSON error/no-store만 반환. field/enum/type·malformed JSON·415/413 회귀 통과 |
| #84 | 연결·기여·로그인·대시보드의 낮은 텍스트 대비와 표시 이름/접근성 이름 불일치 | 실제 WCAG A/AA 실패를 재현 후 색상/이름 수정. 로그인한 analytics·score history 포함 자동 위반 0; axe rule 제외/disable 없음 |
| #88 | 실제 native DB 복원·진행 중 write 종료·Redis 장애복구·규모 profile·소스 설치의 실행 증거 부족 | owned PostgreSQL/Redis 복구 7개, macOS CLI/MCP·두 host 설치/제거, Keychain round trip, release 검색 1k/10k/100k 측정. production·signed release·실계정과 구분 |

2026-10-30 검증: Rust workspace **347 pass/23 external ignored**, strict Clippy/fmt, Python **194**, web unit **25**/Svelte check 0/build 통과. JSON 오류 회귀 확장 후 diagnostics 9개와 package Clippy도 통과했다. Browser **70** + privacy **1** + lifecycle **1** + OAuth **2** 통과. 키보드 가이드 복사 강화 후 네 프로젝트의 관련 4개를 재검증했고 OAuth 로그인/승인도 키보드로 실행했다. Privacy 증거는 `target/e2e-logs/security-sinks/2026-10-30T02-12-55-502Z-91a4/`에 있으며 source `dirty=true`, synthetic-only로 기록한다. 실제 8종 sink의 0 leak/0 scan error, 의도한 오염 500개 탐지와 owned client 정리가 확인됐다. 개발 fixture의 JSON/SQLite·scripted provider 결과이며 PostgreSQL/Redis/WAL·실제 모델·staging 결과로 승격하지 않는다.

실행은 `pnpm --dir web run test:ui` 및 `pnpm --dir web run test:security`다. 로컬 Node 26의 이전 Playwright 설치가 100%에서 멈춘 경우 Node 22로 공식 installer를 실행해 복구했고 최종 browser matrix도 Node 22로 실행했다. GitHub Actions 자동 실행은 제공하지 않는다. macOS 27 Firefox의 app-data lookup 문제는 [upstream #42768](https://github.com/microsoft/playwright/issues/42768)을 참고한다. `CFFIXED_USER_HOME`을 새 홈으로 분리하고 표준 디렉터리를 미리 생성했을 때 이 기기에서 기동과 teardown이 통과했다. macOS WebKit 키보드 검사는 [Option-Tab 동작](https://support.apple.com/en-gb/guide/safari/cpsh003/27.0/mac/27)을 반영한다. 실제 Safari/VoiceOver 인증은 아니다.

## 출시 프로필과 고정 게이트

`client`는 CLI/MCP 공개 바이너리 배포, `service`는 client 조건을 포함한 hosted service 출시다. 성능 이득을 홍보하려면 어느 프로필이든 `--claims`를 추가한다. 통과 기준 수치는 [metrics.md](metrics.md)를 따르며, DB 지연처럼 **목표**로 정의된 수치를 측정 후 임의의 필수 게이트로 바꾸지 않는다. 목표 미달과 필수 게이트 실패는 보고서에서 따로 설명한다.

정확한 case 목록·허용 증거 종류는 [release-gates.json](release-gates.json)에 있다. 다음 표는 해당 ID의 테스트 명세다.

| ID | 범위 | 통과 oracle와 필수 증거 |
|---|---|---|
| REL-01 | 두 프로필 | 고정 candidate의 Rust fmt/Clippy/workspace, Python, web unit/check/build, UI·lifecycle·OAuth E2E가 실패 0. web-ui 증거에는 metrics.md의 Chrome/Safari/Firefox·mobile/keyboard/accessibility·웹 성능 기준 coverage를 적고 Chrome 통과만으로 다른 브라우저를 완료 처리하지 않음. MCP catalog 수/정의/instructions는 고정 proxy budget 이내이며 실제 host tokenizer 수를 별도 보고. ignored 외부 테스트는 별도 결과가 있어야 하며 기본 suite로 대체하지 않음 |
| REL-02 | 두 프로필 | candidate lockfile에 대한 Rust/web dependency advisory 검사에서 high 이상 0. 예외를 숨기거나 scanner 부재를 pass로 표시하지 않음 |
| REL-03 | 두 프로필 | Linux x64/ARM64, macOS Intel/ARM64, Windows x64의 실제 native build/test/CLI·MCP 시작·종료. package 대상과 installer target의 집합이 같음 |
| REL-04 | 두 프로필 | 각 타깃의 실제 GitHub OIDC bundle·정확한 signer·SHA256·source commit 확인 후 설치/호스트 등록/제거. stub verifier 양성 테스트는 수용 증거 아님. Apple/Windows 플랫폼 signing 요구와 배포 제약도 보고 |
| REL-05 | 두 프로필 | 광고하는 Codex/Claude/Arise에서 실제 GitHub 승인 → private rank/usage → token refresh/revoke → 즉시 401 → 제거. 웹 로그인, 등록, 발견, 인증, 호출 단계를 별도 확인. 실제 계정 자료는 비공개로 보관하고 공유 보고서에는 식별값을 제거 |
| REL-06 | 두 프로필 | macOS Keychain, Linux Secret Service, Windows Credential Manager에서 disposable credential 저장/읽기/삭제; 실제 token이 argv/config/plaintext에 없음. 테스트가 만든 항목만 제거 |
| REL-07 | 두 프로필 | 합성 sentinel ≥500으로 client/web/API/DB/diagnostics/cache/process/Gatekeeper sink를 **각각** 수집·scan. inventory에 누락된 sink가 없고 유출 0, scanner 오류/skip도 실패. 실제 credential을 corpus로 사용하지 않음 |
| REL-08 | service | 운영과 같은 PostgreSQL/Redis/TLS/proxy 설정에서 기여→게시 +20→명시 사용 +2→재시작/재시도 멱등성, 백업 복원, Redis outage/failover, write 중 SIGTERM, TTL/삭제 재시도, 경보 및 HTTPS 보안 헤더. liveness `/healthz` 200만으로 종속 서비스 준비를 판정하지 않음 |
| REL-09 | service | 독립 라벨 recall ≥500/held-out 및 curator ≥60, 실제 provider/replay 평가. generated labels·scripted 60개를 인간 검증 데이터로 계산하지 않음. metrics.md의 품질/누출/주입 기준에 대한 전체 결과 공개 |
| REL-10 | service | 고정 runner/seed에서 1k/10k/100k, 별도 1M stress와 limiter/queue 장애 복구를 반복 측정. core/DB/API 지연·품질·RSS·쿼리계획·throughput·error를 분리하고 모든 run을 유지. 성능 목표 미달은 측정값과 원인을 보고 |
| REL-11 | 두 프로필 | 프로젝트 LICENSE 선택/원문, reference 출처·재배포 권한, 작동하는 비공개 보안 연락 경로, 첫 공개 범위(client/service)를 확정한 검토 기록. 파일 존재만으로 권리 검토를 대신하지 않음 |
| REL-12 | `--claims` | 사전 등록한 T/M/I 교차 실험, 동일 model/effort/prompt/tools, 반복 수·무효율·독립 oracle·전체 실패 포함. A/B/C 주장 등급과 README/landing 문구가 metrics.md와 일치 |
| REL-13 | 판정기 자체 | 누락/중복/unknown case, 잘못된 source/policy hash, dirty 결과, stale/future timestamp, fixture로 대체한 live 증거, 변조/외부 경로/symlink/oversize artifact를 거부. pending/blocked/skipped는 pass가 아님 |
| REL-14 | 명세·자동화 일치 | gate ID가 이 문서에 정의되고, package/installer의 필수 타깃이 일치하고 로컬 browser 설치 뒤 UI 검사를 실행. GitHub Actions를 필수 실행 경로로 가정하지 않음 |

## 운영 재현 절차

2026-10-30 추가 검증 결과:

| 검증 | 실제 결과 | 증거 |
|---|---|---|
| native PostgreSQL/Redis 통합 회귀 | application/diagnostics/cleanup/quota 16개 + Gatekeeper 4개, 실패/ignored 0 | `target/e2e-logs/release-data-services/20261030-032332-5fb0/` |
| 로컬 복구 rehearsal | 전체 table/schema hash가 같은 dump/restore·PG 재시작; +20/+2·device 인증·immutable 제약 보존; 진행 중 write의 SIGTERM drain/flush/멱등 retry; Redis 503·수동 승격 후 quota 보존과 API 복구; cleanup 포함 7개 통과 | `target/e2e-logs/release-services/20261030-025758-64bd/` |
| macOS ARM64 source 설치 | 실제 `cargo install --locked --offline`의 CLI status 및 stdio initialize/list 10개/positive recall/EOF; Codex 0.165.0·Claude 2.1.314 등록/재등록/제거/재제거·다른 등록 보존; cleanup 포함 4개 통과 | `target/e2e-logs/native-install/20261030-031817-302b/` |
| macOS Keychain | 실제 OS store의 고유 dummy credential 저장/읽기/삭제, plaintext 파일 없음 | `target/release-verification/macos-keychain.log` |
| release 검색 profile | 1k/10k/100k 실제 PG에서 7 case × 5회 × 10표본, exhaustive core·DB·loopback HTTP의 ID/version/score 일치, HTTP error 0; raw percentile·계획·RSS 보존. 100k HTTP 지연 목표는 미달 | `target/e2e-logs/search-profile/20261030-032442-429e/` |
| 검증 도구 회귀 | Rust workspace·strict Clippy/fmt, generator 계약 1개, Python 전체 201개 통과. timeout/누락·중복 case/NaN/표본과 percentile 불일치 및 RSS 감시 실패를 성공 처리하지 않는 신규 8개 포함 | 현재 dirty 개발 트리의 로컬 실행; RSS 감시 수정 후 실제 1k 재실행도 통과 (`search-profile/20261030-032805-56b5/`) |

공유 트리에서 동시에 추가되는 landing/demo 미디어도 점검했다. 생성 중에는 web unit 26/27 및 누락 media의 prerender 404를 재현해 #98로 기록했다. 기존 생성 작업이 끝난 뒤 **web unit 27/27, Svelte check 오류/경고 0, static build 통과**로 재검증해 해결했다. 증거는 `target/release-verification/web-final-20261030-033631/`다. 이 디렉터리의 첫 build는 분석 단계의 일시적인 `ERR_MODULE_NOT_FOUND`로 실패했으며, 그 로그를 보존한 뒤 단독 재실행한 `build-retry.log`가 exit 0이었다. 원인이 동시 build인지 단정하지 않는다. 영상의 시각 검수나 변경된 landing의 browser UI 재검증까지 수행했다는 뜻은 아니다. 이전 70개 UI 결과와 현 소스의 unit/build 결과를 구분한다.

완료 run의 규모별 worst-trial p95(DB/HTTP)는 1k **2.3/16.5ms**, 10k **2.1/105.7ms**, 100k **15.8/553.9ms**였다. 100k의 HTTP 500ms 목표 초과를 #97로 등록했다. 앞선 별도 100k run은 11.6/415.2ms였으며 두 실행을 모두 보존한다. 세 규모 모두 이전 실행과 corpus hash가 같았다. 100k probe peak RSS는 각각 약 1905/2046MiB로 exhaustive oracle와 embedded API를 포함하고 PostgreSQL은 제외한다. 고정 7 query·warm cache·concurrency 1·fixture quota의 결과이며 100 RPS/5분·production 용량 인증은 아니다. 초기 2048MiB 자원 상한 실행의 실패도 보존했고, 4096MiB 상한으로 완료했다. 1M stress는 이 16GiB 기기에서 실행하지 않았다. [workload와 남은 수용 범위](retrieval-benchmark.md), [실행 조건](../../tests/bench/README.md#native-search-profile)을 함께 확인한다.

GitHub read-only live 확인은 현재 인증되지 않은 `gh`가 exit 4를 반환해 미확인이다 (`target/release-verification/github-readonly.json`). Cosign/서명된 release artifact도 없어 REL-04를 완료하지 않았다. 소스 설치와 OS store의 성공을 다른 OS·실계정 OAuth·서명 검증 증거로 승격하지 않는다.

새 실행 명령은 `python3 -B tests/env/verify_release_services.py`, `python3 -B tests/env/verify_native_install.py`, `python3 -B tests/bench/profile_search.py`다. 상세 prerequisite/격리는 [native 환경 guide](../../tests/env/README.md#native-release-rehearsal)를 따른다.

로컬 component DB/Redis 회귀는 `python3 -B tests/env/check_data_services.py`로 실행한다. 설치된 `initdb/postgres/psql/redis-server/redis-cli/cargo`가 필요하다. runner는 새 cluster/namespace/포트를 만들고 PostgreSQL data_directory와 Redis process_id가 자신이 만든 것인지 확인한 뒤에만 테스트 URL을 전달한다. 실제 daemon/설정 DB에 붙지 않으며 로그와 종료·정리 결과를 `target/e2e-logs/release-data-services/`에 남긴다. 의존 도구가 없으면 명시적으로 실패하고 생략을 pass로 계산하지 않는다. 이 검사는 아래 운영 복원/failover 부하 검증을 대체하지 않는다.

REL-08은 전용 staging DB/Redis와 격리된 계정 A/B로 실행한다. A가 동의해 기여하고 실제 worker/replay 뒤 immutable v1과 +20을 확인한다. B가 recall receipt의 사용 버전을 명시해 success를 보고하고 A에게만 +2가 한 번 생겼는지 SQL로 확인한다. 응답 유실 후 동일 key를 재시도하고 서버/worker를 재시작해도 row/ledger/outbox 합계가 같아야 한다.

그 snapshot을 별도 DB에 restore해 catalog/receipt/ledger/profile/identity 제약을 다시 확인한다. 기여와 publication 사이, publish commit 직전/직후, Redis 거절/장애, 계정 삭제 cleanup 도중의 장애는 [failure-matrix.md](failure-matrix.md) 순서로 주입한다. idle SSE와 진행 중 write가 있을 때 SIGTERM을 보내 admission 중단·stream 종료·진단 flush·lease 회복을 기록한다. 수집 로그에는 DB URL, API key, cookie, raw 사용자 URL을 남기지 않는다. 복구 테스트가 성공한 실제 snapshot/배포/명령 버전을 기록하며 문서만으로 운영 복구를 pass 처리하지 않는다.

REL-07의 scanner는 `tests/security/scan_sentinels.py`를 사용한다. 로컬 수집은 `pnpm --dir web run test:security`, inventory 판정은 `tests/security/check_sink_inventory.py`로 실행한다. 수집 inventory는 각 sink의 artifact 경로/hash, scan 결과, 미수집 사유와 coverage/runtime/source metadata를 보존한다. 단일 디렉터리가 깨끗하다는 결과를 전체 DB/API/cache/process 검증으로 확장하지 않는다. [수집 범위와 명령](../../tests/security/README.md)을 따른다.

## 증거 판정 도구

```sh
python3 tools/release/readiness.py --init target/release-evidence/candidate.json \
  --candidate YOUR_FULL_GIT_COMMIT --profile service
python3 tools/release/readiness.py --evidence target/release-evidence/candidate.json \
  --candidate YOUR_FULL_GIT_COMMIT --profile service --json
```

`--init`은 pending template만 만들며 기존 파일을 덮어쓰지 않는다. 보고서/로그 요약을 template 옆에 보관하고 각 record에 상대 artifact 경로와 SHA256, UTC observed_at, 실제 실행 종류, fixture 여부, source commit을 채운다. 수동 권리/연락 경로 검토에는 reviewer도 적는다. plaintext credential을 metadata/command에 넣지 않는다.

정책 SHA256과 각 record의 candidate SHA가 같아야 한다. CLI는 실제 checkout HEAD/dirty 상태도 확인한다. 증거는 14일 이내, 미래 clock 허용은 최대 5분이다. artifact는 manifest 디렉터리 안의 일반 파일만 허용하고 symlink, traversal, 빈 파일, 64 MiB 초과를 거부한다. schema/duplicate JSON key도 엄격히 검사한다. 관측 일시·실행 종류를 바꿔 실제 시험을 가장하는 것을 도구가 증명할 수는 없으므로 artifact의 내용·oracle·출처를 릴리스 검토에서 확인한다.

종료 코드 0은 선택한 프로필의 **선언된 증거와 파일 무결성**이 조건을 만족했다는 뜻이다. 실제 배포/권리/모델 품질을 새로 인증하거나 publish하지 않는다. 필수 증거 부족은 1, 잘못된 입력/schema는 2다. `--claims` 없는 성공으로 성능 개선 주장을 승인하지 않는다. draft 생성은 준비 작업으로 허용되며 공개 publish 전에 이 검사를 수행한다.

## 현재 남은 출시 차단 항목

2026-10-29 로컬 검증: Rust workspace tests, strict Clippy, rustfmt 통과; MCP catalog 변경 후 MCP/server 표적 58개 통과. Python 184개, web unit 23개, Svelte check 0 error/warning·build, UI 14개 및 lifecycle 1/OAuth 2개가 통과했다. 새 PostgreSQL/Redis fixture에서 application/diagnostics/cleanup/quota 16개, Gatekeeper queue/store 4개가 ignored 없이 통과했고 생성 리소스 정리를 확인했다 (`target/e2e-logs/release-data-services/20261029-143611-3589/`). Catalog는 4092 → 3179 UTF-8 bytes, 고정 proxy 1023 → 795로 줄었으며 실제 host tokenizer 수를 뜻하지 않는다.

실행 가능한 pending 증거 template와 차단 보고서는 `target/release-audit/preflight-20261029-144712/`에 있다. service 프로필은 49개 필수 case를 요구하며 현 개발 트리는 dirty checkout과 미제출 candidate 증거 때문에 통과하지 않는다. 이 숫자는 49개 제품 결함을 뜻하지 않는다.

현재 HEAD용 pending template/판정도 `target/release-verification/preflight-20261030-033239/`에 남겼다. 실제 CLI는 exit 1/`ready=false`였으며 dirty tree·미제출 candidate 증거·root LICENSE 부재를 확인했다. 위 로컬 성공 결과를 clean candidate나 production record로 임의로 채우지 않았다.

- 고정된 clean candidate와 다섯 native target 실행·서명 artifact 설치 증거가 없다. macOS ARM64 source 설치와 정적 matrix 검사는 다른 네 타깃과 서명 증거를 대체하지 않는다. GitHub Actions는 사용자 요청으로 제거했으며 자동 release를 제공하지 않는다.
- 실제 GitHub/Claude/Arise OAuth, Windows/Linux native store, production DB 복원·Redis failover·부하·write-in-flight 종료가 남아 있다 (#25/#34/#36/#27). macOS store와 로컬 native 복원·수동 Redis 승격·write-in-flight 검증은 통과했다.
- Chrome/Firefox/WebKit 및 mobile·keyboard·자동 a11y 70개는 통과했다. 실제 Safari/모바일 기기·수동 WCAG/스크린리더와 Lighthouse/Web Vitals 기준은 미확인이다. Playwright WebKit을 실제 Safari 수용 시험과 구분한다.
- 독립 라벨/실모델 replay, PostgreSQL/Redis/WAL·native host/provider/staging sink 수집, 전용 runner의 동시 부하·1M stress 및 실제 agent demo가 남아 있다 (#10/#15/#39). 로컬 8종 수집과 1k/10k/100k profile이 구현·실행됐으며 나머지 환경에도 같은 inventory/scan 계약을 적용해야 한다.
- 로컬 100k HTTP worst-trial p95 553.9ms가 500ms 목표를 넘었다 (#97). DB 후보 조회 p95 15.8ms와의 격차는 전역 vocabulary 조회 등의 분리 계측과 전용 runner 재현으로 원인을 특정해야 한다. correctness pass와 latency 목표 충족을 분리하며, 임의 LIMIT/threshold 완화로 숨기지 않는다.
- root LICENSE(Apache-2.0)는 2026-11-01에 추가했다. REL-11은 reference 권리 검토, 작동하는 보안 연락 경로 확인, 공개 범위 선택의 검토 기록이 남아 있어 아직 통과가 아니다. `docs/open-source-readiness.md`의 해당 체크를 임의로 완료 처리하지 않는다.
- lockfile audit(2026-10-29): Rust vulnerabilities 0, web high/critical 0. Web의 deepmerge 4.3.1 low advisory GHSA-gcqx-w48m-w2h4와 Rust Linux secure-store 전이 의존성의 unmaintained 경고 3건은 별도 추적한다. `target/release-audit/` 보고서는 현재 dirty 개발 트리의 관측이며 clean candidate의 REL-02 결과로 자동 승격하지 않는다.
- [metrics.md](metrics.md)의 MCP 도구 수 4는 실제 10개 계약과 불일치해 갱신했고, 정의의 고정 UTF-8/4 proxy가 800 이내가 되도록 설명을 축약했다. Native MCP 초기화 시간과 cached source 설치 시간은 수집했다. 실제 host tokenizer·깨끗한 기기의 다운로드/컴파일·실계정 첫 사용 시간은 별도 측정해야 하며 기존 임계값을 측정 없이 완화하지 않는다.
- macOS Firefox 초기 실패 probe의 OS 보호 임시 홈 3개는 정리 EACCES로 #87에서 추적한다. 새 runner의 기동·전체 70개·teardown은 통과했고, 기존 사용자 Firefox 홈/OS 권한은 변경하지 않았다. 초기 probe의 잔여 경로와 새 실행의 정리를 구분한다.

실계정·배포·라이선스 결정을 코드나 합성 결과로 만들어 채우지 않는다. 남은 blocker는 공유 이슈에서 추적하고, 출시 시 이 표와 증거를 함께 검토한다.
