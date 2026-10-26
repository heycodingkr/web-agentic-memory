# Memory Gatekeeper 설계 — 공유 웹 에이전틱 메모리의 검열·중복제거·갱신 파이프라인

상태: 설계 초안 (구현 전). 기존 구현(`docs/specs/web-agentic-memory.md`)을 확장하는 문서다.
범위: 설계만. 코드는 포함하지 않는다.

## 1. 목표와 비목표

### 목표
1. 사람들이 특정 사이트에서 한 에이전트 업무(워크플로)를 공유하면, 다른 사람의 에이전트가 이를 주입받아 쓴다.
2. 공유 전에 **개인정보·비밀값(API 키, 토큰, 이메일, 계정 식별자 등)을 반드시 제거**한다.
3. **기존 메모리와 다른(새로운) 것만** 적재하고, 같은 것은 합치거나 버린다. 개선된 것은 기존 항목을 갱신한다.
4. AI가 큐레이션을 수행하되, AI 판단 하나만으로 공개하지 않는다 (결정적 검사 + 실측 검증 + 승인 게이트).
5. **호스트 중립**: Arise 브라우저뿐 아니라 Chrome 등 다른 브라우저 에이전트에서도 MCP 설치만으로 동작한다.

### 비목표
- 사용자의 개인 메모리(로컬 SQLite) 공유. 개인 메모리는 계속 로컬 전용이다.
- 사이트별 계정 데이터, 문서 내용, 대화 내용의 수집.
- 서버가 제출된 명령을 실행하는 것. (기존 원칙 유지)

## 2. 용어

| 용어 | 의미 |
|---|---|
| Trace | 한 번의 에이전트 작업에서 관측된 도구/결과 카테고리 시퀀스 (원문 없음) |
| Contribution | 사용자가 공유에 동의해 제출한 trace 또는 사용자가 검토한 힌트 초안 |
| Candidate | 게이트키퍼를 통과해 정제된 워크플로 후보 |
| Memory Record | 승인되어 카탈로그에 게시된 사이트별 워크플로 항목 (`web:<slug>`) |
| Gatekeeper | Contribution을 Candidate로, Candidate를 Record로 바꾸는 단계별 파이프라인 |

## 3. 전체 구조

```
[호스트 에이전트] ─MCP─▶ [Memory MCP Server] ──HTTPS──▶ [Gatekeeper Service]
 (Arise / Chrome 등)      recall / expand / propose        ├ G0 수신·스키마 검증
                          │ 1차 로컬 redaction            ├ G1 결정적 redaction (2차)
                          └ 로컬 캐시                     ├ G2 LLM sanitizer (3차, 보조)
                                                         ├ G3 신규성/중복 판정
                                                         ├ G4 재현 검증 (A/B replay)
                                                         ├ G5 게시 게이트 (승인)
                                                         └ G6 릴리스·롤백·감시
                                                                │
                                                       [Catalog API: 익명 GET]
```

핵심 원칙: **방어는 겹겹이(defense in depth)**. 어느 한 단계가 뚫려도 다음 단계가 잡는다. 클라이언트 단계는 신뢰하지 않는다 (서버가 항상 다시 검사한다).

## 4. 호스트 중립성 (Arise / Chrome / 그 외)

### 4.1 결론
가능하다. 메모리 계층을 "브라우저 구현"이 아니라 **MCP 서버 + 호스트 중립 스키마**로 분리하면 된다.
메모리는 hostname과 정규화된 액션 카테고리에만 의존하고, 특정 브라우저 API에 의존하지 않기 때문이다.

### 4.2 구성

| 계층 | 역할 | 호스트 의존성 |
|---|---|---|
| Memory MCP Server (stdio + remote HTTP) | `recall`, `expand`, `propose` 도구 제공, 로컬 캐시, 1차 redaction | 없음 (표준 MCP) |
| Host Adapter | 호스트의 도구 호출을 정규화 액션으로 매핑 | 호스트별 얇은 매핑 표 |
| Gatekeeper Service | 검열·중복제거·검증·게시 | 없음 |

