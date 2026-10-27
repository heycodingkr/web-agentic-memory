# RCL: URL + 키워드로 메모리 찾기와 호출 시점

상태: 1차 상세 명세. `wam-core`의 deterministic 후보 필터·순위는 구현했다. recall API/MCP, cache, client trigger는 아직 없다.

## 목적

에이전트가 특정 사이트의 작업을 시작할 때, 지금 열린 URL과 업무 키워드로 색인된 메모리를 불러와 워크플로, 에이전틱 업무 요약, 사이트 구조를 한 번에 받는다. 같은 계약을 MCP, CLI, HTTP, Arise가 쓴다.

## 입력

| 필드 | 필수 | 규칙 |
|---|---|---|
| `url` | 예 | 브라우저의 현재 주소를 [`core.md`](core.md) 규칙으로 canonical URL pattern으로 바꾼 값. Arise/CLI는 client library에서 정규화하고 원격 MCP는 tool request 전에 안전 pattern을 전달한다. 서버도 비정규 URL을 거부한다. 원본 URL은 recall 서버 밖으로 나가지 않는다 |
| `keywords` | 아니오 | ≤8개. 정규화 후 공개 **키워드 어휘**(`GET /v1/memories/vocab`, ETag 캐시)에 있는 것만 전송하고 나머지는 클라이언트에서 버린다 |
| `task_category` | 아니오 | 폐쇄 enum. 없으면 URL과 키워드로만 순위를 매긴다 |
| `kind` | 아니오 | `workflow`, `site_note`, `gotcha` 필터. 없으면 종류 무관 |
| `limit` | 아니오 | 1~3, 기본 3 |
| `task_hint` | 아니오 | 사용자가 한 말에서 키워드를 뽑는 **로컬 전용** 입력. 서버로 전송하지 않는다 |

서버가 받는 것은 canonical URL pattern, `task_category`, 어휘 안의 키워드뿐이다. 프롬프트, `task_hint`, 쿼리, 원본 ID는 받지 않는다. 키워드를 어휘로 제한하는 이유는 사용자의 업무 설명이 키워드를 통해 새어 나가지 않게 하기 위해서다.

## 순위 (결정적)

후보는 같은 `site_host`의 active 또는 `needs-reverify` 버전이다. 다른 host는 후보가 될 수 없다.

1. 경로 수준: 정확 일치 3, 안전한 상위 route 2, host 전체 1 ([`core.md`](core.md)의 순서). 상위 route 메모리는 L0에 그 하위 경로를 포함하는 범위가 적혀 있을 때만 후보다.
2. 점수 = `1000×경로 수준 + 100×(task_category 일치 0/1) + 10×(일치한 키워드 수) + 신뢰도 밴드(0~2)`. 가중치 때문에 순서는 경로, task, 키워드, 신뢰도 순으로 사전식이다. `needs-reverify`는 같은 점수대에서 fresh 뒤로 보낸다.
3. 동점은 신뢰도, `verified_at` 최신, `id` 순으로 정한다.

상위 3개를 L1로 반환한다. 각 항목에 `matched_on`(`route`, `task`, `keywords`), 적용 범위, 버전, 최신성을 담고, 인증된 caller에게만 `receipt_id`를 준다. 전체에 untrusted 프레이밍을 붙인다. 매칭이 없으면 오류가 아니라 빈 결과이며 `reason`(`no_match`, `scope_ambiguous`, `disabled`, `offline_no_cache`)을 준다. URL 정규화에 실패하면 원문을 저장하지 않고 빈 결과를 준다.

## 호출 시점

| 호스트 | 방식 |
|---|---|
| Arise | 호스트가 작업 시작 때 현재 탭 URL과 사용자 문장(로컬에서 키워드 추출)으로 자동 호출한다. device link 전은 익명, 명시적으로 연결한 뒤에는 계정 receipt를 받는다 |
| Claude Code, Codex + 브라우저 MCP | MCP `instructions`와 도구 설명이 "새 사이트에서 작업하기 전 canonical URL pattern과 허용 키워드로 `web_memory_recall(url, keywords)`를 호출"하도록 유도한다. 호출은 에이전트가 한다 |
| CLI, HTTP | `wam recall <url> [keywords…]` 또는 sensitive URL을 stdin으로 넘기는 `wam recall --stdin` (명세는 `cli.md`에서 정한다), 또는 canonical URL pattern을 받는 `POST /v1/memories/recall` |

