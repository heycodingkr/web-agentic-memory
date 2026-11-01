# 운영 배포 절차

2026-10-30 코드 기준. 이 문서는 **정적 웹 + Rust API/worker + PostgreSQL + Redis를 같은 HTTPS origin으로 제공하는 배포**의 설정과 확인 순서다. 호스팅 업체·운영 도메인·실제 GitHub OAuth App·모델/replay 실행 파일은 배포 입력이다. 예시 `https://wam.example`을 실제 주소로 바꾼다. GitHub에 소스를 push하는 것과 웹 서비스를 배포하는 것은 별개이며 GitHub Actions 자동 실행은 제공하지 않는다.

출시 판정은 [release-readiness.md](../test/release-readiness.md)가 기준이다. 아래 절차나 로컬 fixture 통과만으로 실계정·운영 복구·권리 검토가 완료되지는 않는다.

## 1. 배포할 소스와 범위 고정

- 공개할 기능과 지원 host/OS를 확정하고 commit 하나를 고정한다. 다른 세션의 미완료 작업을 함께 배포하지 않는다.
- 웹/remote MCP 서비스와 공개 CLI 바이너리의 배포물을 구분한다. 현재 CLI 사용 경로는 [Cargo 소스 설치](../../README.ko.md)이며 자동 서명 릴리스는 없다.
- [출시 게이트](../test/release-gates.json)의 client/service/성능 주장 범위와 필요한 증거를 기록한다. 실행하지 않은 gate를 pass로 채우지 않는다.
- 프로젝트 라이선스·reference 배포 권한·비공개 보안 신고 경로·공개 범위는 #78의 결정이 필요하다. 보안 문서의 조건부 연락 문구만으로 작동하는 신고 경로를 확인했다고 보지 않는다.

## 2. 빌드

저장소 루트에서 실행한다. Rust, Node와 lockfile에 맞는 pnpm 환경을 기록한다.

```sh
cargo build --locked --release -p wam-server --bin wam-server --bin wam-worker
pnpm --dir web install --frozen-lockfile
VITE_SITE_URL=https://wam.example \
VITE_WAM_MCP_ENDPOINT=https://wam.example/mcp \
pnpm --dir web run build
```

API/worker는 `target/release/wam-server`, `target/release/wam-worker`, 정적 웹은 `web/build/`다. `web/build/200.html`은 동적 `/u/{handle}`, memory 상세와 account 경로의 SPA fallback이다. 정적 자산과 API 응답을 같은 fallback으로 처리하지 않는다.

동일 origin 구성에서는 `VITE_WAM_API_URL`을 설정하지 않는다. 다른 API origin이 꼭 필요하면 session cookie·CSRF·CORS·OAuth callback까지 별도로 검증한다. `VITE_*` 값은 공개 웹 번들에 들어가므로 비밀값을 넣지 않는다. 웹 origin, OAuth resource 또는 소개 이미지 주소가 바뀌면 다시 빌드한다.

## 3. API 환경과 비밀값

비밀값은 호스팅의 secret store나 접근을 제한한 환경 파일로 전달한다. 저장소·웹 번들·호스트 MCP 설정·공개 로그에 넣지 않는다. API는 다음 값을 받아 실행한다.

