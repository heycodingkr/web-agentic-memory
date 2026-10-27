# 테스트 하네스 자체 검증

상태: 일부 구현·실행됨. 실험이 신뢰되려면 하네스가 먼저 검증되어야 한다([`metrics.md`](metrics.md) 9절). 이 문서는 하네스 도구(`tests/lib/wamtest`, `tests/env`)의 검증 케이스와 현재 상태를 기록한다. 제품 기능 테스트가 아니다.

실행:

```
python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib -v     # 브라우저 없이, 수 초
python3 -B tests/env/selfcheck.py                                          # 로컬 Chrome 필요, 약 1분
```

| ID | 대상 | 합격 기준 | 도구 | 상태 |
|---|---|---|---|---|
| HARN-01 | fixture oracle 정확도 | 성공·실패를 의도적으로 만든 시나리오 ≥20건에서 판정과 사유 코드가 100% 일치, oracle이 에이전트 포트로 노출되지 않음 | `tests/env/selfcheck.py`, `tests/env/arise_check.py` | 구현. 로컬 Chrome 156 32/32, 로컬 Arise 0.1.54(격리 실행) 32/32 |
| HARN-02 | 무효 run 분류 | 신호(로그인 만료, 쿼터, 5xx, UI 변경, 환경 불일치, 기능 없음)가 우선순위대로 분류되고 종료 이벤트 없는 run은 `invalid-env` | `wamtest.bench.invalid` | 분류기 구현. fixture 장애 모드로 끝에서 끝까지 검증은 미구현 |
| HARN-03 | 시간 측정 오차 | 트레이스 타임스탬프와 외부 시계 차이 ≤100ms | 미정 | 미구현 |
| HARN-04 | 정리 | run 종료 뒤 하네스가 만든 프로세스·디렉터리 잔존 0, 남의 프로세스 영향 0 | `wamtest.procs`, `selfcheck.py`, `arise_check.py` | 구현. Chrome과 Arise 실행 모두 잔존 0, 실행 중이던 사용자 Arise 프로세스는 전후 동일 |
| HARN-05 | 재현성 | 같은 입력 fixture 반복에서 제출까지 시간의 변동계수 ≤15% | 미정 | 미구현 |
| HARN-06 | 환경 일치 | 호스트가 쓴 설정 경로가 의도와 다르면 run이 무효 처리됨 | `test_arise_adapter.py` | 정적 부분 구현: Arise 어댑터가 모든 데이터 경로를 임시 루트로 돌리고 사용자 세션 변수를 물려받지 않는다. 실행 중 설정 경로 불일치 탐지는 미구현 |
| HARN-07 | 통계 계산 | 알려진 표본에서 중앙값·표준편차·부트스트랩 CI가 맞고, 같은 seed는 같은 결과, A/B/C·invalid 등급 규칙이 문서대로 | `wamtest.bench.stats` | 구현 |
| HARN-08 | 조건 교차 스케줄 | 모든 (task, block)에 각 조건이 정확히 한 번, 고정 순서 금지, seed 기록 | `wamtest.bench.schedule` | 구현 |
| HARN-09 | 트레이스 지표 | 제출까지 시간·호출 수와 제출 뒤 대기가 분리되고 명시적 done/failed 없이는 완료가 아니다 | `wamtest.bench.trace` | 구현 |
| HARN-10 | 위험 행동 가드 | 공유·발행·결제·삭제 라벨과 경로가 차단되고 `blocked-action` 이벤트가 남는다 | `wamtest.bench.guards` | 판정 엔진 구현. 브라우저 연결은 미구현 |
| HARN-11 | 명세와 테스트 ID 추적 | 명세가 인용한 테스트 ID가 모두 `docs/test`에 정의되어 있다 | `wamtest.doctrace` | 구현 |
| HARN-12 | 큐레이터 도구 정의 | `curator/tools.v1.json`이 구조 규칙, 한도, 결정 enum, 지침·명세와 일치한다 (`CUR-01`의 정적 부분) | `selftest/test_curator_tools.py` | 구현 |

운영 규칙: 하네스가 검증을 통과하기 전의 측정값은 README 수치로 쓰지 않는다([`metrics.md`](metrics.md) 1.1절 사전 등록).
