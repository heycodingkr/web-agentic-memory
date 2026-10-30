# GK: Memory Gatekeeper Agent의 결정 권한

상태: Gatekeeper tool/policy layer, scripted/fake and Responses API provider interfaces, PostgreSQL queue/store, versioned batch processing, server worker and replay-gated publication path are implemented and covered by tests. Real external model quality, configured browser replay executable, production deployment/failover, and post-publication withdrawal re-evaluation remain unverified or incomplete.

## 역할

Gatekeeper Agent는 로그인 사용자가 제출한 특정 URL 범위의 웹 업무 기록을 기존 승인 메모리와 비교하고, 새로 만들지·중복으로 합칠지·현재 버전을 교체할지·오래된 버전을 폐기할지 최종 판정한다. 일반 케이스에서 관리자가 매번 누르는 승인 버튼은 필수 단계가 아니다. 사용자가 맡긴 이 판정은 gatekeeper가 수행한다. 모델 호출 방식(지침과 도구)은 [`curator.md`](curator.md), 작업 큐는 [`queue.md`](queue.md)가 정한다.

AI 판단은 자유로운 프롬프트 실행이 아니다. 서버가 고정한 정책과 구조화 데이터만 입력하며, 결정적 보안 검사·증거 재생·권한 검증을 통과하지 않은 결과는 DB에 게시할 수 없다. 확신이 낮거나 정책 위반 신호가 있으면 agent는 자동 게시를 거부하고 예외 검토 큐로 보낸다. 사람 검토는 예외, 이의제기, 보안 사고에 사용한다.

## 입력

입력은 authenticated contributor id, canonical URL pattern, task category, 기존 활성 memory version과 증거, sanitized action trace, 결과 category, 해당 URL의 UI/fingerprint 관측값, source consent version으로 제한한다. 원본 prompt, 브라우저 입력 텍스트, page HTML, screenshot, selector, 쿠키, 원 URL query를 LLM에 보내지 않는다.

기존 기록을 비교할 때 정확한 URL pattern과 task category가 일치하는 활성 메모리를 기준으로 한다. 상위 route 메모리는 candidate 범위가 하위 route에 포함되고 task meaning이 같을 때만 비교 대상이다. 같은 host라는 이유만으로 merge하지 않는다.

## 결정 종류

| 결정 | 의미 | 데이터 결과 |
|---|---|---|
| reject_invalid | schema, 동의, 비밀값 또는 위험한 절차 검사 실패 | 게시 없음, 제한된 사유 코드 반환 |
| duplicate_support | 기존 절차와 의미가 같고 더 낫지 않음 | 새 메모리 없음, 독립 기여 support만 반영 |
| merge_evidence | 같은 절차에 독립 증거를 보탬 | 기존 version 유지, support/evidence 갱신 |
| create_new | 같은 URL/task intent에 기존 기록이 없음 | 새 record와 첫 version 생성 |
| supersede | 같은 intent에서 새 절차가 검증상 더 나음 | 새 immutable version을 만들고 이전 버전을 superseded |
| retire | 기존 안내가 더 이상 유효하지 않거나 안전하지 않음 | active pointer 제거, tombstone과 사유 저장 |
| needs_human_review | 증거 상충, confidence 부족, 정책 예외 | 공개 변화 없음, 전담 큐에 보냄 |

hard delete는 일반 gatekeeper decision에 없다. 삭제는 새 기록의 게시가 아니라 공개 검색에서 제외하는 soft retirement다. 모든 decision은 이전/새 버전 관계와 근거를 남겨 롤백 가능해야 한다.

## 비교·최신성·개선 판정

먼저 exact fingerprint와 normalized action sequence로 중복을 판정한다. 다음으로 같은 task intent에서 절차 단계, 성공·실패 결과, pitfall 해결, UI fingerprint, 검증 시점을 비교한다. 단순히 더 짧다는 이유만으로 개선 판정을 내리지 않는다. 성공률이 같거나 높고, 위험 행동이 늘지 않으며, replay evidence가 합격해야 supersede할 수 있다.

최신성은 작성 시각만으로 판정하지 않는다. 다음을 함께 본다.

- last_verified_at과 정책상 freshness window
- 최근 독립 사용자 outcome에서 실패율이 증가하는지
- 같은 canonical URL pattern의 새 UI fingerprint가 관측됐는지
- 새 contribution이 기존 지침과 충돌하는지
- 동일 환경의 replay에서 기존/새 후보 성능 차이가 있는지

오래됐지만 실패 증거가 없으면 곧바로 폐기하지 않고 needs-reverify 상태로 둔다. 기존 실패와 새 후보 성공이 재현되면 supersede 또는 retire를 결정한다. 신뢰도 기준 미달은 새 메모리를 넣지 않는 기본 결과다.

## 고정 안전 게이트

1. 서버 schema, URL normalization, consent, user ownership 검증을 통과한다.
2. deterministic redaction에서 secret 또는 PII가 검출되면 해당 candidate를 폐기한다.
3. prompt-injection 분류기는 제출물을 data로만 전달한다. 제출된 명령을 실행할 권한은 없다.
4. 결정의 URL 범위가 submission scope를 넘어가면 거부한다.
5. replay는 격리된 fixture/QA 환경에서 고정된 harness만 사용한다. 게시, 공유, 결제, 계정 변경 같은 비가역 행동은 차단한다.
6. Agent decision은 policy version, model identifier, structured rationale code, confidence band, evidence digest를 기록한다. 민감한 chain of thought와 원문 prompt는 기록하지 않는다.
7. 승인 후보는 catalog schema validator와 final secret scan을 통과한 뒤 immutable version으로 게시한다.

## 결정 권한과 예외

Gatekeeper Agent는 신규·중복·갱신·soft-retire의 정상 판정을 소유한다. 관리자는 모든 기여를 사전 승인하는 역할이 아니라, confidence 기준 미달, 상충 증거, 보안 예외, abuse 신고, 정책 변경을 검토한다. 관리자는 published version을 kill switch로 즉시 차단할 수 있지만, 사유와 rollback target을 audit log에 남긴다.

agent가 reject 또는 retire를 결정한 작성자는 대시보드에서 reason code와 사용 가능한 증거 요약을 확인하고 재제출 또는 이의제기를 할 수 있다. 사람의 이의제기 결정도 agent 판정을 덮어쓸 때 반드시 audit event와 policy basis를 남긴다.

## 관측 가능성

기여자는 접수됨, 검증 중, 비교 중, 재생 중, 게시, 중복, 교체, 폐기, 사람 검토 필요 상태를 볼 수 있다. 공개 카탈로그에는 승인된 현재 version만 나타난다. 관리자 audit 화면은 decision tree와 digest를 보여주지만 contributor private payload는 최소 권한으로 열람한다.

## 관련 테스트

GK-39~48은 판정별 상태 전이, 최신성 신호, replay 실패, low confidence, prompt injection, soft retire, rollback, agent retry와 human exception을 검증한다. DEDUP-13~18은 URL/task scope 별 중복과 supersede를 검증한다. SECURITY-C 및 SECURITY-B는 score abuse와 악성 contribution이 agent 결정을 오염시키지 않는지 검사한다.