### 4.3 호스트별 연결 방식

| 호스트 | 설치 | 힌트 주입 | trace 수집 |
|---|---|---|---|
| Arise 브라우저 | 내장 | 자동 주입 (기존 경로) | 내장 관측 (기존) |
| Chrome + 브라우저 MCP (Playwright MCP, Chrome DevTools MCP 등) | Memory MCP를 같은 에이전트에 추가 | MCP `instructions`와 도구 설명으로 "사이트 작업 전 `recall(hostname)` 호출"을 유도. 자동 주입이 아니라 에이전트가 호출 | 에이전트가 작업 후 `propose`를 호출하거나, Adapter가 도구 호출 로그를 정규화 |
| Chrome 확장 (선택) | 확장 설치 | 확장이 현재 hostname을 MCP에 전달 | 확장이 탐색/액션 카테고리 관측 |
| 기타 MCP 클라이언트 | MCP 서버만 추가 | recall 수동 호출 | `propose` 수동 호출 |

주의: 호스트가 자동 주입을 지원하지 않으면 효과는 "에이전트가 recall을 호출하느냐"에 좌우된다. 이 차이는 벤치마크에서 호스트별로 분리해 보고한다 (Arise 자동 주입 vs 범용 수동 recall). 기존 측정에서도 수동 recall이 일부 조건에서 더 빨랐으므로, 수동 방식이 불리하다고 가정하지 않는다.

### 4.4 정규화 액션 분류 (Host-neutral Action Taxonomy)

호스트마다 도구 이름이 다르므로 공통 카테고리로 매핑한다. 예:

| 정규화 카테고리 | Arise 예 | 일반 브라우저 MCP 예 |
|---|---|---|
| `navigate` | `browser_navigate` | `navigate`, `goto` |
| `wait` | `browser_wait_for` | `wait_for` |
| `click` | native click | `click` |
| `type` | `browser_type` | `type`, `fill` |
| `key` | key press | `press_key` |
| `snapshot` | snapshot/screenshot | `snapshot`, `screenshot` |
| `scroll`, `select`, `upload`, `dialog` | 각 도구 | 각 도구 |

매핑 표는 Adapter 설정 파일로 두고, 알 수 없는 도구는 `other`로 처리한다. (기존 메모에 있듯 실제 식별자는 `browser_navigate`/`browser_wait_for`다. 이름을 추측하지 말고 호스트별로 실측해서 채운다.)

## 5. 데이터 모델

### 5.1 Contribution (입력, 폐쇄 스키마)

자유 텍스트를 받지 않는 것이 기본이다.

```
Contribution v1
- schema_version
- host_kind            (arise | chrome-mcp | other)   # 통계용
- site_host            (등록 가능한 도메인으로 정규화된 hostname)
- task_category        (closed enum: compose-draft, save-draft, search, login-flow, upload, ...)
- action_sequence[]    (정규화 카테고리 + 결과 카테고리 + 소요 구간 버킷)
- outcome              (success | partial | failed) + error_category
- observed_traits[]    (closed enum: iframe-editor, resume-dialog, offscreen-save, nbsp-text, ...)
- client_nonce / 기여자 익명 ID (일일 회전)
```

수집하지 않는 것: 프롬프트, 입력한 텍스트, 셀렉터 문자열, 페이지 HTML/스크린샷, 전체 URL(경로/쿼리), 쿠키, 계정 식별자, 도구 인자/결과 원문.

### 5.2 자유 텍스트 힌트 제출 (별도 흐름, 선택)

사용자가 "이 사이트는 이렇게 하면 돼요" 같은 문장을 직접 공유하려는 경우에만 사용한다.
- 제출 전 **사용자에게 redaction 결과 diff를 보여주고 승인**받는다.
- 길이 상한(예: 600자), URL 금지(도메인 외), 명령형 문장 템플릿 제한.
- 기본 비활성. 켜도 G1~G5를 모두 통과해야 한다.

### 5.3 Memory Record (게시물)

