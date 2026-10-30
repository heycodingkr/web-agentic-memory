# 설치·등록·수명주기 시나리오 (INST)

상태: 시나리오 정의와 부분 로컬 구현·검증. Cargo 설치·격리된 host 등록/멱등성/제거는 검증됐고, live 계정·다른 OS·서명 artifact 수용 결과는 별도다. 실행 근거는 [출시 수용 기준](../release-readiness.md)과 host 문서를 따른다. 명세 원본은 [`../../spec/install.md`](../../spec/install.md)이고 분류 체계는 [`README.md`](README.md)에 있다.
ID 접두사: `INST` (INST-01~48). 호스트별 확인된 명령과 설정은 [`../hosts/claude-code.md`](../hosts/claude-code.md), [`../hosts/codex.md`](../hosts/codex.md)를 쓴다.

## 전제

- 서버 이름은 `web-memory`, endpoint는 배포 설정 값 `<MCP_ENDPOINT>`다. 로컬 CLI는 `wam`, stdio server는 별도 `wam-mcp` 바이너리다. `<API_BASE>`는 로컬 stdio가 호출할 API base이며 원격 endpoint의 `/mcp`와 구분한다.
- 모든 실행은 일회용 HOME, XDG, `CODEX_HOME`에서 한다. 사용자의 실제 설정, 프로필, 키를 건드리지 않는다.
- 설치 문자열은 웹 `/guide`, 문서, `wam install`이 쓰는 **같은 템플릿**에서 나오고, 테스트는 그 문자열을 가공 없이 실행한다 (INST-22).
- 분류 칸은 `형태 · 호스트 · 계층 자동화 · 우선`이다. 약어는 [`README.md`](README.md)에 있다.

## 설치 (바이너리 채널·플랫폼)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-01 | 설치 스크립트로 설치 (macOS arm64) | MCPL/CLI · - · L4 자동 · P0 | 일회용 HOME에서 sudo 없이 사용자 디렉터리에 설치되고 `wam --version`이 출력된다. 설치 경로와 PATH 안내가 나오고 종료 코드는 0이다 | SMK-01 |
| INST-02 | 설치 스크립트로 설치 (Linux x64) | MCPL/CLI · - · L4 자동 · P0 | INST-01과 같다 | SMK-01 |
| INST-03 | 설치 스크립트로 설치 (Windows PowerShell) | MCPL/CLI · - · L4 자동 · P0 | `wam.exe`가 사용자 경로에 설치되고 공백이 있는 사용자 경로에서도 동작한다 | SMK-01 |
| INST-04 | 릴리스 아카이브 수동 설치 | MCPL/CLI · - · L4 자동 · P1 | 체크섬 파일로 검증한 뒤 압축을 풀어 실행할 수 있다 | SMK-01 |
| INST-05 | `cargo install` | MCPL/CLI · - · L4 자동 · P1 | 소스 빌드 결과가 같은 버전, 같은 동작이다 (SMK-01~05 통과) | SMK-01~05 |
| INST-06 | 체크섬 불일치 | MCPL/CLI · - · L4 자동 · P0 | 설치가 중단되고 내려받은 파일이 삭제된다. 비밀값이 없는 오류를 출력하고 부분 설치가 남지 않는다 | F-INST-02 |
| INST-07 | 다운로드 실패 (오프라인, 프록시, DNS) | MCPL/CLI · - · L4 자동 · P1 | 재시도 안내가 나오고 이전 설치가 그대로 남는다 (원자적 교체) | F-INST-11 |
| INST-08 | 쓰기 불가 대상 디렉터리 | MCPL/CLI · - · L4 자동 · P1 | sudo를 요구하지 않고 대안 경로를 안내하며 부분 파일이 남지 않는다 | F-INST-02 |
| INST-09 | 같은 버전 재설치 | MCPL/CLI · - · L4 자동 · P1 | 멱등이다. 설정, 토큰, 캐시가 보존된다 | F-INST-02 |
| INST-10 | macOS 격리 속성 | MCPL/CLI · - · L4 수동 · P1 | 브라우저로 내려받은 아카이브(quarantine 속성)는 첫 실행이 막힐 수 있다. 서명·공증 또는 해제 방법 안내로 사용자가 막히지 않는다 | F-INST-12 |
| INST-11 | Linux 호환 | MCPL/CLI · - · L4 자동 · P1 | 최소 지원 배포판(목록은 열린 질문)에서 실행되고 링크 방식과 glibc 요구가 문서에 있다 | F-INST-12 |
| INST-12 | Windows 경로와 쉘 | MCPL/CLI · - · L4 자동 · P1 | 공백과 백슬래시가 있는 경로와 PowerShell에서 설치, 실행되고 호스트 등록에는 `wam.exe` 절대경로를 쓴다 | F-INST-12 |

