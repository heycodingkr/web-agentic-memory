# 테스트 하네스 자체 검증

상태: 일부 구현·실행됨. 실험이 신뢰되려면 하네스가 먼저 검증되어야 한다([`metrics.md`](metrics.md) 9절). 이 문서는 하네스 도구(`tests/lib/wamtest`, `tests/env`)의 검증 케이스와 현재 상태를 기록한다. 제품 기능 테스트가 아니다.

실행:

```
python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib -v     # 브라우저 없이, 수 초
python3 -B tests/env/selfcheck.py                                          # 로컬 Chrome 필요, 약 1분
python3 -B tests/env/drift_check.py                                        # UI v1/v2 복구·실패/독립 oracle
python3 -B tests/env/repeatability_check.py --repeats 10                    # HARN-03/05, 로컬 Chrome
python3 -B tests/env/guard_check.py                                         # HARN-10, 로컬 Chrome
python3 -B tests/env/arise_check.py --styles rich                           # HARN-06, 설치된 Arise 필요
```

| ID | 대상 | 합격 기준 | 도구 | 상태 |
|---|---|---|---|---|
| HARN-01 | fixture oracle 정확도 | 성공·실패를 의도적으로 만든 시나리오 ≥20건에서 판정과 사유 코드가 100% 일치, oracle이 에이전트 포트로 노출되지 않음 | `tests/env/selfcheck.py`, `tests/env/arise_check.py` | 구현. Chrome 40/40 + 분리/정리 2, UI drift v1/v2 6 + 정리 1 통과(2026-10-28). Arise 기존 32/32 증거는 유지하며 새 style은 미실행 |
| HARN-02 | 무효 run 분류 | 신호(로그인 만료, 쿼터, 5xx, UI 변경, 환경 불일치, 기능 없음)가 우선순위대로 분류되고 종료 이벤트 없는 run은 `invalid-env` | `wamtest.bench.invalid` | 분류기와 runner 연결 구현. 무종료·알 수 없는 신호·adapter 예외를 invalid-env로 보존하며, 과다 무효는 전체 비교를 무효 처리. 실제 host 장애 흐름은 미검증 |
| HARN-03 | 시간 측정 오차 | trace `ts` 벽시계와 `t` monotonic 경과의 최대 차이 ≤100ms; controlled run에서 누락·초과는 `invalid-env` | `wamtest.bench.trace.clock_error_ms`, `repeatability_check.py` | 구현. 벽시계 step 150ms를 controlled test에서 `invalid-env`로 분류; Chrome fixture 10회 최대 1ms 차이 (run `20261027-232656-3d58`) |
| HARN-04 | 정리 | run 종료 뒤 하네스가 만든 프로세스·디렉터리 잔존 0, 남의 프로세스 영향 0 | `wamtest.procs`, `selfcheck.py`, `arise_check.py` | 구현. Chrome과 Arise 실행 모두 잔존 0, 실행 중이던 사용자 Arise 프로세스는 전후 동일 |
| HARN-05 | 재현성 | 같은 입력 fixture 반복에서 제출까지 시간의 변동계수 ≤15%; 최소 5회, 누락 측정은 합격 불가 | `wamtest.bench.repeatability`, `repeatability_check.py` | 구현. 10회 제출 TTS CV 1.18% (0.7099s 평균, 0.008399s 표준편차; run `20261027-232656-3d58`) |
| HARN-06 | 환경 일치 | 호스트가 쓴 설정 경로가 의도와 다르면 run이 무효 처리됨 | `test_arise_adapter.py`, `arise_check.py` | 부분 구현. 정적 env 경로 검사와 runtime XDG settings read probe 통과 (`env-selfcheck-arise/20261027-233155-77c8`, rich 7/7). 나머지 런타임 cache/session 경로는 host별 검증 필요 |
| HARN-07 | 통계 계산 | 알려진 표본에서 중앙값·표준편차·부트스트랩 CI가 맞고, 같은 seed는 같은 결과, A/B/C·invalid 등급 규칙이 문서대로 | `wamtest.bench.stats` | 구현. 무효 제외 후 성공률 분모, 제출 전 실패의 null 지표, 0 기준값 보고 회귀 검증 |
| HARN-08 | 조건 교차 스케줄 | 모든 (task, block)에 각 조건이 정확히 한 번, 고정 순서 금지, seed 기록 | `wamtest.bench.schedule`, `runner` | 구현. 미등록 task/block·빈 설계·중복 입력 거부, 실행 전 plan/hash 저장, fixed order 거부 |
| HARN-09 | 트레이스 지표 | 제출까지 시간·호출 수와 제출 뒤 대기가 분리되고 명시적 done/failed 없이는 완료가 아니다 | `wamtest.bench.trace`, `context` | 구현. runner가 잘못된 시간/종료 순서를 거부하고 RunDir 정리가 무종료 adapter를 성공 처리하지 않음 |
| HARN-10 | 위험 행동 가드 | 공유·발행·결제·삭제 라벨과 경로가 차단되고 `blocked-action` 이벤트가 남는다 | `wamtest.bench.guards`, `browser_guard.py`, `test_browser_guard.py`, `guard_check.py` | 구현. CDP Fetch 정책 경계의 허용·차단·오류·fail-closed와 로그 URL 원점만 기록하는 동작을 7개 브라우저 없는 단위 테스트로 확인했다. 로컬 Chrome에서 publish_count 0 + blocked-action도 확인 (`env-browser-guard/20261027-233017-a251`, pass 2/fail 0). Arise 자체 MCP 도구 호출은 별도 host adapter 범위 |
| HARN-11 | 명세와 테스트 ID 추적 | 명세가 인용한 테스트 ID가 모두 `docs/test`에 정의되어 있다 | `wamtest.doctrace` | 구현 |
| HARN-12 | 큐레이터 도구 정의 | `curator/tools.v1.json`이 구조 규칙, 한도, 결정 enum, 지침·명세와 일치한다 (`CUR-01`의 정적 부분) | `selftest/test_curator_tools.py` | 구현 |

2026-10-28 감사에서 기존 HARN-12 실패 두 건을 재현·수정했다. per-job limit 키 검사는 현재 scope-batch 계약(8 reads/1 decision/20 candidates)으로 바꾸고, 지침에 STORAGE_UNAVAILABLE의 stop/retry 처리를 추가했다. 공용 실행기와 replay 사용법은 [tests/bench](../../tests/bench/README.md), 사전 등록 양식은 [prereg/_template.md](prereg/_template.md)에 있다. CI workflow의 모든 명령은 로컬에서 검사하며 GitHub 실행 자체는 별도 증거다.

운영 규칙: 하네스가 검증을 통과하기 전의 측정값은 README 수치로 쓰지 않는다([`metrics.md`](metrics.md) 1.1절 사전 등록).