| 설정 | 운영 값과 확인 사항 |
|---|---|
| `WAM_ENV` | `production` |
| `WAM_BIND` | 예: `127.0.0.1:3000`. 이 예시는 같은 머신의 TLS 프록시 뒤에서 API를 제공한다 |
| `WAM_PUBLIC_ORIGIN` | 실제 HTTPS 웹 origin. userinfo·query·fragment·추가 경로를 넣지 않는다 |
| `WAM_DATABASE_URL` | 전용 PostgreSQL DB/계정. 원격 연결의 인증서 검증 TLS를 사용하고 백업·복원 대상을 기록한다 |
| `WAM_RATE_LIMIT_REDIS_URL` | 공유 Redis primary. loopback 밖에서는 `rediss://`; 모든 API replica가 같은 quota store를 사용한다 |
| `WAM_RATE_LIMIT_KEY_SECRET` | 32바이트 이상 비밀값. replica 간 동일하게 유지하며 rotation 때 quota/삭제 projection 처리도 계획한다 |
| `WAM_GITHUB_CLIENT_ID`, `WAM_GITHUB_CLIENT_SECRET` | 이 서비스의 실제 GitHub OAuth App. 단순 GitHub CLI/PAT 인증으로 대신할 수 없다 |
| `WAM_GITHUB_REDIRECT_URI` | `https://wam.example/v1/auth/github/callback`. GitHub App 등록 값과 일치시킨다. production에서 기본 loopback callback을 사용하지 않는다 |
| `WAM_MCP_PUBLIC_URL` | `https://wam.example/mcp`. `/mcp` 경로만 사용하고 query·fragment·credential을 넣지 않는다. 설정해야 remote MCP OAuth discovery가 활성화된다 |
| `WAM_MCP_ALLOWED_ORIGINS` | browser MCP client의 정확한 origin 목록. 예: 같은 origin인 `https://wam.example`; wildcard나 임의 origin 반사를 사용하지 않는다 |
| `WAM_TRUSTED_PROXY_CIDRS` | 실제 API 직전 proxy peer만 신뢰한다. loopback proxy만 접근 가능한 예에서는 `127.0.0.1/32,::1/128`. 비워 두면 proxy 뒤 사용자가 한 IP quota를 공유할 수 있다 |
| `WAM_MAX_INFLIGHT_REQUESTS` | 기본 256. DB/Redis pool과 부하 시험에 맞춰 결정한다 |
| `WAM_LOG_DATABASE_URL` | 선택값. 생략하면 운영 application DB에 diagnostics를 저장한다. 별도 설정 시 그 DB의 접근·retention·백업도 관리한다 |
| `WAM_REVIEW_TOKEN` | 선택적인 검수 operator 비밀값. 일반 사용자에게 제공하거나 웹 번들에 넣지 않는다. 검수 접근 통제와 보관 절차를 별도로 검토한다 |

`WAM_DEMO_TOKEN`은 운영 환경에서 **미설정**이어야 한다. 빈 문자열 설정도 production 시작을 거부한다. PostgreSQL은 빈 catalog로 시작하며 개발용 demo seed나 임시 snapshot이 운영 데이터가 되지 않는다. API 시작 시 migration을 적용하므로 schema 소유권·권한과 동시에 시작할 replica 수를 준비한다.

환경을 주입한 process supervisor에서 실행한다.

```sh
./target/release/wam-server
```

`/healthz`는 liveness다. 이 경로의 200만으로 DB/Redis·worker·외부 모델 준비를 판정하지 않는다. API replica는 PostgreSQL/Redis를 공유하며 개발용 `.wam-data/` 파일을 여러 process가 공유하는 방식으로 운영하지 않는다.

## 4. 프록시와 정적 웹 라우팅

| 공개 경로 | 대상 |
|---|---|
| `/v1/*`, `/healthz`, `/mcp` | Rust API |
| `/.well-known/oauth-protected-resource`, `/.well-known/oauth-protected-resource/mcp`, `/.well-known/oauth-authorization-server` | Rust API |
| `/oauth/authorize`, `/oauth/register`, `/oauth/token`, `/oauth/revoke` | Rust API |
| `/oauth/approve` | **정적 웹**의 승인 페이지/fallback |
| 나머지 웹 경로 | `web/build/` 자산, 없는 앱 경로는 `200.html` |

`/oauth/*` 전체를 API로 proxy하면 승인 화면이 사라진다. API 401/403/429/503을 SPA HTML 200으로 바꾸지 않는다. `/mcp`와 OAuth·인증·개인 API 응답은 CDN cache에서 제외하고 서버의 `no-store`, 인증 challenge와 rate-limit 헤더를 보존한다. 공개 데이터 cache도 별도 검증 없이 임의 활성화하지 않는다.