## 등록 (호스트 등록·범위·설정 위생)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-13 | Claude Code 원격 한 줄 등록: `claude mcp add --transport http web-memory <MCP_ENDPOINT>` | MCPR · CC · L4 자동 · P0 | `Added ...`가 출력되고 `claude mcp get web-memory`가 서버를 보여 주며 연결된다 (익명 읽기 가능 상태). 사용자 설정은 변하지 않는다 | HOST-01, MCP-16, MCP-19 |
| INST-14 | Codex 원격 한 줄 등록: `codex mcp add web-memory --url <MCP_ENDPOINT>` | MCPR · CX · L4 자동 · P0 | `codex mcp list --json`에 나타나고 격리 `CODEX_HOME`의 `config.toml` `[mcp_servers.web-memory]`에 url만 기록된다 (토큰, 키 없음) | HOST-02, MCP-17 |
| INST-15 | Claude Code 로컬 stdio 등록: `wam --endpoint <API_BASE> install claude-code --local --scope user` | MCPL · CC · L4 자동 · P0 | 연결 상태가 되고 tools/list에 도구 10개가 나온다 | SMK-02 |
| INST-16 | Codex 로컬 stdio 등록: `wam --endpoint <API_BASE> install codex --local` | MCPL · CX · L4 자동 · P0 | 연결되고 tools/list에 도구 10개가 나온다 | SMK-02 |
| INST-17 | `wam install claude-code`, `wam install codex` (기본 원격, `--local`은 로컬) | CLI · CC CX · L4 자동 · P0 | 호스트 자신의 CLI를 호출해 INST-13~16과 **같은 설정**을 만든다 | F-INST-03 |
| INST-18 | `wam install` 멱등 | CLI · CC CX · L4 자동 · P0 | 같은 명령을 2번 실행해도 서버 항목은 1개이고 오류가 없다 | F-INST-03 |
| INST-19 | 기존 서버 보존 | CLI · CC CX · L4 자동 · P0 | 브라우저 MCP 같은 다른 서버가 그대로 남는다. 설정 diff는 우리 항목 추가 1건뿐이다 | HOST-03, HOST-04 |
| INST-20 | `--dry-run`, 호스트 CLI 없음 | CLI · CC CX · L4 자동 · P0 | 설정을 바꾸지 않고 실행할 명령을 출력한다. 호스트 CLI가 없을 때는 파일을 직접 고치지 않고 종료 코드로 구분한다 | F-INST-03 |
| INST-21 | 같은 이름, 다른 endpoint 충돌 | CLI · CC · L4 자동 · P1 | 기존 정의를 감지해 덮어쓰기 전에 멈추고 제거 방법을 안내한다 (Claude Code는 scope 간 충돌을 경고한다) | F-INST-03 |
| INST-22 | 단일 템플릿, 그대로 실행 | 전체 · CC CX · L3 자동 · P0 | 웹 `/guide`, 문서, `wam install`이 보여 주는 문자열이 호스트별 하나의 템플릿에서 생성되고, 테스트가 그 문자열을 가공 없이 실행해 통과한다 | MCP-16, MCP-17 |
| INST-23 | 경로당 명령 2개 이하 | 전체 · CC CX · L3 자동 · P0 | 원격 MCP는 등록 1개(익명 read, 회원 host auth는 별도), 로컬은 binary install 1개와 `wam install <host>` 1개, `/guide` CLI는 installer 1개와 첫 recall 1개다. 템플릿의 명령 수를 검사한다 | F-INST-04, INST-48 |
| INST-24 | 설정 위생 | 전체 · CC CX · L3 자동 · P0 | 복사 문자열에 앞뒤 공백과 줄바꿈이 없다. 서버 이름은 문자, 숫자, 하이픈, 밑줄만이고 호스트 예약 이름이 아니다. 비밀값이 없다 | MCP-16, SEC-H02 |
| INST-25 | 붙여넣기 뒤 경고 없음 | MCPR · CC · L4 자동 · P1 | 템플릿을 그대로 붙여넣은 뒤 `claude mcp list`에 공백, `${VAR}` 경고가 없다 | F-INST-04 |
| INST-26 | Claude Code scope | MCPR/MCPL · CC · L4 자동 · P1 | `--scope local`, `user`, `project` 각각 해당 위치에만 기록되고 다른 scope는 변하지 않는다. 가이드가 기본값(local)과 차이를 안내한다 | F-INST-05 |
| INST-27 | Claude Code 프로젝트 범위 `.mcp.json` | MCPR/MCPL · CC · L4 자동 · P1 | 승인 전에는 ⏸ Pending approval이고 연결되지 않는다. 우리 도구가 승인과 신뢰를 우회하지 않고 파일에 비밀이 없다 | F-INST-05 |
| INST-28 | Codex 사용자 설정과 프로젝트 설정 | MCPR/MCPL · CX · L4 자동 · P1 | 두 위치를 안내하고, 신뢰되지 않은 프로젝트의 `.codex/config.toml`은 적용되지 않는다는 점을 알린다 | F-INST-05 |

