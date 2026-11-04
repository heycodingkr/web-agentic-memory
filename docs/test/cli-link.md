# CLI 기기 연결·호스트 등록 검증

2026-10-27 기준. CLI와 local stdio MCP는 같은 endpoint와 config namespace에 저장된 OS credential을 읽는다. 계정 연결은 사용자의 웹 승인 후에만 완료된다.

## 구현과 자동 검증

- `wam link [--no-browser] [--json]`: device start/poll, polling interval과 만료, 거절 처리. 명령 출력은 승인 URL과 user code, 승인 후 공개 device metadata만 포함한다. device secret과 access token은 출력하지 않는다.
- `wam unlink [--local-only] [--json]`: 기본은 서버에서 현재 token의 기기만 해지한 뒤 로컬 credential을 삭제한다. 네트워크 실패 시 재시도할 수 있도록 credential을 보존한다. `--local-only`는 서버 해지를 했다고 표시하지 않는다.
- OS secure store: macOS Keychain, Windows Credential Manager, Linux Secret Service. 평문 파일 fallback은 없다. endpoint와 절대 config 경로로 namespace를 분리한다. `WAM_CONFIG_DIR`, HOME, XDG/APPDATA를 존중한다. Secret Service를 사용할 수 없는 Linux에서는 익명 읽기만 가능하다.
- `wam doctor --json`: API reachability, saved device authentication, config namespace, cache availability를 구분한다. 만료/해지된 credential은 `reauth_required`다.
- `wam install codex|claude-code [--local] [--scope ...] [--mcp-bin ...] [--dry-run] [--json]`, `wam uninstall ...`: host CLI만 호출한다. 같은 설정은 멱등, 다른 `web-memory` 설정은 conflict이며 덮어쓰지 않는다. Claude는 user/local/project, 현재 Codex CLI 등록 도우미는 user scope를 지원한다.
- `guides/install-templates.json`은 CLI와 웹 가이드의 공통 템플릿이다. 기본은 anonymous remote HTTP 등록이며 local stdio에는 endpoint/config namespace만 넣는다. 등록은 실제 연결 또는 계정 인증 완료를 의미하지 않는다.
- 등록 제거는 해당 host entry만 지운다. 여러 host가 공유하는 계정과 cache는 `wam unlink`, `wam cache clear`로 별도 삭제한다.
- `wam recall --stdin --json`: URL을 argv에 넣지 않고 closed-schema JSON 객체 한 개를 받는다. 최대 16 KiB, 알 수 없는 필드/추가 JSON/잘못된 limit은 네트워크 호출 전에 종료 코드 2다. canonical URL만 서버에 전송한다. 429는 종료 코드 6이다.
- `wam guide print [--host codex|claude-code] [--write]`: 오프라인 출력, 현재 디렉터리의 AGENTS.md/CLAUDE.md WAM 마커 갱신. 실제 binary 6개 테스트로 파일 생성·멱등 교체, CRLF/외부 본문/권한 보존, malformed/duplicate/inline marker·symlink·oversize·UTF-8 오류·lock 거부를 검증했다.
- API endpoint는 HTTPS 또는 loopback HTTP만 허용하고 userinfo/query/fragment와 HTTP redirect는 거부한다. 익명 private command와 동의 없는 contribution은 client에서 요청 전에 차단한다.

추가 회귀는 `wam-client/tests/keyword_boundary.rs`의 실제 HTTP body 검사(원문 sentinel 차단, 어휘 실패 시 raw fallback 차단, 알 수 없는 task 거부, 동시 trace 분리), `wam-mcp/tests/stdio.rs`의 실제 프로세스 검사(취소 중 ping, frame 상한, legacy 응답), CLI `tests/input.rs`의 JSON 오류·cache 출력, `wam-server/tests/mcp.rs`의 HTTP protocol별 구조화 응답을 사용한다. 기본 기여 목록은 별도 queued/published fixture로 진행 이유와 정확한 반영 버전을 확인한다. `--json` 오류는 빈 stdout 대신 schema-versioned error 문서 하나를 반환하며 종료 코드와 stderr의 안전한 진단은 유지한다.