TLS 프록시는 SSE를 buffering하지 않고 초기 comment와 15초 keep-alive를 전달해야 한다. `GET /mcp` 및 `POST /mcp`의 SSE 응답, disconnect와 graceful shutdown을 모두 확인한다. `Origin`, `Accept`, `Content-Type`, `Authorization`, `MCP-Protocol-Version`, CSRF 및 필요한 CORS 응답 헤더가 유지되어야 한다. 서버는 `Last-Event-ID` replay를 지원하지 않는다.

승인 화면에는 배포 호스트에서 다음 헤더를 적용한다. 정적 adapter만 빌드하면 운영 응답에 이 헤더가 자동 적용되는 것은 아니다.

```text
Content-Security-Policy: frame-ancestors 'none'
X-Frame-Options: DENY
Cache-Control: no-store
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
```

`frame-ancestors`는 meta tag가 아닌 HTTP 헤더여야 한다. 나머지 HTTPS 페이지의 보안 헤더와 CSP는 실제 frontend 실행을 검사해 정한다. Access log에 callback code/query, cookie, bearer, 원문 요청 body를 기록하지 않는다. 운영 proxy/TLS 설정 자체는 staging에서 확인해야 한다.

## 5. 기여를 실제로 게시하려면 worker 연결

API 접수만 실행하면 기여는 영속 큐에 남는다. 모델과 독립 replay가 준비되어야 승인된 버전으로 게시된다. PostgreSQL 운영에서는 API와 같은 application DB를 사용하는 별도 worker를 supervisor로 실행한다.

| Worker 설정 | 확인 사항 |
|---|---|
| `WAM_ENV`, `WAM_DATABASE_URL` | API와 동일한 production DB |
| `WAM_LLM_PROVIDER` | 예: `openai-compat`. `fake`는 live worker가 거부한다 |
| `WAM_LLM_BASE_URL` | 실제 `<base>/responses` protocol을 지원하는 HTTPS endpoint. `deepseek`/`muse` 별칭만으로 API 호환성을 입증하지 않는다 |
| `WAM_LLM_MODEL`, `MODEL_API_KEY` | 검증한 model ID와 worker 전용 secret. Meta Model API 콘솔은 무료 크레딧 사용 중 콘텐츠가 제품 개선에 쓰일 수 있다고 표시하므로(2026-11-01 확인) 운영 키는 결제수단을 등록한 유료 계정에서 발급하고, 유료 전환 후의 조건은 별도로 확인한다. 콘솔의 월 한도는 이메일 알림뿐이라 차단형 상한은 아래 `WAM_LLM_DAILY_BUDGET_USD`로 직접 건다 |
| `WAM_LLM_REASONING_EFFORT` | 선택. `minimal`/`low`/`medium`/`high`/`xhigh`/`max` 중 하나를 `reasoning.effort`로 보낸다. 비우면 보내지 않는다. 추론 모델은 고정 출력 한도(2048)를 숨은 추론에 먼저 쓰므로 모델 기본값에서는 출력 텍스트가 없는 `incomplete` 응답이 나와 재시도·비용이 늘 수 있다. 사용할 값은 모델별로 검증해 고정한다 |
| `WAM_LLM_PRICE_INPUT_PER_M`, `WAM_LLM_PRICE_CACHED_INPUT_PER_M`, `WAM_LLM_PRICE_OUTPUT_PER_M` | 선택이지만 셋을 함께 설정한다. 1M 토큰당 달러(Meta 콘솔의 Muse Spark 1.3 표시값은 1.25 / 0.15 / 4.25). 설정하면 HTTP provider가 `usage`로 계산한 비용(`cost_usd`)을 모든 응답에 보고하고 일일 상한이 이를 센다. 일부만 설정하면 시작하지 않는다. 가격이 바뀌면 직접 갱신한다 |
| `WAM_LLM_DAILY_BUDGET_USD` | 선택. 추정 모델 지출의 UTC 일일 상한(달러, 0보다 큼). 닿으면 다음 UTC 날짜까지 새 job을 시작하지 않고 기여는 `queued`로 대기한다. job 시작 전에 검사하므로 마지막 job은 상한을 넘을 수 있고 worker 프로세스마다 한 건씩 더 넘을 수 있다. 비용을 보고하지 않는 provider(위 가격 미설정 등)와 함께는 시작하지 않는다 |
| `WAM_LLM_QUOTA_PAUSE_SECS`, `WAM_LLM_QUOTA_PAUSE_MAX_SECS` | 선택. provider가 크레딧·쿼터 소진을 알리면서 재시도 시각을 알리지 않을 때 모델 호출을 멈추는 시간. 기본 900초에서 시작해 연속 거절마다 두 배로 늘고 기본 3600초가 최대다(30~86400초). provider가 시각을 알리면 30초~최대값 안에서 그 값을 쓴다 |
| `WAM_REPLAY_PROGRAM` | operator가 관리하는 절대 실행 파일 경로. 사용자/모델이 지정한 프로그램을 실행하지 않는다 |
| `WAM_REPLAY_ARGS_JSON` | 선택적 문자열 배열. shell 문자열이 아니다 |