```
MemoryRecord
- id                web:<slug>
- site_host, task_category
- steps[]           (정규화 액션 + 조건부 힌트: "이어쓰기 팝업이 있으면 Cancel")
- pitfalls[]        (closed enum + 짧은 고정 템플릿 문장)
- evidence          (replay 결과 요약, 표본 수, 버전, 측정일)
- fingerprint       (정규화 후 해시)
- confidence        (표본 수 + 재현 성공률 기반)
- status            (candidate | approved | superseded | revoked)
- version, supersedes, created_at, last_verified_at
- ttl / decay_at
```

주입용 L0/L1 요약은 도메인당 최대 3개, 400 estimated tokens 이하 (기존 제약 유지).

## 6. Gatekeeper 단계

### G0. 수신·스키마 검증
- 알려진 `schema_version`, 필드 크기 상한(기존: 2048 byte/건), enum 외 값 거부.
- 호스트 정규화 (소문자, 포트/경로/쿼리 제거, public suffix 기준 등록 도메인 확인).
- 속도 제한, 기여자 ID별 일일 상한.
- 이 단계에서 거부된 입력은 **저장하지 않고** 카운터만 올린다.

### G1. 결정적 redaction (필수, AI 없음)
폐쇄 스키마라서 대부분 필요 없지만, 자유 텍스트/예기치 않은 필드가 올 때를 대비한 안전망이다.

| 대상 | 방법 |
|---|---|
| API 키/토큰 | 알려진 포맷 정규식 (cloud/provider 키 접두사, JWT, `Bearer`, private key 블록 등) + 높은 엔트로피 문자열 탐지 |
| 개인정보 | 이메일, 전화번호, 주민/신분증 형식, 카드번호(Luhn), IP, 주소 패턴 |
| 계정/식별자 | `@handle`, 사용자명 포함 경로, UUID/긴 숫자 ID |
| URL | 호스트 외 전부 제거 (경로, 쿼리, 프래그먼트 포함) |
| 로컬 경로/호스트명 | `/Users/...`, `C:\Users\...`, 내부 IP/호스트 |

정책: **탐지되면 마스킹이 아니라 해당 필드/항목 폐기**를 기본으로 한다. 부분 마스킹은 잔여 정보가 남기 쉽다. 허용 목록(allow-list) 방식으로, 알려진 안전 토큰(enum 값, 정규화 카테고리)만 통과시킨다.

### G2. LLM sanitizer (보조, 단독 사용 금지)
- 역할: 결정적 규칙이 놓친 맥락 의존 개인정보(이름, 회사명, 문서 제목 등) 후보를 **표시**한다.
- 입력은 이미 G1을 통과한 데이터뿐이다.
- 출력은 "위험 표시"만이고, LLM이 내용을 새로 써서 통과시키지 않는다. 표시된 항목은 폐기하거나 사람 검토로 넘긴다.
- 프롬프트 인젝션 방어: 제출물은 항상 데이터로 취급하고, 시스템 지시와 분리해서 전달한다. 도구 권한은 주지 않는다.

### G3. 신규성·중복 판정 ("기존과 다른 것만 적재")

목표: 같은 지식이 중복 적재되지 않게 하고, 더 나은 지식은 갱신한다.

1. **정규화**: `(site_host, task_category)` 키 아래에서 `action_sequence`를 표준형으로 변환 (연속 중복 제거, 관측용 `snapshot`/`wait` 압축, 순서 의미 있는 것만 유지).
2. **정확 중복**: 정규화 후 `fingerprint` 동일 → 기존 Record의 `support_count`만 +1, 새 레코드 생성 안 함.
3. **유사 중복**: 시퀀스 편집거리/집합 유사도(필요 시 임베딩)가 임계값 이상 → 기존 Record와 병합 후보로 표시.
4. **신규 판정 기준** (하나라도 만족해야 Candidate 생성):
   - 기존에 없는 `observed_trait`/`pitfall`에 대한 해결 경로
   - 기존 대비 더 짧은 성공 시퀀스 (정확도 동일 이상)
   - 기존 Record가 실패하는 조건(사이트 UI 변경)에서의 성공 경로
5. **결과 처리**