## 인증 (OAuth 호환·로그인 경로)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-29 | Claude Code `/mcp` 인증 | MCPR · CC · L4 반자동 · P0 | 사전 등록 없이 DCR(또는 CIMD)로 클라이언트가 등록되고, 호스트가 고른 임의 loopback 포트의 콜백을 받는다. 완료 뒤 `claude mcp list`가 연결 상태를 보여 준다 | HOST-01 |
| INST-30 | Codex `codex mcp login web-memory` (`--no-browser` 포함) | MCPR · CX · L4 반자동 · P0 | CIMD 또는 DCR로 등록되고 가변 포트 loopback redirect(RFC 8252 7.3)를 받는다. 응답의 `iss`가 일치하며, 어긋나면 거부한다 | HOST-02 |
| INST-31 | authorization server 메타데이터 | MCPR · GEN · L2 자동 · P0 | `.well-known` 문서와 보호 리소스 메타데이터가 유효해서 두 호스트와 스크립트 클라이언트가 수동 설정 없이 발견한다 | F-INST-06 |
| INST-32 | 헤드리스, 원격(SSH) 로그인 | MCPR/CLI · CC CX · L4 수동 · P1 | `codex mcp login --no-browser`, `wam link --no-browser`로 URL과 코드를 수동으로 다루어 완료한다. 토큰이 화면과 로그에 없다 | SMK-14 |
| INST-33 | `wam link` 경로 | MCPL/CLI · CC CX · L4 반자동 · P0 | OAuth를 지원하지 않는 호스트나 로컬 stdio에서 device flow로 연결하고, 토큰은 OS secure store에만 저장된다 | LINK-01~08, SMK-08 |

## 사용 (설치 직후 첫 사용)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-34 | 설치 직후 첫 recall (익명, 시드 fixture) | 전체 · CC CX · L4 자동 · M | 설치 시작부터 힌트 확인까지의 명령 수와 소요 시간을 기록한다. 목표값은 metrics.md에서 정한다 | SMK-03, F-INST-13 |
| INST-35 | 도구 노출 확인 | MCPR/MCPL · CC CX · L4 자동 · P0 | `claude mcp get`, `codex mcp list --json`과 세션의 tools/list에 도구 10개가 보인다 | MCP-19, SMK-02 |
| INST-36 | `wam doctor` 한 번 점검 | CLI · CC CX · L4 자동 · P0 | 연결, 인증 상태, 호스트 등록(Claude Code, Codex) 검출, 버전을 한 번에 확인한다. 비밀값은 출력하지 않는다 | CLI-25 |

