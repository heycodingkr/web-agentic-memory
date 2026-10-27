# MEM: 구조화 메모리 레이어 (L0/L1/L2)

상태: 1차 상세 명세. `wam-core`는 색인 projection의 필수값을 검증한다. L0/L1/L2 content 저장, 색인 persistence, `expand`는 아직 없다. 여기서 말하는 L0/L1/L2는 메모리 레이어이며 테스트 계층 L0~L6과 다른 개념이다.

## 목적

메모리는 한 덩어리 텍스트가 아니라 세 레이어로 나뉜 구조화 레코드다. 에이전트는 작업을 시작할 때 가벼운 L0로 찾고, 주입에는 L1만 쓰고, 필요할 때만 L2를 연다. 그래야 recall 토큰이 작게 유지되고(recall당 최대 3개, 힌트당 ≤400 estimated tokens), URL과 키워드로 정확히 색인되며, 상세 절차와 사이트 구조는 필요할 때만 읽힌다.

## 레이어

| 레이어 | 이름 | 내용 | 누가 읽나 | 한도 |
|---|---|---|---|---|
| L0 | 위치·색인 | `kind`, `site_host`, `url_patterns[]`, `task_category`, `intent_key`, `keywords[]` | recall 매칭 (주입하지 않음) | 직렬화 ≤1 KiB, URL 패턴 ≤4, 키워드 ≤8개 (각 ≤32자) |
| L1 | 에이전틱 업무 요약 | `title`, `summary`, `key_points[]`, `pitfalls[]`, 적용 범위, 최신성 | 에이전트에 주입 | 힌트당 ≤400 estimated tokens, recall당 ≤3개 |
| L2 | 상세 | `steps[]` (조건부 단계 포함), `structure` (화면 유형, 경로, UI 특성), `preconditions[]`, `recovery[]`, 증거 요약 | `expand(level=2)`로만 | 직렬화 ≤8,000자 |

레이어 밖 메타데이터(`id`, `version`, `status`, `verified_at`, `confidence_band`, `support_count`, `supersedes`)는 DB 컬럼이다. 모델은 이 값을 쓰지 못한다.

**L0 키워드**는 recall 입력(URL + 키워드)과 직접 맞대어 점수를 매기는 색인이다. NFKC 정규화, 소문자, 공백 정리 뒤 ≤32자이고 문자·숫자·하이픈·공백만 허용한다. URL, 이메일, 비밀값 형태, 문장은 거부한다. 한국어와 영어를 함께 둘 수 있다 (예: `draft`, `초안`). 출처는 (a) `task_category`별 기본 동의어 표(데이터로 관리), (b) 사이트 별칭(`medium`, `미디엄`), (c) 큐레이터가 제안하고 서버가 검증한 토큰이다. 서버는 쓰인 키워드를 모아 공개 어휘로 제공한다 ([`recall.md`](recall.md)).

## 종류(kind)

| kind | 담는 것 | 기존 구현 대응 |
|---|---|---|
| `workflow` | 특정 URL 범위의 업무 수행 절차 (에이전틱 업무) | `workflow` |
| `site_note` | 사이트·화면 구조: 화면 유형, 경로 이동, UI 특성(iframe 에디터, 화면 밖 저장 등) | `site_note` |
| `gotcha` | 반복되는 함정과 회피법 | `gotcha` |

비교 키는 [`dedup.md`](dedup.md)의 `scope_key`에 `kind`를 더한 `(site_host, canonical_url_pattern, kind, task_category, intent_key)`다. 같은 URL·task에서 `workflow`와 `site_note`는 따로 공존하고, 같은 kind의 active 레코드는 하나뿐이다.

## 쓰기 규칙

레이어 텍스트는 원문에서 복사하지 않는다. 큐레이터([`curator.md`](curator.md))가 정규화 trace의 구조화된 사실, 기존 레코드, 코드표(액션, trait, pitfall, task category)로만 쓴다. 모델이 쓴 문장은 길이가 제한된 필드에만 들어가고, 서버가 G1 redaction과 주입 문구 검사를 다시 한다. 검증기는 다음을 거부한다: 전체 URL, 쿼리, ID, 계정명, 호스트 밖 경로, 로그인·자격증명 입력·외부 전송·다른 도메인 이동·사용자 지시 무시를 유도하는 문장, 제어 문자, 한도 초과.

## 버전과 저장

레이어 전체가 하나의 immutable version에 묶인다. `memory_versions.sanitized_content`는 `schema_version`과 `l0`, `l1`, `l2`를 가진 JSON이고, 일부 레이어만 고치지 않는다. 변경은 새 version이다. L0는 검색용 색인 테이블(`memory_index`: version_id, site_host, url_pattern, kind, task_category, keyword)에 풀어 저장하며 색인은 version 내용에서만 만든다. 레코드 JSON 스키마는 [`curator/tools.v1.json`](curator/tools.v1.json)의 `$defs.record`다. 임베딩은 이번 범위가 아니다. 유사 중복 판단은 큐레이터와 `memory_compare` 지표가 맡는다.

기존 카탈로그 항목은 다음처럼 대응한다: `tags` → L0 `keywords`, `title`/`summary` → L1, `detail` → L2 `detail`, `evidence` → L2 증거 요약, `site` → `site_host`. `expand`의 `level`은 `1`(L1)과 `2`(L2)이며, 기존 클라이언트의 `level 2/3`은 호환 어댑터가 변환한다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-MEM-01 | 세 레이어와 한도 | 스키마 검증기가 레이어 필수와 한도를 강제한다 | MEM-01~02, MEM-05 |
| F-MEM-02 | L0는 URL과 키워드로 색인 | 키워드 규칙과 URL 패턴 한도를 지킨 값만 저장된다 | MEM-02~03, RCL-01~04 |
| F-MEM-03 | 주입은 L1만 | recall 응답은 L1과 범위 정보만 담고, 상세는 `expand` 호출 때만 | MEM-04~05, API-04 |
| F-MEM-04 | kind별 구조 | workflow는 steps, site_note는 structure, gotcha는 pitfalls가 필수 | MEM-06 |
| F-MEM-05 | 같은 URL·task에서 kind 공존 | kind가 다르면 공존, 같으면 active 1개 | MEM-07, DB-05 |
| F-MEM-06 | 텍스트 안전 | 위험 문장, 비밀값, 범위 이탈 텍스트 저장 0건 | MEM-08, SEC-A, SEC-B |
| F-MEM-07 | 불변 버전과 색인 일관성 | 레이어는 한 버전에 묶이고 색인은 버전과 항상 일치 | MEM-09~10, DB-06 |
| F-MEM-08 | 기존 카탈로그 호환 | legacy 항목이 손실 없이 왕복 변환된다 | MEM-11, API-11 |

## 다른 명세에 필요한 변경

| 문서 | 변경 |
|---|---|
| [`dedup.md`](dedup.md) | `scope_key`에 `kind` 추가 |
| [`database.md`](database.md) | active 레코드 unique 제약에 `kind` 포함, `memory_index` 테이블 추가 |
| [`api.md`](api.md), [`mcp.md`](mcp.md) | recall 입력에 `keywords`, 어휘 endpoint, expand에 `level` 추가 ([`recall.md`](recall.md)) |