| 판정 | 처리 |
|---|---|
| 동일 | 카운트만 증가, 폐기 |
| 유사·열등 | 폐기 (카운트 증가) |
| 유사·개선 | 기존 Record의 개정안 Candidate (`supersedes` 지정) |
| 신규 | 새 Candidate |
| 충돌 (기존과 반대 지시) | 격리하고 G4 재현 검증에서 우열 판정 |

### G4. 재현 검증 (실측 A/B)
- 격리 프로파일에서 같은 모델/effort/태스크로 `tools-only` vs `injected`를 비교한다 (기존 `probe_agent_memory_live.py` 방식, fresh state).
- 게시 조건 예: 성공률이 기존 이상이고, 호출 수 또는 시간이 개선되며, 오류가 늘지 않을 것. 표본 수와 반복 횟수를 증거에 기록한다.
- 로그인/결제 등 위험 작업은 QA 계정/가짜 fixture에서만 실행하고, 발행/결제 같은 비가역 행동은 금지한다.
- **개선이 측정되지 않으면 폐기**한다. "그럴듯하다"는 게시 사유가 아니다.
- 한 번의 파일럿 차이를 일반화하지 않는다는 기존 원칙을 따른다 (반복 측정).

### G5. 게시 게이트
- 자동 승인 조건: G1 무검출, G2 무표시, G4 기준 충족, 독립 기여자 N명 이상(예: 3명)이 유사 경로를 관측 (Sybil 방지).
- 초기 단계는 **관리자 승인 필수**. 신뢰 지표가 쌓이면 저위험 사이트/카테고리부터 자동 승인.
- 승인 후 불변 버전으로 게시 (기존 candidate → attestation → publish → versioned release 흐름).

### G6. 릴리스·감시·롤백
- 클라이언트가 보고하는 익명 결과(주입 후 성공/실패 카테고리)를 집계해 **게시 후 성능 저하**를 감시한다.
- 임계 이하로 떨어지면 자동으로 `needs-reverify` 상태로 전환하고, 재검증 실패 시 이전 버전으로 롤백하거나 revoke한다.
- 오래된 항목은 `ttl/decay`로 재검증 대상이 된다 (사이트 UI는 바뀐다).

## 7. 위협 모델과 대응

| 위협 | 대응 |
|---|---|
| 비밀값/개인정보 유출 | 폐쇄 스키마, 클라이언트 1차 + 서버 G1 결정적 규칙 + G2 보조, 탐지 시 폐기, 원문 미저장 |
| 메모리 포이즌 (악의적 힌트) | 독립 기여자 N명 요구, G4 실측 검증, 관리자 게이트, 도메인 간 주입 금지 |
| 프롬프트 인젝션 (힌트 안에 악성 지시) | 구조화 필드와 고정 템플릿만 허용, 자유 텍스트 기본 비활성, 주입 시 untrusted framing, "다른 도메인 이동/데이터 전송/자격증명 입력" 유도 문장 차단 |
| Sybil/스팸 | 속도 제한, 일일 회전 익명 ID, 평판 가중치 (평판은 계정 식별과 분리) |
| 재식별 | 희귀 조합(`site_host` + 희소 trait) k-익명성 임계 미만이면 집계 후 공개, 개별 trace는 공개하지 않음 |
| 서버 측 코드 실행 | 서버는 제출물을 실행하지 않는다. 검증은 격리 샌드박스에서 고정된 하네스만 실행 |
| 카탈로그 변조 | 불변 버전, 다이제스트 증거, 롤백, 캐시는 엔드포인트에 바인딩 |

## 8. 프라이버시·동의

- 기여는 기본 **off**, 명시적 opt-in (기존 `web_workflow_contribution` 유지, 텔레메트리 동의와 독립).
- 철회 시 대기 큐를 즉시 비우고 추가 업로드를 막는다.
- 사용자에게 "무엇이 전송되는지" 예시 JSON을 UI/문서로 보여준다.
- 서버 보관: 원본 Contribution은 G1~G3 처리 후 짧은 기간(예: 30일) 내 삭제하고, 집계값과 승인된 Record만 유지한다.
- 삭제 요청: 익명 구조라 개인별 삭제는 어렵다는 점을 고지하고, 대신 개인 식별 데이터를 애초에 받지 않는다.

