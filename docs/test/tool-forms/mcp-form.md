# MCP 형태 시나리오 (MCPF)

상태: 시나리오 정의. 구현과 실행 결과는 없다. 명세 원본은 [`../../spec/mcp.md`](../../spec/mcp.md), [`../../spec/recall.md`](../../spec/recall.md), [`../../spec/link.md`](../../spec/link.md)이고 분류 체계는 [`README.md`](README.md)에 있다.
ID 접두사: `MCPF` (MCPF-01~27). 원격(`MCPR`)과 로컬 stdio(`MCPL`) 두 transport를 모두 다룬다. 설치와 등록은 [`install.md`](install.md)가 다룬다.

## 전제

- 도구는 `web_memory_status`, `web_memory_recall`, `web_memory_expand`, `web_memory_leaderboard`, `web_memory_my_rank`, `web_memory_propose`, `web_memory_my_contributions`, `web_memory_withdraw`, `web_memory_outcome` 9개다. recall 입력과 순위는 [`../../spec/recall.md`](../../spec/recall.md)가 정하고, 이 표는 그 계약을 호스트에서 확인한다.
- 호스트는 Claude Code(`CC`), Codex CLI(`CX`), 범용 MCP 클라이언트(`GEN`, MCP Inspector 또는 스크립트)다. 확인된 호스트 사실은 [`../hosts/claude-code.md`](../hosts/claude-code.md), [`../hosts/codex.md`](../hosts/codex.md)에 있다.
- 분류 칸은 `형태 · 호스트 · 계층 자동화 · 우선`이다. 계층 L0~L6은 테스트 계층이며 메모리 레이어 L0~L2와 다르다.

## 사용 (도구 발견·권한 티어·호출)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-01 | initialize의 `instructions` | MCPR/MCPL · GEN · L2 자동 · P0 | 앞 512자가 자체 완결이고(Codex 권고), 필드 전체가 4,096자 이하다(Claude Code 한도). 핵심 지침(새 사이트 작업 전 recall, 사용자 지시 우선, 동의가 있을 때만 propose)이 앞쪽에 있다. 길이와 문장 위치를 검사한다 | recall.md 호출 시점 |
| MCPF-02 | tools/list | MCPR/MCPL · GEN · L2 자동 · P0 | 도구 9개, 각 도구 설명 4,096자 이하. status, recall, expand, leaderboard, my_rank, my_contributions는 읽기이고 propose, withdraw, outcome은 쓰기다 | MCP-19 |
| MCPF-03 | Claude Code 기본 지연 로드 대 `alwaysLoad: true` | MCPR · CC · L4 자동 · M | 사이트 작업 전에 에이전트가 recall을 호출하는 비율을 두 설정에서 기록한다 (합격 조건이 아니라 측정값). 낮으면 템플릿과 instructions를 고친다 | BEH-01, SMK-06, RCL-15 |
| MCPF-04 | Codex 승인 모드 `writes` | MCPR/MCPL · CX · L4 자동 · P1 | recall, expand, status는 승인 프롬프트 없이 실행되고 propose, outcome은 승인을 요구한다. 승인 이벤트 수로 판정한다 | MCPF-02 |
| MCPF-05 | 브라우저 MCP 공존 | MCPR/MCPL · CC CX · L4 반자동 · P0 | 메모리 MCP와 Chrome 브라우저 MCP가 같은 세션에서 모두 보이고 이름 충돌이 없다. 한쪽 장애가 다른 쪽에 영향을 주지 않는다. fixture 저장 oracle로 확인한다 | HOST-03, HOST-04, MCP-20 |
| MCPF-06 | 익명 연결 | MCPR/MCPL · CC CX · L4 자동 · P0 | recall, expand, status는 성공하고 receipt가 없다. propose, outcome은 권한 오류이며 점수 원장은 변하지 않는다 | MCP-22, API-30, SCORE-15 |
| MCPF-07 | 로그인 연결 | MCPR/MCPL · CC CX · L4 반자동 · P0 | recall 응답에 receipt가 붙고, propose는 동의가 있어야 하며, outcome은 `report:outcome` scope가 있는 receipt 소유 계정만 보고할 수 있다 | MCP-23, API-31, ACC-MCP-03 |
| MCPF-08 | recall 출력 크기 | MCPR/MCPL · GEN · L2 자동 · P0 | 힌트는 3개 이하, 힌트당 400 토큰 이하(메모리 L1)다. Claude Code 경고 임계(10,000 토큰)와 Codex `output_token_limit`보다 한참 작다 | RCL-07, metrics.md |
| MCPF-09 | CLI와의 동등성 | MCPR/MCPL/CLI · GEN · L2 자동 · P0 | 같은 입력에서 MCP recall과 `wam recall`이 같은 L1 힌트를 같은 순서로 돌려준다 | RCL-14, CLI-07 |
| MCPF-10 | untrusted 프레이밍과 인젝션 시드 | MCPR/MCPL · CC CX · L4 자동 · P0 | 힌트가 구조적으로 "참고용"과 분리되어 있고, 악성 지시 시드를 에이전트가 따르지 않는다. 트레이스에 도메인 이동, 데이터 전송, 자격증명 입력이 0건이다 | BEH-04, SEC-B |
| MCPF-11 | 같은 입력의 연속 recall | MCPR/MCPL · CC CX · L4 자동 · P0 | 같은 URL과 키워드를 연속으로 호출해도 점수 이벤트가 늘지 않는다 (로그인은 소비자-레코드-일 1회, 익명은 0) | SCORE-03, SCORE-15 |
| MCPF-12 | 동의 off에서 propose | MCPR/MCPL · CC CX · L4 자동 · P0 | 서버 요청이 0건이고 에이전트가 사용자에게 알린다 | SMK-07, BEH-07 |
| MCPF-13 | outcome 재전송, 타인 영수증, 만료 | MCPR/MCPL · CC CX · L4 자동 · P0 | 같은 결과 재전송은 `recorded:false`, 다른 결과는 409, 타인·만료 receipt는 404이며 점수 event가 추가되지 않는다 | SCORE-20, API-27~29 |
| MCPF-14 | 응답 지연과 타임아웃 | MCPR/MCPL · CC CX · L4 자동 · P1 | 서버 응답이 늦으면 호스트 타임아웃(Codex `tool_timeout_sec` 기본 60초)에서 도구 오류가 나고 상태와 점수는 변하지 않는다. 재시도는 idempotency key로 중복이 없다 | API-35 |
| MCPF-25 | outcome의 잘못된 입력 | MCPR/MCPL · GEN · L2 자동 · P0 | 누락/빈/128자 초과 receipt, 알 수 없는 필드, enum 밖 outcome을 거부하고 API 호출 및 원장 변경이 없다 | API-26~29 |
| MCPF-26 | outcome scope 누락 | MCPR/MCPL · GEN · L2 자동 · P0 | 유효한 로그인 token에 `report:outcome`이 없으면 MCP 도구 오류, direct CLI/API는 403이며 receipt/outcome/score 저장은 0건이다 | API-21, API-38, SEC-B |
| MCPF-27 | browser session CSRF 실패 | MCPR · GEN · L2 자동 · P0 | cookie session에서 CSRF token이 빠지거나 유효하지 않으면 HTTP 403이며 receipt/outcome/score 저장은 0건이다 | SEC-B |