**크레딧·쿼터가 소진되면.** provider가 402(`billing_error`), `insufficient_quota` 또는 사용 한도를 알리면 worker는 그 job을 `waiting_budget`으로 두고(시도 소모 없음) 모델 예산 게이트를 닫는다. 이후에는 새 기여도 `queued`로 접수만 되고 replay와 모델 호출은 일어나지 않으며 recall은 영향이 없다. 상태는 `wam-worker --budget-status`(JSON: 닫힌 이유, 재개 시각, 오늘 지출, 대기 job 수), outbox 이벤트 `worker_budget_paused`/`worker_budget_resumed`, 큐 건강 지표로 본다. 대기 job은 큐 건강 지표에 포함되므로 가장 오래된 job의 나이가 알림 기준을 넘으면 알린다. 크레딧을 충전한 뒤에는 `wam-worker --resume-budget`으로 기다리지 않고 재개한다. 그대로 두면 worker가 스스로 다시 시도한다(시각을 알리지 않으면 15분, 30분, 1시간 간격이고 이후 1시간마다 한 건). 일일 상한은 `--resume-budget`으로 풀리지 않고 다음 UTC 날짜에 열리거나 상한을 올려 재시작하면 열린다. 상태는 데이터베이스에 있어서 worker를 재시작하거나 늘려도 같다.

이 기능은 PostgreSQL migration `0003`(work 상태 `waiting_budget`)을 포함하며 worker나 API가 연결할 때 자동 적용된다. migration 뒤에 이전 binary로 되돌려야 하면 새 worker를 멈추고 `wam-worker --resume-budget`으로 대기 job을 `queued`로 돌려놓은 다음 되돌린다. 이전 binary는 `waiting_budget` 상태의 job을 가져가지 않는다.

```sh
./target/release/wam-worker
```