## 수명주기 (업그레이드·제거)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-37 | 업그레이드 | MCPL/CLI · CC CX · L4 자동 · P0 | 구 버전에서 신 버전으로 바꾼 뒤 등록(절대경로), 토큰, 캐시가 유지된다. 도구 스키마가 바뀌면 사용자에게 알린다 | SMK-13 |
| INST-38 | 실행 중인 서버가 있을 때 업그레이드 | MCPL · CC CX · L4 자동 · P1 | 바이너리를 쓰고 있어 교체가 안 되는 경우(Windows 파일 잠금 등) 이전 버전이 유지되고 안내가 나온다 | F-INST-09 |
| INST-39 | Claude Code 제거 | MCPR/MCPL · CC · L4 자동 · P0 | `claude mcp remove web-memory`가 항목과 호스트가 저장한 OAuth 토큰, client registration을 지운다. 다른 서버는 보존되고 다시 등록하면 재로그인이 필요하다 | HOST-06, SMK-10 |
| INST-40 | Codex 제거 | MCPR/MCPL · CX · L4 자동 · P0 | `codex mcp logout web-memory` 뒤 `codex mcp remove web-memory`. 설정 diff는 우리 항목 삭제뿐이다 | HOST-06, SMK-10 |
| INST-41 | `wam uninstall <host>` | CLI · CC CX · L4 자동 · P0 | 호스트 등록, 로컬 토큰, 캐시를 지우고 서버 쪽 device 해지 방법을 안내한다. 사용자 원본 설정은 보존된다 | SMK-10 |
| INST-42 | 설치와 제거 3회 반복 | 전체 · CC CX · L4 자동 · P1 | 홈 디렉터리와 호스트 설정에 diff가 남지 않는다 (대칭) | HOST-06 |

## 진단·장애

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| INST-43 | 호스트가 `wam`을 찾지 못함 | MCPL · CC CX · L4 자동 · P0 | 등록에 절대경로를 써서 제한된 PATH(GUI 앱 등)에서도 실행된다. 실패하면 `wam doctor`와 가이드가 PATH 원인을 설명한다 | F-INST-11 |
| INST-44 | 잘못된 endpoint, DNS, TLS 오류 | MCPR · CC CX · L4 자동 · P0 | 호스트의 실패 상태와 `wam doctor`가 DNS, TLS, HTTP status를 구분한다. 원문 URL과 토큰은 노출하지 않는다 | mcp.md 실패와 복구 |
| INST-45 | 프록시, 사내 CA 환경 | MCPR/MCPL/CLI · - · L4 수동 · P2 | 지원 방식(환경변수)이 문서에 있고, 실패하면 원인 메시지가 나온다 | F-INST-11 |
| INST-46 | Codex 환경변수 필터링 | MCPL · CX · L4 자동 · P0 | stdio 서버가 설정 경로를 받지 못해도 `env`, `env_vars`에 명시한 값만 읽는다. `wam status`가 사용 중인 설정 디렉터리를 보여 주고, 어긋난 측정은 무효로 버린다 | host-smoke 1, 3절 |
| INST-47 | 시작 시간 | MCPL · CC CX · L4 자동 · P0 | `wam-mcp`의 콜드 스타트 initialize 시간을 Codex `startup_timeout_sec`(기본 10초), Claude Code 연결 대기(`alwaysLoad` 서버 5초)와 비교해 기록한다. 호스트 타임아웃을 넘으면 실패다 | SMK-02 |
| INST-48 | 비회원 `/guide` CLI 카드 복사와 설치 | CLI · CC CX · L3 자동 · P0 | 계정 로그인 없이 OS에 맞는 검증된 `wam` 설치 명령, checksum/버전 확인, 첫 익명 recall 예시를 볼 수 있다. 설치 명령 하나와 첫 `wam recall` 한 번으로 hint까지 확인한다. 복사 텍스트를 그대로 일회용 HOME에서 실행하고 installer가 설치 버전을 보고하는지 확인한다. 페이지는 copy만으로 설치 완료라 표시하지 않고, 명령·release hash·제거 안내는 같은 버전의 manifest에서 나온다 | F-INST-02/04/14, CUST-03 |
