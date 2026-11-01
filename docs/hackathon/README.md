# 메타 해커톤 — 공유 웹 에이전틱 메모리 (Shared Web Agentic Memory)

> 한 사람이 어떤 사이트에서 알아낸 작업 방법을, 모든 에이전트가 MCP로 주입받아 **처음 가는 사이트도 바로 능숙하게** 다루게 한다.

## 한 줄 아이디어

웹 기반으로 인덱싱된 사이트별 "작업 워크플로 메모리" 카탈로그를 공유하고, MCP를 통해 에이전트에 주입한다.
사람들이 주로 업무하는 사이트(Medium, Naver Blog 등)에서는 메모리 없이 시작할 때보다 **더 빠르고 더 적은 턴**으로 끝난다.
카탈로그는 Gatekeeper Agent가 URL/task별 contribution을 비교해 정상 사례를 결정하고 갱신한다. 저신뢰, 상충, 보안 예외만 사람이 검토한다.

## 왜 필요한가

- 브라우저 에이전트는 처음 보는 사이트에서 헤맨다 (포커스 못 잡기, 화면 밖 버튼 클릭, 에디터 입력 거부 등).
- 이 시행착오는 사이트마다 똑같이 반복되는데, 지금은 누구도 공유하지 않는다.
- 공유 메모리는 "이 사이트에서는 이렇게 하면 된다"를 400 토큰 이하 힌트로 주입한다.

## 파일럿 성능 (관찰값)

출처: `docs/specs/browser-agent-memory-evaluation.md` (2026-10-16). 모델: Codex `gpt-6-luna`, effort `max`.

| 대상 | 메모리 없음 | 메모리 주입 | 변화 |
|---|---|---|---|
| Medium (실사이트) | 85.5초 / 10콜 | 44.3초 / 7콜 | 약 48% 단축, 콜 30% 감소. 빈 P 블록 문제도 해결 |
| Naver Blog (실사이트) | 84.1초 / 16콜 | 58.8초 / 12콜 | 약 30% 단축, 콜 25% 감소 |
| 리치 에디터 (fixture) | 130.3초 / 16콜 | 55.2초 / 11콜 | 약 58% 단축 |
| iframe 에디터 (fixture) | 195.4초 / 28콜, 오류 3 | 130.9초 / 17콜, 오류 0 | 콜과 오류 감소 |

### 주의 (발표/README에서 정직하게 밝힐 것)

- 조건당 **1회 시행**, 실행 순서 무작위 아님. 일반적 지연 보장이 아니라 파일럿 관찰이다.
- Naver 실사이트는 두 번째 실행에서 이어쓰기 팝업과 임시저장 개수가 달라 통제된 A/B가 아니다.
- iframe fixture에서는 자동 주입(130.9초)이 수동 recall(91.7초)보다 느렸다.
- TODO: 조건별 3~5회 반복 측정으로 평균/편차 보강.

## 구조

```
사이트 방문 (정규화 URL pattern + task)
   └─ 에이전트 ──MCP──▶ 공유 웹 메모리 카탈로그 (api.meshcode.ai/web-memory/v1/catalog)
                         ├─ 도메인/태스크 힌트 (L0/L1, 최대 3개, ≤400 tokens, untrusted framing)
                         └─ 상세/근거는 browser_memory_expand 로 필요할 때만
```

기존 프로토타입에 있던 기능 (`docs/specs/web-agentic-memory.md`):

- 설정 "웹 에이전틱 메모리 활용" (`web_agentic_memory_enabled`, 기본 on) — 개인 메모리와 독립
- 승인된 카탈로그 캐시(최대 256KiB/32개, 5분 TTL), 오프라인 번들(Medium/Naver)
- 선택적 기여(`web_workflow_contribution`, 기본 off): legacy path는 관측된 hostname과 고정된 도구/결과 카테고리만 전송. 새 앱에서는 로그인 사용자 소유권과 URL route pattern/task를 다룸
- 서버: legacy catalog/read-report 경로, 관리자 전용 candidate → smoke evidence → publish/version rollback
- 사용자 리포트는 격리 데이터이며 프롬프트 지침으로 직접 쓰이지 않음

## 로그인 기반 Agent Gatekeeper (해커톤 새 작업)

이 독립 실행형 프로젝트는 기존 prototype의 익명 report와 사람 전수 승인 흐름을 그대로 가져오지 않는다. 사용자가 로그인하고 명시적으로 동의한 contribution만 URL/task scope로 큐에 넣는다. Gatekeeper Agent가 Meta 모델을 사용해 의미상 중복·신규·stale 후보를 판단하고, 서버의 deterministic privacy checks와 격리 replay가 통과한 경우 정상 건을 결정한다. 모델 출력만으로 게시하지 않으며 confidence 부족, 충돌, 보안 신호는 예외 검토로 보낸다.

1. **기여**: 로그인한 사용자가 canonical URL pattern, task category, 정규화 action/result 범주만 동의 후 제출한다.
2. **비교**: Gatekeeper는 기존 URL/task memory version을 찾고 Meta 모델에서 구조화된 duplicate/new/stale decision candidate를 받는다.
3. **검증**: 서버가 scope, privacy, security, replay evidence를 확인한다. 모델 장애·불확실 응답은 fail closed다.
4. **판정**: 정상 기여는 Agent가 reject, merge, create, supersede, retire 중 결정한다. 사람은 저신뢰·상충·보안 예외만 검토한다.
5. **보상**: 새 승인과 다른 로그인 사용자의 success receipt에 따라 작성자 dashboard에 점수를 기록한다. 익명 조회는 공개 힌트를 읽을 수 있어도 user receipt나 점수를 만들지 않는다.
6. **복구**: 버전은 immutable로 보존하며 stale/abuse 판단은 soft retirement, rollback, reversal event로 처리한다.

