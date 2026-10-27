# Memory Gatekeeper 설계 — 공유 웹 에이전틱 메모리의 검열·중복제거·갱신 파이프라인

상태: 설계 초안 (구현 전). 상세 기능 요구사항은 [`../spec/README.md`](../spec/README.md)와 하위 스펙을 기준으로 한다.
범위: 설계만. 코드는 포함하지 않는다.

## 1. 목표와 비목표

### 목표
1. 사람들이 특정 사이트에서 한 에이전트 업무(워크플로)를 공유하면, 다른 사람의 에이전트가 이를 주입받아 쓴다.
2. 공유 전에 **개인정보·비밀값(API 키, 토큰, 이메일, 계정 식별자 등)을 반드시 제거**한다.
3. **기존 메모리와 다른(새로운) 것만** 적재하고, 같은 것은 합치거나 버린다. 개선된 것은 기존 항목을 갱신한다.
4. Gatekeeper Agent가 중복·최신성·교체·폐기 여부를 정상 사례에서 최종 판정한다. 결정적 보안 검사와 실측 증거가 필수이며, 사람 검토는 저신뢰·상충·보안 예외에 한정한다.
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
                                                         ├ G5 Agent 판정·게시 (예외만 사람 검토)
                                                         └ G6 릴리스·롤백·감시
                                                                │
                                                       [Catalog API: 익명 GET]
```

핵심 원칙: **방어는 겹겹이(defense in depth)**. 어느 한 단계가 뚫려도 다음 단계가 잡는다. 클라이언트 단계는 신뢰하지 않는다 (서버가 항상 다시 검사한다).

## 4. 호스트 중립성 (Arise / Chrome / 그 외)

### 4.1 결론
가능하다. 메모리 계층을 "브라우저 구현"이 아니라 **MCP 서버 + 호스트 중립 스키마**로 분리하면 된다.
메모리는 안전하게 정규화한 URL route pattern, 업무 category, 정규화 액션에 의존하고 특정 브라우저 API에는 의존하지 않는다.

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
| Chrome + 브라우저 MCP (Playwright MCP, Chrome DevTools MCP 등) | Memory MCP를 같은 에이전트에 별도 추가 | MCP `instructions`와 도구 설명으로 "사이트 작업 전 `recall(url, task_category)` 호출"을 유도. 자동 주입이 아니라 에이전트가 호출 | 에이전트가 작업 후 로그인·동의가 있는 경우에만 `propose`를 호출하거나, Adapter가 도구 호출 로그를 정규화 |
| Chrome 확장 (선택) | 확장 설치 | 확장이 현재 URL을 안전하게 정규화해 URL/task 범위를 전달 | 확장이 탐색/액션 카테고리 관측 |
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
- canonical_url_pattern (query/hash/계정 ID를 제거한 안전한 라우트 패턴)
- task_category        (closed enum: compose-draft, save-draft, search, login-flow, upload, ...)
- action_sequence[]    (정규화 카테고리 + 결과 카테고리 + 소요 구간 버킷)
- outcome              (success | partial | failed) + error_category
- observed_traits[]    (closed enum: iframe-editor, resume-dialog, offscreen-save, nbsp-text, ...)
- contributor_user_id  (인증 세션에서 서버가 설정, payload에서 받지 않음)
- client_nonce         (사용자 범위 멱등 키)
```

수집하지 않는 것: 프롬프트, 입력한 텍스트, 셀렉터 문자열, 페이지 HTML/스크린샷, 원본 전체 URL, query/fragment, 쿠키, 계정 ID, 도구 인자/결과 원문. 검색과 비교에 필요한 안전한 canonical URL pattern만 허용한다.

### 5.2 자유 텍스트 힌트 제출 (별도 흐름, 선택)

사용자가 "이 사이트는 이렇게 하면 돼요" 같은 문장을 직접 공유하려는 경우에만 사용한다.
- 제출 전 **사용자에게 redaction 결과 diff를 보여주고 승인**받는다.
- 길이 상한(예: 600자), URL 금지(도메인 외), 명령형 문장 템플릿 제한.
- 기본 비활성. 켜도 G1~G5를 모두 통과해야 한다.

### 5.3 Memory Record (게시물)

