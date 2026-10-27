# CLI: `wam` 명령 표면과 자동화 계약

상태: 1차 상세 명세. `wam` 바이너리는 아직 없다. 명세 작성은 코드 동작을 검증했다는 뜻이 아니다.
영역 / 테스트 ID 접두사: `CLI` (시나리오 원본: [`../test/tool-forms/cli-form.md`](../test/tool-forms/cli-form.md))
기반 설계: [`../design/architecture.md`](../design/architecture.md) 3·4·8절, [`install.md`](install.md), [`recall.md`](recall.md), [`memory.md`](memory.md), [`link.md`](link.md)

## 1. 범위

`wam`은 사람이 직접 쓰거나 에이전트가 셸 도구로 호출하는 명령줄 도구이고, 로컬 stdio MCP 서버(`wam mcp`)와 호스트 등록 도우미를 포함한다. 이 명세는 명령 표면, 종료 코드, 출력 계약, 설정과 토큰 위치를 정한다. 설치 채널과 호스트 등록 요구는 [`install.md`](install.md), recall 입력과 순위는 [`recall.md`](recall.md)가 정한다.

## 2. 명령 표면 (초안)

MCP 도구와 1:1로 대응하는 명령은 같은 입력과 출력 스키마를 쓴다.

| 명령 | 역할 | 대응 도구 또는 API | 인증 |
|---|---|---|---|
| `wam recall <url> [keywords...]` 또는 `wam recall --stdin` | 승인된 메모리의 L1 힌트 최대 3개. 옵션 `--task <category>`, `--kind <종류>`, `--limit <1~3>`, `--json`. `--stdin`은 같은 입력 필드의 단일 JSON 객체를 stdin에서 읽는다 | `web_memory_recall`, `POST /v1/memories/recall` | 익명 가능. 로그인하면 receipt가 붙는다 |
| `wam expand <id> [--level <1 또는 2>] [--json]` | 메모리 L1 또는 L2 상세 | `web_memory_expand` | 익명 가능 |
| `wam outcome <receipt_id> <결과>` | 주입 뒤 결과 보고. 결과는 `success`, `partial`, `failed` | `web_memory_outcome` | 로그인 + receipt 소유 |
| `wam propose [--from <파일>]` | 정규화된 contribution 제출. 파일이 없으면 stdin | `web_memory_propose` | 로그인 + 동의 |
| `wam status [--json]` | endpoint, host auth 상태(`anonymous`, `linked`, `reauth_required`), 동의, 카탈로그 상태, 사용 중인 설정 디렉터리 | `web_memory_status` | 익명 가능 |
| `wam link [--no-browser]`, `wam unlink` | 기기 연결(device flow), 연결 해지 | `/v1/link/*` ([`link.md`](link.md)) | 웹 계정 승인 |
| `wam doctor [--json]` | 연결, TLS, 인증, 동의, 캐시, 호스트 등록 점검 | 없음 | 익명 가능 |
| `wam install <호스트> [--local] [--scope <범위>] [--dry-run]`, `wam uninstall <호스트>` | Claude Code(`claude-code`) 또는 Codex(`codex`)에 등록·제거 | 호스트 CLI 호출 ([`install.md`](install.md)) | 없음 |
| `wam guide print [--host <호스트>] [--write]` | instruction 스니펫 출력 또는 마커 블록 갱신 | 없음 | 없음 |
| `wam mcp` | 로컬 stdio MCP 서버 | MCP 도구 5개 | 토큰이 있으면 로그인 모드 |
| `wam vocab [--json]` (제안) | 공개 키워드 어휘 | `GET /v1/memories/vocab` | 익명 |
| `wam --version`, `wam --help` | 버전, 도움말 | 없음 | 없음 |

전역 옵션(초안): `--endpoint <url>`(기본은 배포 설정), `--json`, `--yes`(확인 프롬프트 생략).

규칙:

- `recall`은 [`recall.md`](recall.md)의 입력 규칙을 따른다. URL은 클라이언트가 canonical URL pattern으로 바꾼 뒤에만 쓰고, 키워드는 정규화 뒤 공개 어휘에 있는 것만 서버로 보낸다. 어휘 밖 키워드는 로컬에서 버리고 stderr에 알린다. 자유 텍스트 `task_hint`는 CLI 옵션이 아니며 서버로 나가지 않는다.
- URL에 query나 계정별 path가 포함될 수 있는 agent 호출은 `wam recall --stdin [--json]`을 쓴다. stdin은 `url`, `keywords`, `task_category`, `kind`, `limit`만 허용하는 JSON 하나이며 최대 입력 크기는 CORE 입력 상한을 따른다. 클라이언트는 서버 전송·캐시·로그 전에 URL을 정규화한다. stdin body, 원 URL, 사용자 프롬프트를 stdout/stderr, process args, telemetry에 복사하지 않는다. 단순한 안전 fixture URL을 직접 넘기는 positional 형식은 계속 지원한다.
- `wam recall`은 MCP `web_memory_recall`과 **같은 힌트를 같은 순서로** 반환한다 (RCL-14).
- `wam recall --stdin --json` 입력은 JSON object 하나로 제한한다. 허용 key는 `url`, `keywords`, `task_category`, `kind`, `limit`이며 `url`은 client에서 정규화해 서버로 보낸다. 예: `{"url":"<current URL>","keywords":["naver","blog","write"],"task_category":"compose-draft"}`. 잘못된 JSON, 알 수 없는 필드, 크기 초과는 종료 코드 2로 거부하고 body를 echo하지 않는다.
- `expand`의 level은 1 또는 2다 ([`memory.md`](memory.md)). 그 밖의 값은 종료 코드 2다.
- 이 표의 명령과 옵션은 구현 전에 합의하는 초안이다. 바뀌면 이 표를 먼저 고치고 시나리오를 따라 고친다.

## 3. 종료 코드 (초안)

| 코드 | 의미 | 예 |
|---|---|---|
| 0 | 성공 (빈 결과 포함) | 매칭 없는 recall, 캐시나 시드로 답한 오프라인 recall |
| 1 | 예기치 않은 오류 | 내부 오류 |
| 2 | 사용법 또는 입력 오류 | 알 수 없는 서브커맨드, 허용되지 않는 level, 정규화할 수 없는 URL |
| 3 | 인증 필요 | 익명으로 `propose`, `outcome`, 만료된 토큰 |
| 4 | 권한 또는 동의 없음 | 동의 off에서 `propose`, 다른 계정의 receipt |
| 5 | 네트워크 또는 서버 불가 | 서버 확인이 필요한 명령(`propose`, `outcome`, `link`, `doctor`의 서버 점검)에서 연결 실패 |
| 6 | 호출 제한 | 429 |

recall은 캐시와 시드로 답할 수 있으므로 서버가 불가해도 종료 0이다. 이때 응답의 `reason`(`offline_no_cache` 등)으로 구분한다.

## 4. 출력 계약

- 사람용 출력은 stdout, 진행과 진단은 stderr다. `--json`이면 stdout에는 JSON 문서 하나만 나가고 `schema_version`을 포함한다.
- recall 출력은 [`recall.md`](recall.md)의 응답을 그대로 담는다. 힌트는 최대 3개이고 각 항목에 `matched_on`, 적용 범위, 버전, 최신성이 있으며, 인증된 호출에만 `receipt_id`가 있다. 전체에 untrusted 프레이밍이 붙는다. 매칭이 없으면 오류가 아니라 빈 결과와 `reason`이다.
- 출력, 오류, 로그에 토큰, 키, 센티널, 원본 URL의 쿼리가 없다.
- `NO_COLOR`가 설정됐거나 TTY가 아니면 ANSI 색을 쓰지 않는다.
- TTY가 아니면 프롬프트를 띄우지 않는다. 확인이 필요한 동작은 `--yes`가 없으면 종료 코드 2로 끝난다.

## 5. 설정, 데이터, 토큰