자동 회귀:

```sh
cargo test -p wam-cli -p wam-client
cargo test -p wam-server --test device_link
pnpm --dir web run test
```

`crates/wam-cli/tests/account.rs`는 승인·만료·저장 실패 rollback·오프라인 unlink를 injected store/provider로 검증한다. `tests/input.rs`는 실제 binary/stdin과 loopback HTTP request를 확인한다. `tests/install.rs`는 등록 멱등성, 충돌 보존, 제거, JSON dry-run을 검증한다. `wam-server/tests/device_link.rs`는 기기 자기 조회·해제와 읽기 scope의 쓰기 차단을 검증한다.

실제 unlocked OS 저장소의 일회용 항목 round-trip:

```sh
cargo test -p wam-client --test secure_store -- --ignored
```

macOS에서 실제 Keychain 저장·읽기·삭제를 통과했고 plaintext config 파일이 생성되지 않음을 확인했다. Windows/Linux native runtime은 이 환경에서 실행하지 않았다. 기본 suite는 실제 OS credential store를 건드리지 않는다.

## 호스트 등록 smoke

실제 Codex와 Claude CLI를 일회용 HOME, XDG, CODEX_HOME, CLAUDE_CONFIG_DIR에서 실행했다. local stdio 등록 → 동일 재등록 → 제거 → 재제거가 각 host에서 성공했고, 함께 등록한 다른 서버가 보존됐다. 모델 호출과 실제 계정 인증은 실행하지 않았다. Codex의 임시 CODEX_HOME은 실행 전에 디렉터리를 생성해야 한다.

명령은 설치된 CLI의 `mcp add/get/remove --help`와 [공식 OpenAI MCP 등록 안내](https://developers.openai.com/learn/docs-mcp)로 확인했다. 등록 템플릿은 host 업데이트 시 다시 검증한다.

최종 공유 트리 검증: Rust workspace 315 pass / 0 fail / 22 ignored, strict Clippy/rustfmt/diff check 통과. 웹 17 pass, Svelte check 0 errors/0 warnings, production build 성공. DB/Redis/provider 등의 ignored 항목은 이 CLI 실행의 증거에 포함하지 않는다.

## 남은 범위

2026-10-29 추가 검증: `status` 인증·동의·catalog 메타데이터, `expand --version-id`와 잘못된 버전 거부를 자동 회귀로 확인했다. 선택형 `wam browser-mcp`의 지속 child 연결은 실제 macOS Arise의 browser 도구 39개를 보존하며 현재 탭 recall 도구 1개를 추가했다. 실제 browser MCP가 발급한 소유 lease로 query/fragment 없는 요청이 local capture API에 도달했고 자체 테스트 탭만 닫았다. Codex와 Claude의 일회용 HOME에서 이 stdio bridge 등록·조회·삭제를 통과했다. 모델 호출이나 실제 계정 인증은 실행하지 않았다. 재현 명령과 제한은 [Arise 검증](hosts/arise.md)을 따른다.

표준 remote MCP OAuth discovery/DCR/PKCE/refresh/revoke 및 instruction snippet writer는 구현됐다. [Remote OAuth 검증](remote-oauth.md)은 합성 계정의 실제 Chrome 동의·해지/CORS와 격리 native Codex OAuth login/private rank·usage/logout을 포함한다. 실제 GitHub 웹 로그인과 device 승인부터 CLI/MCP의 private call까지 운영 계정 여정, 실제 GitHub OIDC 서명 릴리스 설치, Windows/Linux 및 Arise Hub remote OAuth acceptance는 별도다. `WAM_TOKEN`은 기존 local development override로 남아 있으며 link/install은 token을 환경변수나 config에 기록하지 않는다. `--anonymous` 또는 local MCP의 `WAM_ANONYMOUS=1`은 저장된 credential을 읽지 않는다.

전체 suite에서 관찰했던 cache 초기화의 간헐 실패 #48은 다른 세션이 timestamp 기반 임시 경로 충돌로 재현하고 random 경로로 수정해 resolved 처리했다. 현재 공유 트리의 전체 회귀가 통과한다.
