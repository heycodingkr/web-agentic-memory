# 영역명 기능 명세

상태: 초안 | 합의 | 구현 중 | 완료
영역 / 테스트 ID 접두사: `CORE` `GK` `DEDUP` `API` `LINK` `MCP` `SCORE` `WEB` `DB` `PRIV` `MODEL` 중 하나
기반 설계: `docs/design/...` 링크

## 1. 범위

이 영역이 책임지는 것과 책임지지 않는 것. 한 문단 이내.

## 2. 요구사항

| ID | 요구사항 | 합격 기준 | 관련 테스트 |
|---|---|---|---|
| `F-AREA-01` | | | |

작성 규칙:

- 합격 기준은 독립 oracle로 판정할 수 있어야 한다 (`../test/README.md` 원칙 4).
- 수치는 여기서 새로 정하지 않고 [`../test/metrics.md`](../test/metrics.md)를 인용한다.
- "관련 테스트"에는 `functional-tests.md`, `host-smoke.md`, `security-scenarios.md`, `user-flows.md`의 ID를 적는다. 비어 있으면 미완성이다.

## 3. 인터페이스와 데이터

API 경로, 스키마, 이벤트. 설계 문서에 이미 있으면 링크만 건다.

## 4. 비기능과 보안 제약

성능, 크기 상한, 프라이버시, 위협 모델(`SEC-*`) 연결.

## 5. 열린 질문