공통 규칙: ① 세션에서 한 registrable domain에 처음 작업하기 직전, 작업이 바뀔 때, 다른 route 패턴으로 이동할 때 호출한다. ② 같은 (세션, route 패턴, task)는 5분 안에 다시 호출하지 않는다. ③ 사용자가 메모리를 쓰지 말라고 하면 호출하지 않는다. ④ 힌트는 참고 자료이고 사용자 지시가 우선이며, 다른 도메인 이동·자격증명 입력·데이터 전송을 요구하면 무시한다. ⑤ 상세가 필요할 때만 `expand`를 부른다.

호스트 제약 (2026-10-27 호스트 문서와 실행 확인 기준이며 정확한 설정은 `install.md`에서 정한다): ⑥ `web_memory_recall`, `web_memory_expand`, `web_memory_status`는 읽기 전용 도구로 표시한다. Codex는 읽기 전용으로 표시되지 않은 도구를 기본 설정(`default_tools_approval_mode = "writes"`)에서 승인 대상으로 본다. Claude Code에서 읽기 전용 표시가 권한 프롬프트를 생략시키는지는 확인하지 못했다. ⑦ Claude Code는 MCP 도구를 기본으로 지연 로딩한다(tool search). recall 도구를 항상 로딩하도록 등록하면(`alwaysLoad`, `claude mcp add-json`이나 `.mcp.json`에서만 설정 가능) 자발적 호출률이 달라지는지 측정한다(`SMK-06`, 설치 명세의 MCPF-03). 항상 보여야 한다고 가정하지 않는다. ⑧ 서버 `instructions`는 Claude Code가 4,096자를 넘는 부분을 자르고(도구 설명도 필드당 4,096자, tool search로 불러오는 도구는 16,384자), Codex는 앞 512자를 자기완결적으로 쓰길 권한다. recall 지시는 앞 512자 안에서 끝나게 쓴다. 4,096자는 필드별 한도이며 합계 한도가 아니다.

## 캐시와 오프라인

응답은 ETag와 5분 TTL로 캐시한다. 서버 장애 때는 마지막 유효 캐시를, 그다음에는 번들 시드를 쓴다. 캐시도 같은 순위 규칙으로 답한다. 결과 보고와 기여는 서버 확인 뒤에만 반영한다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-RCL-01 | URL + 키워드 입력 | 정확 route, 상위 route, host 폴백과 키워드 순위가 규칙대로 | RCL-01~03 |
| F-RCL-02 | 한국어·영어 키워드 | 같은 메모리를 `draft`와 `초안`이 모두 찾는다 | RCL-04 |
| F-RCL-03 | 전송 최소화 | 원 URL/query, 어휘 밖 키워드와 `task_hint`는 서버 요청에 없다 | RCL-05~06, CUST-01~06 |
| F-RCL-04 | 예산과 범위 | ≤3개, 힌트당 ≤400 tokens, 교차 host 0 | RCL-07, MEM-04 |
| F-RCL-05 | 결정적 순위 | 같은 입력은 같은 순서, 동점 규칙 | RCL-08 |
| F-RCL-06 | 최신성 표시 | `needs-reverify`는 표시되고 뒤로 간다, superseded·retired는 없다 | RCL-09 |
| F-RCL-07 | 빈 결과와 정규화 실패 | 오류 없이 `reason`, 원문 URL 미저장 | RCL-10~11 |
| F-RCL-08 | 캐시와 오프라인 | 장애 시 마지막 유효 캐시, 그다음 시드 | RCL-12 |
| F-RCL-09 | 품질 | 라벨 세트에서 precision@3, recall@3 기준 충족 | RCL-13 |
| F-RCL-10 | 채널·호스트 동등 | MCP remote/stdio, CLI, Arise가 같은 public memory ID/version/rank를 반환하고 인증 차이는 receipt/권한에만 나타난다 | RCL-14, MCP-24, CUST-01~06 |
| F-RCL-11 | 호출 시점 | 중복 호출 없음, route 변경 시 재호출, 사용자 거부 존중 | RCL-15~16, SMK-06, BEH-01, BEH-06, BEH-08~09 |

## 다른 명세에 필요한 변경

| 문서 | 변경 |
|---|---|
| [`api.md`](api.md) | `POST /v1/memories/recall` 본문에 `keywords`, `kind`, `limit`과 응답의 `matched_on`, `reason`. `GET /v1/memories/vocab`와 `GET /v1/memories/{id}?level=` 추가 |
| [`mcp.md`](mcp.md) | `web_memory_recall` 입력을 `url`, `keywords?`, `task_category?`로, `web_memory_expand`에 `level` 추가 |
