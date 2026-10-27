# DEDUP: URL/task 범위 중복·최신성·갱신 판정

상태: 1차 상세 명세. 중복 검색과 Gatekeeper Agent 판정기는 미구현이다.

## 범위

중복 판정은 같은 웹 호스트에서 수집된 기여를 안전한 canonical URL pattern, 업무 category, intent 기준으로 묶어 새 지식인지·기존 지식의 재확인인지·교체가 필요한지 결정한다. 최종 공개 판정은 Gatekeeper Agent가 고정 정책과 재생 증거에 따라 한다. 사용자 task의 의미가 다른 후보를 같은 host라는 이유로 병합하지 않는다.

## 정규 비교 키

`scope_key = (site_host, canonical_url_pattern, kind, task_category, intent_key)`이다. `kind`는 [`memory.md`](memory.md)의 `workflow`, `site_note`, `gotcha`다. `canonical_url_pattern`은 query, fragment, userinfo, tracking을 제거하고 동적 object/account ID를 `:id` 또는 `:account` template으로 바꾼 결과다. 전체 URL 또는 원본 ID를 key나 fingerprint에 넣지 않는다. `intent_key`는 화면에서 고른 task와 폐쇄형 action/result category에서 계산한 안정값이며 자유 텍스트 prompt에서 만들지 않는다.

조회 순서는 exact route + task, 안전한 상위 route + 같은 task, host-wide generic memory 순이다. 상위 범위에서 내려온 memory는 해당 route를 포함하는 명시적 scope일 때만 사용한다. 동일 URL 안의 다른 task는 다른 record다. 다른 host·registrable domain으로 후보가 건너가면 agent 판단 전에 거부한다.

## 판정 절차

1. CORE 정규화와 privacy/consent hard gate를 통과시킨다.
2. 동일한 `scope_key`에서 active version과 `needs-reverify` version을 가져온다. retired/revoked version은 기본 recall 검색에서 제외하되 감사·rollback 인덱스에는 보존한다.
3. action sequence, normalized fingerprint, outcome category, observed traits, verification evidence를 비교한다.
4. exact duplicate는 기존 record에 비식별 support evidence만 추가한다. 신규 메모리 승인 점수는 지급하지 않는다.
5. 유사하지만 개선이 없는 후보는 duplicate/declined로 종료한다. 새로운 scope의 유효 성공 절차는 `create_new` 후보가 된다.
6. 기존 절차보다 낫다고 주장하는 후보는 같은 fixture/model/tool 조건의 replay가 통과하기 전까지 active pointer를 바꾸지 않는다.
7. 개선 증거가 충분하면 새 immutable version을 만들고 기존 version을 `superseded`로 만든다. 기존 지식이 더 이상 안전/유효하지 않고 대안도 없으면 `retire` tombstone을 만든다.
8. 충돌, 낮은 confidence, 범위 모호성, 보안 신호는 `needs_human_review`로 격리한다. 사람은 예외만 검토하며 정상 판정을 전수 승인하지 않는다.

최신성은 등록 시각만으로 계산하지 않는다. `last_verified_at`, freshness policy, 실패 outcome 추세, 새 UI fingerprint, 충돌 여부, replay 성능을 함께 고려한다. 메모리가 오래됐다는 사실만으로 즉시 삭제하지 않고 `needs-reverify`를 우선한다.

## 결정 데이터와 불변조건

각 decision은 source contribution, 비교 대상 version, 결정 종류, policy version, confidence band, reason code, evidence digest, 결정 시각을 남긴다. 모델 chain-of-thought와 민감한 입력은 저장하지 않는다. Agent는 다음 action만 선택한다: `reject_invalid`, `duplicate_support`, `merge_evidence`, `create_new`, `supersede`, `retire`, `needs_human_review`.

Agent가 hard-delete를 수행할 수 없다. 공개 catalog에서 빠지는 `retire/revoke`와 실제 데이터 파기는 별개의 법률·보안 절차다. `supersede`/rollback은 immutable version history를 유지한다. 동시에 같은 scope에 대해 decision이 들어오면 하나의 active version만 존재해야 한다.

## 요구사항과 테스트

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| F-DEDUP-01 | URL/task scope로만 비교 | scope가 다른 경로·업무·host 간 자동 merge 0건 | CORE-24~25, DEDUP-06~07, DB-05 |
| F-DEDUP-02 | 정확 중복 통합 | 같은 fingerprint는 새로운 record/version을 만들지 않음 | DEDUP-01, DEDUP-11, GK-39 |
| F-DEDUP-03 | 신규 절차 등록 | 빈 scope의 성공 후보는 Agent 판정과 보안/replay 통과 후 publish | DEDUP-04, GK-40~41 |
| F-DEDUP-04 | stale 후보 감지 | timestamp 단독 폐기 금지; 실패·fingerprint·replay 근거 사용 | DEDUP-13~16, GK-42~43 |
| F-DEDUP-05 | 개선 version 전환 | 새 version을 추가하고 active pointer를 원자 이동, 이전 content 변경 없음 | DEDUP-03, DEDUP-17, DB-06, DB-13 |
| F-DEDUP-06 | 모순/저신뢰 처리 | 확인 불가하면 catalog는 그대로 두고 human exception queue에 적재 | DEDUP-05, DEDUP-18, GK-44, GK-48 |
| F-DEDUP-07 | retirement와 복구 | 공개 recall에서는 숨기되 audit과 rollback에 과거 version 보존 | GK-47, DB-13 |
| F-DEDUP-08 | 병렬·재시도 안전성 | 동일 scope에 active memory가 중복되지 않고 job 재시도가 멱등 | DEDUP-12, GK-37~38, DB-10~11 |

정확도/지연 목표는 [`../test/metrics.md`](../test/metrics.md)에 둔다. 종합 흐름은 [`../test/feature-acceptance.md`](../test/feature-acceptance.md)의 `ACC-SERVER-01~02`를 따른다.
