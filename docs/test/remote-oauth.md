# Remote MCP OAuth acceptance

구현 대상: MCP 2025-11-25 authorization profile의 public DCR clients, authorization code + S256 PKCE, resource indicator, opaque access token, refresh rotation과 revocation. CIMD는 광고하지 않으며 DCR 경로를 제공한다.

기존 INST-29~33, MCPF-15~17, LINK-04~10의 자동 검증으로 다음을 확인한다.

- Protected resource / authorization server metadata가 configured canonical resource와 issuer를 사용한다. Host/Forwarded 헤더로 URL을 만들지 않는다.
- 인증이 필요한 MCP tool call은 HTTP 401 + WWW-Authenticate, 부족한 scope는 HTTP 403 + 최소 scope challenge다. Public recall/initialize는 익명으로 유지한다.
- DCR은 public client만 받고 HTTPS 또는 loopback callback만 등록한다. loopback port만 바꿀 수 있으며 host/path/query는 바꿀 수 없다.
- 승인 요청은 resource, client, redirect, state, S256 challenge에 묶인다. 사용자의 web session + CSRF + 명시 승인 없이 code/token을 발급하지 않는다.
- code는 짧은 수명과 단일 사용이며 client/redirect/resource/PKCE를 token transaction에서 검사한다. callback에 issuer를 포함한다.
- token/refresh/code/request secret은 hash로 저장하고 raw secret을 오류·로그·API metadata로 내보내지 않는다. public client refresh token은 매 사용마다 rotate하고 재사용은 전체 grant를 폐기한다.
- web device revoke, account deletion, token revocation은 같은 OAuth family의 refresh/access를 함께 무효화한다. restart와 shared application repository에서도 성립한다.
- OAuth registration/authorization/token 경로는 rate limits, bounded input/registry와 retention을 적용한다.
- 잘못된 JSON/form/query는 정적 OAuth error만 반환하며 입력을 echo하지 않는다. RFC 8707의 동일한 resource 반복은 허용하지만 다른 audience가 섞이거나 나머지 parameter가 중복되면 거부한다.
- 허용한 exact browser Origin에만 MCP preflight/CORS를 제공하며 cross-origin cookie 인증은 허용하지 않는다. 401의 WWW-Authenticate와 Retry-After를 client에 노출한다. 동의 UI는 iframe 안에서 승인할 수 없다.
- 인증된 MCP 사용량 조회는 HTTP와 같은 quota/history projection을 사용한다.

## 실행

서버 설정은 [wam-server README](../../crates/wam-server/README.md#remote-mcp-oauth)를 따른다. 아래 fixture는 합성 provider를 사용하는 loopback 전용 예제이며, 실제 계정이나 모델 API를 호출하지 않는다. 저장소 루트에서 API와 웹을 각각 별도 터미널로 실행한다. 기존 QA 서버를 재사용할 때는 다른 테스트를 동시에 실행해 같은 IP quota를 소진하지 않는다.

API:

```sh
export WAM_SMOKE_DATA_DIR="$(mktemp -d "${TMPDIR:-/tmp}/wam-oauth.XXXXXX")"
WAM_SMOKE_BIND=127.0.0.1:35055 \
WAM_SMOKE_WEB_ORIGIN=http://127.0.0.1:32055 \
WAM_SMOKE_MCP_RESOURCE=http://127.0.0.1:32055/mcp \
WAM_MCP_ALLOWED_ORIGINS=http://127.0.0.1:32055 \
WAM_SMOKE_KEY=wam-oauth-fixture-only-key \
cargo run -p wam-server --example smoke_fixture
```

웹:

```sh
VITE_WAM_API_TARGET=http://127.0.0.1:35055 pnpm --dir web exec vite dev --host 127.0.0.1 --port 32055 --strictPort
```

준비된 `http://127.0.0.1:32055/healthz`를 확인한 뒤:

```sh
cargo test -p wam-server --test oauth_flow --test mcp --test mcp_oauth
pnpm --dir web run test:oauth
python3 tests/env/check_remote_oauth_host.py
python3 tests/env/check_remote_oauth_host.py --explicit-resource
```

Chrome과 Codex CLI가 설치되어 있어야 한다. 기본 web smoke는 별도 fixture를 쓰는 `pnpm --dir web run test:smoke`이며 OAuth 전용 spec을 중복 실행하지 않는다. 429를 받으면 Retry-After 이후 재시도하거나 자기 fixture만 새로 시작한다.

## 확인한 증거 (2026-10-27)

- `oauth_flow.rs` 16개: durable repository restart, 저장 실패 rollback, PKCE/code/refresh replay, scope/audience/redirect 결합, CSRF/owner 확인, account/device revoke, quota와 sanitized errors. MCP 전용 테스트가 별도로 Origin·header·scope·CORS를 검증한다.
- quota middleware가 OAuth CORS exposure를 덮어쓰는 결합 회귀를 Rust 기본 suite에도 추가했다. Chrome cross-origin fetch는 route.fulfill callback 대신 실제 loopback 페이지에서 실행한다. 합성 callback 문서의 address-space 판정 때문에 생기는 Local Network Access 실패를 CORS 실패와 구분하며 브라우저 보안 검사를 끄지 않는다. [Chrome LNA 설명](https://developer.chrome.com/blog/local-network-access).
- 실제 Chrome의 동의 페이지: GitHub 대역 로그인 후 명시 승인, PKCE 교환, private read, OAuth 연결의 native rotate 숨김, device 해지와 refresh 실패, frame 승인 차단. APIRequestContext와 별도로 실제 브라우저 fetch가 CORS 및 WWW-Authenticate 노출을 검증한다.
- 설치된 native Codex: discovery → DCR → authorization/PKCE → credential 저장 → 발급 credential만으로 private rank/usage → logout. resource를 metadata에서 추론하는 설정과 명시 `oauth_resource` 설정을 각각 확인한다. 후자는 동일 resource를 반복하므로 서버가 RFC 8707을 처리해야 한다.
- native 검증은 임시 HOME/CODEX_HOME/XDG와 `mcp_oauth_credentials_store="file"`만 사용한다. token을 출력하지 않고 임시 디렉터리를 삭제한다. 실제 사용자 keychain/config는 사용하지 않으며 model calls=0, real accounts=0이다. [공식 Codex 설정](https://learn.chatgpt.com/docs/config-file/config-reference).

실제 GitHub 계정 승인, Claude/Arise remote OAuth, 실제 모델의 자발적 도구 호출, Windows/Linux native credential runtime과 운영 DB/Redis failover는 이 증거에 포함하지 않는다.

기준 원문: [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414), [RFC 7591](https://www.rfc-editor.org/rfc/rfc7591), [RFC 9207](https://www.rfc-editor.org/rfc/rfc9207), [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707).