## 9. MCP 인터페이스 (초안)

| 도구 | 설명 | 부작용 |
|---|---|---|
| `web_memory_recall(hostname, task_hint?)` | 승인된 Record의 L0/L1 힌트 최대 3개 반환 | 읽기 전용 |
| `web_memory_expand(id, level)` | 상세/증거 조회 | 읽기 전용 |
| `web_memory_propose(contribution)` | 폐쇄 스키마 Contribution 제출 (동의 필요) | 큐에 적재, 서버 검증 후 비동기 처리 |
| `web_memory_status()` | 설정/동의/캐시 상태 | 읽기 전용 |

규칙:
- 웹 메모리 Record는 **로컬에서 수정/삭제 불가** (기존 `web:` ID 규칙 유지).
- MCP 서버 `instructions`에 "새 사이트 작업 전 recall 호출, 작업 후 동의된 경우에만 propose" 지침을 넣는다.
- 반환 힌트는 항상 "untrusted reference, 사용자 지시가 우선" 프레이밍을 붙인다.
- 설치 가이드: 호스트별 MCP 설정 스니펫(예: stdio 실행 명령 또는 원격 URL)을 `docs/`에 제공한다.

## 10. 평가 계획

### 10.1 게이트키퍼 안전성
- **비밀값 주입 테스트**: 모든 입력 필드에 센티널 비밀값(가짜 API 키, 이메일, 전화번호, 로컬 경로)을 넣고, 큐/DB/카탈로그/로그 어디에도 남지 않음을 확인한다. (기존 G4 테스트 방식 확장)
- 인젝션 코퍼스: 악성 지시가 담긴 제출물을 넣어 게시되지 않음을 확인한다.
- 오탐률: 정상 제출물이 부당하게 폐기되는 비율을 측정한다.

### 10.2 중복제거 정확도
- 동일/유사/신규/충돌 라벨이 붙은 합성 trace 세트로 판정 정확도를 측정한다.
- 같은 지식 100건을 넣어도 Record가 1개이고 `support_count`만 증가하는지 확인한다.

### 10.3 성능 (호스트별 분리)
- Arise 자동 주입, Chrome MCP 수동 recall, 메모리 없음 세 조건을 조건당 3~5회 반복한다.
- 지표: 성공률, 벽시계 시간, 툴 호출 수, 오류 수. 평균과 편차를 보고한다.

## 11. 구현 단계 (해커톤 범위 제안)

| 단계 | 산출물 | 비고 |
|---|---|---|
| M1 | Contribution 스키마와 G0/G1 (결정적 redaction) + 센티널 테스트 | 가장 먼저, 안전 기반 |
| M2 | G3 중복·신규 판정 + 합성 데이터 평가 | 핵심 차별점 |
| M3 | 호스트 중립 Memory MCP (recall/expand/propose) + Chrome MCP 설치 가이드 | 범용성 데모 |
| M4 | G4 자동 A/B 하네스 연결 | 기존 probe 스크립트 재사용 |
| M5 | G2 LLM sanitizer, G6 감시와 롤백 | 시간 되면 |

데모 우선순위: M1+M2+M3를 보여주고, M4는 사전 측정 결과로 대체해도 된다.

## 12. 열린 질문

1. 공개 카탈로그를 어디에 호스팅하나? (기존 `api.meshcode.ai` 확장 vs 해커톤용 별도 엔드포인트)
2. 자동 승인 기준 N(독립 기여자 수)과 성능 임계값은 얼마로 할까?
3. 자유 텍스트 힌트 제출을 해커톤 범위에 포함할지, 폐쇄 스키마만 쓸지.
4. Chrome용 trace 수집을 확장(extension)으로 할지, 에이전트의 `propose` 호출에만 의존할지.
5. 로그인이 필요한 사이트의 재현 검증을 어떻게 안전하게 할지 (QA 계정 정책).
