# CLI 형태 시나리오 (CLI)

상태: 시나리오 정의. 구현과 실행 결과는 없다. 명세 원본은 [`../../spec/cli.md`](../../spec/cli.md)이고 분류 체계는 [`README.md`](README.md)에 있다.
ID 접두사: `CLI` (CLI-01~33). 호스트 등록 도우미(`wam install`, `wam uninstall`)는 [`install.md`](install.md)가 다룬다.

## 전제

- 명령 표면, 종료 코드, 출력 계약은 [`../../spec/cli.md`](../../spec/cli.md) 2~4절의 초안을 따른다. 초안이 바뀌면 명세를 먼저 고치고 이 표를 따라 고친다.
- `wam recall <url> [keywords...]`는 MCP `web_memory_recall`과 같은 힌트를 같은 순서로 돌려준다. 키워드는 공개 어휘(`GET /v1/memories/vocab`)에 있는 것만 서버로 간다 ([`../../spec/recall.md`](../../spec/recall.md)).
- 종료 코드: 0 성공(빈 결과 포함), 1 예기치 않은 오류, 2 사용법 또는 입력 오류, 3 인증 필요, 4 권한 또는 동의 없음, 5 네트워크 또는 서버 불가, 6 호출 제한.
- 모든 실행은 일회용 HOME, XDG 아래에서 한다. 분류 칸은 `형태 · 호스트 · 계층 자동화 · 우선`이다 (`-`는 호스트 무관).

## 인증

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-01 | `wam link` 정상 | CLI · - · L4 반자동 · P0 | `user_code`와 승인 URL이 출력되고 `--no-browser`가 동작한다. 승인 뒤 토큰은 OS secure store에만 저장되고 출력과 로그에 토큰이 없다 | LINK-01~08, SMK-08 |
| CLI-02 | `wam link` 거부, 만료, Ctrl-C | CLI · - · L4 반자동 · P0 | 토큰이 만들어지지 않고 종료 코드가 구분되며 다시 시도할 수 있다. 서버에 대기 중인 device가 남지 않는다 | LINK-04, LINK-05 |
| CLI-03 | `wam unlink` | CLI · - · L4 자동 · P0 | 로컬 토큰이 지워지고 서버 device 해지가 요청된다. 이후 `propose`, `outcome`은 거부된다 | API-38, LINK-07 |

## 사용

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-04 | `wam --version`, `wam --help` | CLI · - · L4 자동 · P1 | spec/cli.md 2절의 모든 명령이 도움말에 있고 종료 코드는 0이다. 비밀값이 없다 | F-CLI-01 |
| CLI-05 | MCP 도구와 1:1 | CLI · - · L2 자동 · P0 | `status`, `recall`, `expand`, `propose`, `outcome`이 MCP 도구와 같은 입력·출력 스키마를 쓴다 (`--json`) | MCPF-09, F-CLI-02 |
| CLI-06 | 잘못된 인자, 알 수 없는 서브커맨드 | CLI · - · L4 자동 · P1 | 종료 코드 2와 사용법이 나온다 | F-CLI-01 |
| CLI-07 | 익명 `wam recall <url> [keywords...]` | CLI · - · L4 자동 · P0 | 메모리 L1 힌트가 3개 이하이고 각 항목에 `matched_on`, 범위, 버전, 최신성이 있다. receipt는 없고 untrusted 프레이밍이 붙으며 종료 코드는 0이다 | RCL-14, RCL-07 |
| CLI-08 | 모르는 사이트 | CLI · - · L4 자동 · P0 | 빈 결과와 `reason`(`no_match` 등)이 나오고 종료 코드는 0이다 (오류가 아님) | SMK-04, RCL-10 |
| CLI-09 | 키워드 어휘 | CLI · - · L4 자동 · P1 | 공개 어휘 밖의 키워드는 서버로 보내지 않고 로컬에서 버린 뒤 stderr로 알린다. 사용할 수 있는 어휘는 `wam vocab`(제안)으로 확인한다 | RCL-05, RCL-06 |
| CLI-10 | `wam expand <id> --level 1 또는 2` | CLI · - · L4 자동 · P1 | level 1은 L1, level 2는 L2 상세를 돌려주고 MCP `expand`와 같다. 허용되지 않는 level은 종료 코드 2다 | MEM-04, MEM-05 |
| CLI-11 | 익명 `propose`, `outcome` | CLI · - · L4 자동 · P0 | 종료 코드 3이고 네트워크 쓰기가 0건이다 | MCP-22, LINK-10 |
| CLI-12 | 입력 검증과 민감 URL의 stdin 입력 | CLI · - · L4 자동 · P0 | query/fragment가 붙은 URL은 positional 입력에서도 서버로 query가 나가지 않는다. `wam recall --stdin --json`은 allowlist schema와 크기 상한을 검증하고 원 URL·query를 process args, 로그, cache에 남기지 않는다. 잘못된 JSON/schema는 종료 코드 2이며 입력을 echo하지 않는다 | CORE-22~26, SEC-A, F-CLI-11 |
| CLI-13 | 로그인 뒤 recall, outcome | CLI · - · L4 반자동 · P0 | recall에 `receipt_id`가 붙고 `report:outcome` scope가 있는 같은 계정이 outcome을 보고할 수 있다. 다른 계정/만료 receipt는 404로 숨기며 점수는 바뀌지 않는다 | API-28~31, SCORE-20 |
| CLI-14 | 동의 off에서 `propose` | CLI · - · L4 자동 · P0 | 서버 요청이 0건이고 종료 코드는 4다 | SMK-07 |
| CLI-15 | 동의 on, 로그인 뒤 `propose` | CLI · - · L4 반자동 · P0 | 접수 ID와 상태가 나오고, 같은 nonce를 다시 보내도 결과가 같고 새 job이나 점수가 없다 | API-35 |
| CLI-16 | 금지 필드, 센티널 입력 | CLI · - · L5 자동 · P0 | 로컬 1차 redaction과 거부가 서버 G1과 같은 코드(`wam-core`)로 동작한다. 서버 sink에 센티널이 0건이다 | CORE-10~19, SEC-A |
| CLI-17 | 오프라인 | CLI · - · L4 자동 · P0 | recall은 마지막 유효 캐시, 번들 시드 순으로 답하고 둘 다 없으면 `offline_no_cache` 빈 결과(종료 코드 0)다. `propose`, `outcome`은 로컬 큐에 두고 서버 확인 전에는 점수가 없다 | SMK-11, RCL-12 |
| CLI-18 | 같은 입력의 반복 호출 | CLI · - · L4 자동 · P0 | 같은 URL과 키워드를 반복해도 점수 이벤트가 늘지 않는다 (로그인은 소비자-레코드-일 1회, 익명은 0) | SCORE-03, SCORE-15 |
| CLI-31 | outcome scope 또는 인증 누락 | CLI · - · L4 자동 · P0 | 익명·만료 token은 종료 코드 3, 인증은 됐지만 `report:outcome` scope가 없으면 코드 4다. 어떤 경우에도 원장 변경은 없다 | API-21, API-38, MCPF-26 |
| CLI-32 | outcome 중복·충돌 | CLI · - · L4 자동 · P0 | 같은 결과 재전송은 JSON `recorded:false` 또는 사람용 중복 안내를 내고 점수를 더하지 않는다. 다른 결과는 409/종료 코드 4이며 점수는 바뀌지 않는다 | API-27, SCORE-20 |
| CLI-33 | outcome 인자 검증 | CLI · - · L4 자동 · P0 | enum 밖 결과는 parse 오류, 128자를 넘는 receipt ID는 종료 코드 2로 거부하며 API 요청을 보내지 않는다 | API-26, MCPF-25 |