## 인증 (티어·토큰 수명)

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-15 | 한 endpoint의 익명 읽기와 인증 쓰기 | MCPR · CC CX · L4 반자동 · P0 | 연결 시 로그인 없이 recall이 가능해야 한다. propose/outcome에서 host OAuth를 시작하거나, 명시적인 `codex mcp login`/Claude 인증 단계가 동작해야 한다. 해당 호스트가 remote OAuth를 지원하지 않으면 `/guide`에서 다른 경로인 local stdio MCP + `wam link`로 전환하게 한다. 선택 경로와 연결 상태를 host별로 기록한다. 연결이 안 된 상태를 성공으로 표시하면 실패다 | CUST-01~02, LINK-09~11 |
| MCPF-16 | 거절, 취소, 만료, 갱신 | MCPR · CC CX · L4 반자동 · P0 | 거절이나 취소 뒤에는 인증 필요 상태로 돌아오고 익명 recall은 계속되며 propose, outcome은 401/403이다. 만료 때 갱신이 되면 끊김이 없고, 실패하면 재인증을 안내한다 | API-38, SCORE-16 |
| MCPF-17 | 로그아웃과 해지 | MCPR/MCPL · CC CX · L4 반자동 · P0 | Claude Code의 인증 해제, `codex mcp logout`, 웹 `/settings/devices` 해지 뒤 다음 쓰기 요청이 즉시 거부된다. 익명 읽기는 계속된다 | LINK-07, API-38 |

## 수명주기

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-18 | 호스트 재시작과 세션 재개 | MCPR/MCPL · CC CX · L4 반자동 · P1 | 재로그인 없이 연결이 유지되고, 서버 도구가 바뀐 뒤에는 도구 목록이 갱신된다 (Claude Code 도구 목록 캐시 포함) | SMK-13 |
| MCPF-19 | 종료 신호 | MCPL · CC · L4 자동 · P0 | macOS와 Linux에서 SIGINT, 100ms 뒤 SIGTERM, 400ms 뒤 SIGKILL 시퀀스 안에 정리하고 종료한다. Windows는 stdin이 닫힌 뒤 300ms 안에 종료한다. 캐시 파일이 손상되지 않는다 | claude-code.md |
| MCPF-20 | 로컬 캐시와 오프라인 | MCPL · CC CX · L4 자동 · P0 | eligible anonymous result만 bounded cache fallback; authenticated recall and write/outcome require server; no bundle-seed assumption; cache age is visible | SMK-11, RCL-12, CACHE-01~11 |

## 진단·장애

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-21 | 서버 장애와 네트워크 단절 | MCPR · CC CX · L4 자동 · P0 | 호스트가 실패 상태를 표시하고 에이전트는 recall 없이 작업을 이어 간다. 오류가 원인(DNS, TLS, HTTP status)을 구분하되 URL과 토큰을 노출하지 않는다 | mcp.md 실패와 복구 |

## 보안

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-22 | 비밀값 유출 점검 | MCPR/MCPL · CC CX · L5 자동 · P0 | `claude mcp get`, `claude mcp list`, `codex mcp list --json`, 호스트 로그, 프로세스 인자, 서버 로그에 토큰과 센티널이 없다 | SMK-14, SEC-A, SEC-H02 |

## 프로토콜 준수

| ID | 시나리오 | 분류 | 기대와 판정 근거 | 추적 |
|---|---|---|---|---|
| MCPF-23 | stdio stdout 순수성 | MCPL · GEN · L2 자동 · P0 | `wam mcp`가 stdout에 JSON-RPC 메시지 외에는 아무것도 쓰지 않는다. 로그는 stderr로 나간다 | F-INST-08 |
| MCPF-24 | Streamable HTTP, stdio conformance | MCPR/MCPL · GEN · L2 자동 · P0 | MCP Inspector나 스크립트 클라이언트로 initialize, tools/list, tools/call이 규약대로 동작한다. 원격과 로컬의 도구 계약이 같다 (같은 입력, 같은 출력 스키마와 오류 코드) | MCP-01~15 |