```
MemoryRecord
- id                web:<slug>
- site_host, canonical_url_pattern, task_category, intent_key
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
- URL 정규화: host는 소문자화하고 기본 포트를 제거하며 userinfo/query/fragment/tracking은 삭제한다. 안전한 route pattern만 보존하고 public suffix 기준 host를 확인한다.
- 로그인 계정과 명시적 consent를 확인한다. contributor identity는 인증 session에서 설정하며 request body 값은 신뢰하지 않는다.
- 속도 제한, 기여자 ID별 일일 상한.
- 이 단계에서 거부된 입력은 **저장하지 않고** 카운터만 올린다.

### G1. 결정적 redaction (필수, AI 없음)
폐쇄 스키마라서 대부분 필요 없지만, 자유 텍스트/예기치 않은 필드가 올 때를 대비한 안전망이다.

| 대상 | 방법 |
|---|---|
| API 키/토큰 | 알려진 포맷 정규식 (cloud/provider 키 접두사, JWT, `Bearer`, private key 블록 등) + 높은 엔트로피 문자열 탐지 |
| 개인정보 | 이메일, 전화번호, 주민/신분증 형식, 카드번호(Luhn), IP, 주소 패턴 |
| 계정/식별자 | `@handle`, 사용자명 포함 경로, UUID/긴 숫자 ID |
| URL | query, fragment, 사용자 정보와 추적값 제거. 안전한 경로 템플릿만 보존하고 ID/계정명 세그먼트는 변수화 |
| 로컬 경로/호스트명 | `/Users/...`, `C:\Users\...`, 내부 IP/호스트 |

정책: **탐지되면 마스킹이 아니라 해당 필드/항목 폐기**를 기본으로 한다. 부분 마스킹은 잔여 정보가 남기 쉽다. 허용 목록(allow-list) 방식으로, 알려진 안전 토큰(enum 값, 정규화 카테고리)만 통과시킨다.

### G2. LLM sanitizer (보조, 단독 사용 금지)
- 역할: 결정적 규칙이 놓친 맥락 의존 개인정보(이름, 회사명, 문서 제목 등) 후보를 **표시**한다.
- 입력은 이미 G1을 통과한 데이터뿐이다.
- 출력은 "위험 표시"만이고, LLM이 내용을 새로 써서 통과시키지 않는다. 표시된 항목은 폐기하거나 사람 검토로 넘긴다.
- 프롬프트 인젝션 방어: 제출물은 항상 데이터로 취급하고, 시스템 지시와 분리해서 전달한다. 도구 권한은 주지 않는다.
- 호출 방식: `wam-gatekeeper` Rust 워커가 `LlmClient` trait 뒤에서 Meta Model API(OpenAI 호환 HTTP)를 직접 호출한다. 모델 응답(과 curator의 도구 호출 인자)은 Rust가 폐쇄 스키마로 다시 검증하고, 파싱 실패나 enum 밖 값은 fail closed다. Node 프로세스는 두지 않으며, 꼭 필요하면 격리된 작업 큐 워커로만 붙인다 ([`architecture.md`](architecture.md) 3절, [`../spec/model.md`](../spec/model.md)).

### G3. 신규성·중복 판정 ("기존과 다른 것만 적재")

목표: 같은 지식이 중복 적재되지 않게 하고, 더 나은 지식은 갱신한다.

1. **정규화**: (`site_host`, `canonical_url_pattern`, `task_category`, `intent_key`) 범위에서 `action_sequence`를 표준형으로 변환 (연속 중복 제거, 관측용 `snapshot`/`wait` 압축, 순서 의미 있는 것만 유지).
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
| 유사·개선 | 기존 Record의 개정안 Candidate (`supersedes` 지정), Gatekeeper Agent가 재현 증거 후 교체 판정 |
| 신규 | 새 Candidate |
| 충돌 (기존과 반대 지시) | 격리하고 G4 재현 검증에서 우열 판정 |

### G4. 재현 검증 (실측 A/B)
- 격리 프로파일에서 같은 모델/effort/태스크로 `tools-only` vs `injected`를 비교한다 (기존 `probe_agent_memory_live.py` 방식, fresh state).
- 게시 조건 예: 성공률이 기존 이상이고, 호출 수 또는 시간이 개선되며, 오류가 늘지 않을 것. 표본 수와 반복 횟수를 증거에 기록한다.
- 로그인/결제 등 위험 작업은 QA 계정/가짜 fixture에서만 실행하고, 발행/결제 같은 비가역 행동은 금지한다.
- **개선이 측정되지 않으면 폐기**한다. "그럴듯하다"는 게시 사유가 아니다.
- 한 번의 파일럿 차이를 일반화하지 않는다는 기존 원칙을 따른다 (반복 측정).

### G5. 게시 게이트
- 자동 게시 조건: G0/G1과 final secret scan 통과, G4 replay 기준 충족, 독립 증거와 최소 confidence 충족. 기준은 versioned policy로 관리한다.
- 정상 케이스의 최종 게시 판정은 Gatekeeper Agent가 한다. 모든 contribution에 관리자 승인을 요구하지 않는다.
- 증거 부족, 상충, 고위험, 저신뢰 입력은 게시하지 않고 사람 검토 큐로 보낸다.
- 교체와 삭제는 immutable version과 soft retirement로 처리한다. 이전 version은 기본 recall에서 제외하고 audit/rollback 용으로 보존한다. 물리 삭제는 법률·보안상 데이터 파기 절차에 한정한다.
- 관리자 역할은 예외 검토, abuse appeal, 긴급 kill switch 및 rollback이다. 일반적인 신규/중복/갱신 판정을 대신하지 않는다.

### G6. 릴리스·감시·롤백
- 클라이언트가 보고하는 익명 결과(주입 후 성공/실패 카테고리)를 집계해 **게시 후 성능 저하**를 감시한다.
- 임계 이하로 떨어지면 자동으로 `needs-reverify` 상태로 전환하고, 재검증 실패 시 이전 버전으로 롤백하거나 revoke한다.
- 오래된 항목은 `ttl/decay`로 재검증 대상이 된다 (사이트 UI는 바뀐다).

## 7. 위협 모델과 대응

| 위협 | 대응 |
|---|---|
| 비밀값/개인정보 유출 | 폐쇄 스키마, 클라이언트 1차 + 서버 G1 결정적 규칙 + G2 보조, 탐지 시 폐기, 원문 미저장 |
| 메모리 포이즌 (악의적 힌트) | 결정적 보안 검사, Agent의 구조화 판정, G4 실측 검증, 저신뢰/충돌만 사람 검토, URL 범위 밖 주입 금지 |
| 프롬프트 인젝션 (힌트 안에 악성 지시) | 구조화 필드와 고정 템플릿만 허용, 자유 텍스트 기본 비활성, 주입 시 untrusted framing, "다른 도메인 이동/데이터 전송/자격증명 입력" 유도 문장 차단 |
| Sybil/스팸 | 로그인 계정·기기별 속도 제한, 신규 계정/상호 호출 이상치 탐지, 영수증 1회 사용과 일일 점수 상한 |
| 재식별 | 희귀 조합(`site_host` + 희소 trait) k-익명성 임계 미만이면 집계 후 공개, 개별 trace는 공개하지 않음 |
| 서버 측 코드 실행 | 서버는 제출물을 실행하지 않는다. 검증은 격리 샌드박스에서 고정된 하네스만 실행 |
| 카탈로그 변조 | 불변 버전, 다이제스트 증거, 롤백, 캐시는 엔드포인트에 바인딩 |

## 8. 프라이버시·동의

- 기여는 로그인 사용자의 기본 **off**, 명시적 opt-in (기존 `web_workflow_contribution` 유지, 텔레메트리 동의와 독립).
- 철회 시 대기 큐를 즉시 비우고 추가 업로드를 막는다.
- 사용자에게 "무엇이 전송되는지" 예시 JSON을 UI/문서로 보여준다.
- 서버 보관: 원본 Contribution은 G1~G3 처리 후 짧은 기간(예: 30일) 내 삭제하고, 집계값과 승인된 Record만 유지한다.
- 기여 계정 ID는 점수 귀속과 삭제 요청 처리를 위한 내부 metadata로 별도 보관한다. 게시 내용에는 public handle이나 이메일을 넣지 않는다. 삭제 요청 시 저자 attribution을 제거하고, 남은 독립 근거가 없으면 Agent가 메모리를 재평가·retire한다.

## 9. MCP 인터페이스 (초안)

| 도구 | 설명 | 부작용 |
|---|---|---|
| `web_memory_recall(url, task_category)` | 승인된 URL/task 범위 Record의 힌트 최대 3개 반환; 인증 사용자에게만 usage receipt 제공 | 공개 메모리 읽기, 로그인 receipt 발급 |
| `web_memory_expand(memory_id, version_id)` | 해당 승인 버전의 상세/검증 근거 조회 | 읽기 전용 |
| `web_memory_propose(contribution)` | 로그인·동의가 있는 폐쇄 스키마 contribution 제출 | 큐 적재, 서버 검증 후 비동기 처리 |
| `web_memory_outcome(receipt_id, outcome)` | 영수증을 발급받은 계정이 결과를 한 번 보고 | 인증된 결과 기록; success만 점수 후보 |
| `web_memory_status()` | endpoint, 연결 계정, 동의, catalog 상태 | 읽기 전용; 비밀값 미포함 |

규칙:
- 웹 메모리 Record는 **로컬에서 수정/삭제 불가** (기존 `web:` ID 규칙 유지).
- MCP 서버 `instructions`에 "웹 업무 전 URL과 task로 recall 호출, 작업 후 로그인 및 동의된 경우에만 propose" 지침을 넣는다.
- 반환 힌트는 항상 "untrusted reference, 사용자 지시가 우선" 프레이밍을 붙인다.
- 설치 가이드는 웹에서 Claude/Codex용 endpoint 설정을 복사할 수 있게 제공한다. 브라우저 MCP는 별도로 설치하며 함께 연결되는지 스모크 테스트한다.

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
2. Agent가 자동으로 새 버전을 게시할 confidence, 독립 증거, replay 임계값은 얼마로 할까?
3. 자유 텍스트 힌트 제출을 해커톤 범위에 포함할지, 폐쇄 스키마만 쓸지.
4. Chrome용 trace 수집을 확장(extension)으로 할지, 에이전트의 `propose` 호출에만 의존할지.
5. 로그인이 필요한 사이트의 재현 검증을 어떻게 안전하게 할지 (QA 계정 정책).