## 자동화 계약

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-19 | 비대화형 (TTY 없음) | CLI · - · L4 자동 · P0 | 프롬프트 없이 끝나고, 확인이 필요한 동작은 `--yes`가 없으면 종료 코드 2다 | F-CLI-06 |
| CLI-20 | 종료 코드 표 | CLI · - · L4 자동 · P0 | 0~6의 각 코드를 유발하는 케이스를 만들어 표와 일치하는지 확인한다 (빈 결과는 0, 익명 `propose`는 3, 동의 없음은 4, 서버 불가는 5, 429는 6) | spec/cli.md 3절 |
| CLI-21 | `--json` | CLI · - · L2 자동 · P0 | stdout에는 JSON 문서 하나만 나가고 진행과 로그는 stderr로 간다. `schema_version`이 있고 필드가 안정적이다 | F-CLI-06 |
| CLI-22 | 색과 TTY | CLI · - · L4 자동 · P2 | `NO_COLOR`가 설정됐거나 TTY가 아니면 ANSI 문자가 없다 | F-CLI-06 |
| CLI-23 | 에이전트가 셸 도구로 호출 | CLI · CC CX · L4 자동 · M | 민감할 수 있는 현재 URL은 `wam recall --stdin --json`으로 전달해 원 URL이 process args/로그에 없고, 출력 JSON을 파싱해 힌트를 untrusted로 보존한다. 안전한 fixture URL positional 호출과 같은 결과인지 확인한다. 호출률과 파싱 성공률을 기록한다 | SNIP-04, SNIP-05, RCL-14 |

## 수명주기

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-24 | 캐시 손상, 구 버전 캐시 | CLI · - · L4 자동 · P1 | 자동으로 복구되거나 명확한 오류가 나오고, 크래시하지 않는다 | F-CLI-07 |

## 진단·장애

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-25 | `wam doctor` 정상 | CLI · CC CX · L4 자동 · P0 | endpoint, TLS, 인증, 동의, 캐시, 호스트 등록(Claude Code, Codex)을 점검하고 사람용 출력과 `--json`이 모두 있다. 비밀값이 없다 | INST-36 |
| CLI-26 | `wam doctor` 실패 유형 | CLI · - · L4 자동 · P0 | DNS, TLS, 401, 토큰 만료, 캐시 없음, 호스트 미등록, 버전 불일치를 서로 구분해 원인과 다음 행동을 보여 준다 | INST-44 |
| CLI-27 | `wam status` | CLI · - · L4 자동 · P0 | `web_memory_status`와 같은 필드(host auth 상태, 동의, catalog revision, 설정 디렉터리)를 보여 준다. 웹 계정만 만들고 host를 연결하지 않은 사용자는 `anonymous`이며 이메일/내부 ID/token은 나오지 않는다 | MCP-19, CUST-02/04 |

## 보안

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| CLI-28 | 토큰과 센티널 비노출 | CLI · - · L5 자동 · P0 | 프로세스 인자, 환경변수, 로그, 크래시 출력에 토큰과 센티널이 없다 | SMK-14, SEC-H02 |
| CLI-29 | 파일 권한과 심볼릭 링크 | CLI · - · L5 자동 · P1 | 설정과 캐시 파일은 소유자만 접근할 수 있고, 심볼릭 링크를 따라가며 쓰지 않는다 | F-CLI-09 |
| CLI-30 | 헤드리스 Linux, 컨테이너 | CLI · - · L4 자동 · P1 | Secret Service가 없는 환경에서 토큰 저장이 정해진 대로 동작한다 (저장을 거부하고 안내하거나 명시적 대체). 평문 파일을 조용히 만들지 않는다. 결정은 열린 질문이다 | spec/cli.md 7절 |