- 설정과 캐시 위치(초안): macOS와 Linux는 `$XDG_CONFIG_HOME/wam`, `$XDG_CACHE_HOME/wam`(미설정이면 `~/.config/wam`, `~/.cache/wam`), Windows는 `%APPDATA%\wam`, `%LOCALAPPDATA%\wam`이다. HOME과 XDG 변수를 존중해서 일회용 격리 실행이 가능해야 한다.
- 토큰은 OS secure store(macOS Keychain, Windows Credential Manager, Linux Secret Service)에만 둔다. 설정 파일에는 endpoint와 동의 상태 같은 비밀이 아닌 값만 둔다. 토큰을 프로세스 인자, 환경변수, 로그, 평문 파일에 두지 않는다.
- 설정과 캐시 파일은 소유자만 읽고 쓴다. 심볼릭 링크를 따라가며 쓰지 않는다.
- recall 응답은 ETag와 5분 TTL로 디스크에 캐시한다. 서버가 불가하면 마지막 유효 캐시, 그다음 번들 시드로 답하고, 둘 다 없으면 `offline_no_cache` 빈 결과를 준다 ([`recall.md`](recall.md)). `propose`와 `outcome`은 서버 확인 전에 점수로 기록하지 않는다.

## 6. 요구사항

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| `F-CLI-01` | 명령 표면 | 2절의 명령이 도움말에 모두 나오고, 알 수 없는 입력은 종료 코드 2와 사용법을 준다 | CLI-04~06 |
| `F-CLI-02` | MCP 도구와 동등 | `status`, `recall`, `expand`, `propose`, `outcome`이 MCP 도구와 같은 입력·출력 스키마를 쓰고, `recall`은 같은 힌트를 같은 순서로 준다. 입력 검증은 서버 규칙과 같다 | CLI-05, CLI-07, CLI-09, CLI-10, CLI-12, MCPF-09, RCL-14 |
| `F-CLI-03` | 익명과 로그인 구분 | 익명은 recall, expand, status만 되고 receipt가 없다. `propose`, `outcome`은 종료 코드 3과 네트워크 쓰기 0건이다 | CLI-07, CLI-08, CLI-11, CLI-13 |
| `F-CLI-04` | 기기 연결 | `wam link`가 device flow로 연결하고 토큰을 OS secure store에만 저장한다. `unlink`는 로컬 토큰과 서버 device를 함께 지운다 | CLI-01~03 |
| `F-CLI-05` | 진단 | `doctor`와 `status`가 원인을 구분해 보여 주고 비밀값을 출력하지 않는다 | CLI-25~27, INST-36 |
| `F-CLI-06` | 자동화 계약 | 종료 코드 표, `--json`, 비대화형, 색 규칙을 지킨다 | CLI-19~22 |
| `F-CLI-07` | 오프라인과 캐시 | 캐시, 시드, `offline_no_cache` 순서로 답하고 손상된 캐시에서 복구한다 | CLI-17, CLI-24 |
| `F-CLI-08` | 동의와 기여 가드 | 동의 off면 서버 요청 0건, 동의 on이면 멱등 접수, 금지 필드는 서버와 같은 코드로 거부한다. 같은 입력의 반복 호출은 점수를 더하지 않는다 | CLI-14~16, CLI-18 |
| `F-CLI-09` | 비밀값과 파일 권한 | 토큰과 센티널이 인자, 환경, 로그, 크래시 출력에 없고 파일은 소유자만 접근한다 | CLI-28~30 |
| `F-CLI-10` | instruction 스니펫 | `wam guide print`가 [`recall.md`](recall.md)의 호출 시점 규칙을 담은 스니펫을 출력하고, `--write`는 마커 블록만 갱신한다 | SNIP-01~03 |
| `F-CLI-11` | 에이전트의 셸 호출 | 에이전트가 `wam recall --stdin --json` 또는 안전한 positional 호출로 recall하고 출력을 파싱한다. 민감 URL은 argv에 두지 않고 untrusted 표시를 보존한다 | CLI-12, CLI-23 |

## 7. 열린 질문

1. 명령 이름과 옵션(`wam vocab` 포함) 확정. 환경변수 이름(`WAM_ENDPOINT` 등)을 둘지.
2. 헤드리스 Linux나 컨테이너처럼 Secret Service가 없는 환경에서 토큰을 어디에 둘지. 평문 파일을 조용히 만들지 않는다는 원칙만 정했다 (CLI-30).
3. 자체 업데이트 명령(`wam update`)을 둘지. 설치 스크립트를 다시 실행하는 방식으로 충분한지.
4. `wam propose`가 받는 입력 형식. 정규화된 trace만 받는지, 원시 입력을 `wam-core`로 정규화해 주는지 ([`core.md`](core.md)와 합의 필요).
5. 단일 바이너리로 배포할지 ([`install.md`](install.md) 8절 1번).