원칙: 서버는 제출된 명령을 실행하지 않는다. AI가 쓴 힌트도 "untrusted framing"으로 주입한다.

상세 설계(검열, 중복제거, 호스트 중립 MCP): [`../design/memory-gatekeeper.md`](../design/memory-gatekeeper.md)

## 해커톤 적합성과 데모 기준

첨부 참가 가이드에 따라 실제 Meta 모델을 제품의 핵심 판정 흐름에 연결한다. 현재 명세는 Gatekeeper Agent의 structured dedup/stale candidate generation에 Meta Model API 또는 Meta open-weight model을 쓰도록 정의했다. live demo에는 실제 provider/model ID와 결정 trace를 남기며, contract test용 fake model만으로 Meta 사용을 주장하지 않는다. 선택 가능한 모델과 계정 접근은 구현 시점에 확인하고 secret은 저장소에 넣지 않는다. ([참가 가이드 원본](../../Details_–_Meta_Global_AI_Developer_Hackathon_–_Cerebral_Valley.pdf), [`../spec/model.md`](../spec/model.md))

**Meta 모델 사용 현황 (2026-11-01).** Gatekeeper는 Meta Model API의 Muse Spark 1.3(`muse-spark-1.3`)으로 실제 판정한다. 시드 시나리오 5개가 모두 통과했고, 라벨 있는 60건 starter 코퍼스는 `reasoning.effort=low`에서 58/60이다(상충·프롬프트 주입 각각 12/12, 놓친 2건은 사람에게 넘김). 기여 1건당 비용은 콘솔 정가 기준 약 $0.02(추정)이고, 크레딧이 떨어지면 기여는 큐에서 대기하며 `WAM_LLM_DAILY_BUDGET_USD`로 일일 상한을 둘 수 있다. 판정마다 감사 기록에 provider·모델(`muse:muse-spark-1.3`)과 도구 호출이 남는다. 행사 가이드의 심사 기준(Meta 모델 사용 여부는 1단계 합격·불합격, "Use of Meta AI Tooling" 25%)에 맞춘 근거, 비용 표, 한계는 [`../design/meta-model-usage.ko.md`](../design/meta-model-usage.ko.md)에 있다.

제출 영상은 3분 제품 데모를 기준으로 준비한다. 새로 만든 웹 MCP 설정 복사, Claude/Codex 설치, browser MCP와 memory MCP 공존, 로그인 contribution부터 Agent 갱신 결정, 다른 로그인 사용의 성공과 익명 호출 무점수를 연결해 보여준다. 기존 benchmark/prototype 결과와 해커톤 중 새로 만든 기능을 명확히 분리해 발표한다.

## 데모 시나리오 (안)

1. `/guide`에서 Claude Code 또는 Codex 설정을 복사해 Memory MCP를 연결하고 Chrome browser MCP와 공존하는지 보여준다.
2. 사용자 A가 로그인하고 fixture의 특정 URL/task에서 관측 결과를 동의 후 제출한다.
3. Gatekeeper가 Meta 모델로 중복/신규/stale 후보를 분류하고 privacy gate와 replay evidence 뒤에 version을 결정한다.
4. 로그인한 사용자 B가 같은 URL/task 작업에 memory를 사용해 성공한다. A의 dashboard에는 점수가 반영된다.
5. 익명 사용자가 같은 memory를 호출해도 조회는 가능하지만 receipt와 점수는 생기지 않는 것을 DB/화면으로 비교한다.
6. 별도 benchmark에서 memory 전후 작업 시간과 도구 호출을 보고하되, 기존 관찰값과 반복 측정 결과를 구분한다.

## 폴더 구성 (예정)

- `README.md` — 이 문서
- `results/` — 반복 측정 결과 (CSV/MD)
- `demo/` — 데모 스크립트와 영상 링크
- `curator/` — AI 큐레이터 설계/프로토타입

## TODO

- [x] 해커톤 이름/마감/제출 형식 확인: Meta Global AI Developer Hackathon – Cerebral Valley, 제출 마감 2026-11-04 21:00 ET(CV 플랫폼), 3분 데모 영상과 공개 저장소 필수, 1단계 심사는 Meta Model API 또는 오픈 웨이트 사용 여부의 합격·불합격
- [ ] 제출 전 repository 공개 여부, 라이선스, 기존 reference asset의 재배포 권리 확인
- [ ] 반복 측정 (조건별 3~5회)
- [x] Meta model-backed Gatekeeper Agent 실제 provider 통합: 2026-11-01 Muse Spark 1.3 실제 호출 평가, 판정 trace는 감사 기록에 저장([`../design/meta-model-usage.ko.md`](../design/meta-model-usage.ko.md))
- [ ] 데모 화면에서 model trace(provider/model ID, 판정 근거)를 보여 주기
- [ ] 로그인 contribution, 예외 검토, 버전 rollback, score ledger 데모
- [ ] 3분 제출 영상에서 해커톤 신규 작업을 기존 코드/벤치 결과와 분리해 표시
- [ ] 공개 데모용 카탈로그 엔드포인트 배포 확인 (현재 미배포로 기록됨)