별도 worker를 쓰면 API의 `WAM_RUN_WORKER`는 미설정으로 둔다. 개발 file mode에서는 API와 별도 worker가 같은 snapshot lock을 동시에 열 수 없으므로 `WAM_RUN_WORKER=1` 경로를 사용한다. Replay 입출력 계약·timeout·digest·성공 조건은 [서버 README](../../crates/wam-server/README.md#verified-publication)가 기준이다. 고정 성공 JSON을 반환하는 테스트 fake로 운영 replay를 대신하지 않는다.

첫 운영 기여가 receipt → work → 독립 replay → immutable version → +20으로 이어지는 것을 확인한다. 다른 계정의 명시적 `used_version_ids` outcome은 +2를 한 번만 만들고, 재시도·철회·동의 만료·계정 삭제는 점수/게시 정책을 지켜야 한다. 미처리 큐·재시도·검수 대기·worker 실패에 경보를 연결한다.

## 6. 외부 주소에서 배포 후 확인

브라우저가 실제 사용하는 HTTPS 주소에서 검사한다. Vite나 loopback fixture 통과를 배포 결과로 쓰지 않는다.

| 확인 | 기대 결과 |
|---|---|
| `/`, `/ko`, `/guide`, leaderboard 및 상세 URL 직접 열기 | 자산 로딩, SPA fallback, 한·영 안내와 실제 endpoint 일치 |
| `/v1/auth/config` | JSON, GitHub/remote MCP OAuth 활성화 및 resource URL 일치 |
| `/v1/leaderboard` 등 실제 public API | JSON 응답. PostgreSQL/Redis 오류를 HTML fallback이나 liveness 성공으로 숨기지 않음 |
| 익명 MCP initialize → tools/list → status → recall → pinned expand | 로그인 없이 10개 도구와 공개 조회. 빈 catalog의 hits 0은 정상 응답이나 유용한 실제 recall의 증거는 아님 |
| 실제 GitHub 로그인과 host OAuth 승인 | callback·쿠키·CSRF·PKCE·scope·본인 rank/usage와 명시 동의 확인 |
| Host credential 해지/refresh 재사용 | 즉시 401/invalid_grant, 다른 사용자의 연결은 영향 없음 |
| 승인 화면 framing / HTTP 헤더 | iframe 거부와 위 HTTP 헤더. client-side frame guard만으로 서버 헤더를 대체하지 않음 |
| MCP JSON/SSE와 제한 초과 | stream flush/disconnect, anonymous private 401 + challenge, 429 + Retry-After, Redis outage 503 + retry 신호 |

공개 첫 연결의 최소 요청 예시:

```sh
WAM_RELEASE_SITE=https://wam.example
curl --fail-with-body --silent --show-error "$WAM_RELEASE_SITE/v1/auth/config"
curl --fail-with-body --silent --show-error "$WAM_RELEASE_SITE/mcp" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -H 'MCP-Protocol-Version: 2025-03-26' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"release-smoke","version":"1"}}}'
```

실제 host 연결·재인증·해지는 [고객 경로](../test/customer-paths.md), [remote OAuth](../test/remote-oauth.md), [host별 검사](../test/hosts/README.md)를 따른다. 비공개 계정 자료는 공개 증거에 넣지 않는다.

## 7. 복구와 중단 기준

배포 전 PostgreSQL backup을 **별도 DB에 실제 restore**하고 catalog·ledger·identity·outbox 정합성을 확인한다. Redis outage 시 새 비싼 요청/쓰기가 503으로 닫히고 복구 뒤 재시도가 가능한지 확인한다. write와 idle SSE 중 SIGTERM, lease 복구, deletion cleanup 재시도와 diagnostics flush는 [출시 복구 절차](../test/release-readiness.md#운영-재현-절차)를 따른다.

롤백은 이전 artifact와 호환 schema를 검증한 조합으로 수행한다. 새 migration을 적용한 DB에 이전 바이너리가 자동 호환된다고 가정하거나 migration 이력을 지우지 않는다. 필요하면 쓰기를 중단하고 검증한 별도 복원 DB로 전환한다. 전환 기간의 기여/receipt 손실 범위와 복구 시점을 기록한다. 로그·증거에는 원문 credential이나 connection string을 넣지 않는다.

실계정 인증 실패, privacy leak, 원장 중복/부분 commit, 복원 실패, 실제 model/replay 미구성은 해당 기능의 공개 출시를 중단할 사유다. 도메인·호스팅·계정·권리 결정이 없는 항목을 로컬 코드나 문서만으로 완료 처리하지 않는다.
