# 2026-11-04 메모리·CLI·호스트 MCP 스모크

이슈 #303·#304. macOS ARM64에서 실행했다. 운영 최종 소스는 `fb6aac46df5fd5d18658721859c4be0fe793fe65`, 릴리스는 `20261104T071435Z-main-fb6aac4`다. 테스트 중 운영이 `d86d526`으로 갱신되어 소스 설치·로컬 핵심 스모크를 그 버전으로 다시 실행하고, 발견한 Muse 문제를 수정한 최종 운영 버전에서 세 호스트를 모두 재검증했다.

## 입력과 통과 기준

| 경로 | 입력·기대 결과 | 결과 |
| --- | --- | --- |
| 메모리 생성 | 격리된 Chrome에서 기여 제출 → fixture 검수·replay → 게시, catalog·recall·L2에 동일 ID/버전 노출 | 통과 |
| 메모리 개정 | 같은 메모리 재게시 → 기존 버전 보존, 새 version_id 발급, 최신 조회에 새 버전 반환 | 통과 |
| 수명주기 | 철회·동의·계정 삭제, 영수증 소유권·중복 처리·재시작 후 내구성 | Chrome 3개 및 HTTP 계약 3개 통과 |
| CLI | 소스 설치한 `wam`으로 상태·양성 recall·버전 고정 L2 expand·매칭 없는 URL 조회 | 통과 |
| 오류 추적 | 웹·CLI·MCP 오류의 응답/stderr trace_id와 진단 DB 행 일치 | 통과 |

생성된 fixture 메모리는 `web:example-org-compose-draft`다. 첫 버전 `ver-ed1ff3a164afe102257008d257888f1023ba6e6b7f7bad84`와 새 버전 `ver-aa8db8efc4b1153cfa292a91756b4a1ccd5b144b70ede8a0`가 달라야 한다는 검증을 통과했다.

## 설치와 실제 호스트 호출

`wam`·`wam-mcp`는 소스에서 `cargo install --locked --offline`로 소유 임시 경로에 설치했다. `initialize`·도구 10개 발견·stdio recall·정상 EOF를 확인했다. 설치 검증 기준 제품 코드는 `d86d526`이며, 이후 수정은 공유 MCP catalog와 회귀 테스트뿐이다.

| 호스트 | 설치·등록 확인 | 최종 공개 MCP 호출 |
| --- | --- | --- |
| Claude Code 2.1.317 | HTTP·stdio 등록, 반복 등록, 조회, 제거, 다른 등록 보존 | status → recall → expand 성공 |
| Codex CLI 0.166.0 | HTTP·stdio 등록, 반복 등록, 조회, 제거, 다른 등록 보존 | status → recall → expand 성공 |
| Muse Code 1.4.14 | 공식 설치 스크립트, 격리 settings의 원격 MCP 설정·제거·복원 및 실제 재연결 | status → recall → expand 성공 |

Claude·Codex의 로컬 전송 검증은 실제 호스트 실행 파일과 실제 WAM 서버를 연결하고 모델 응답만 loopback fixture로 제공했다. HTTP recall·stdio expand가 모델에 전달됐는지와 서버의 성공 진단 행을 별도로 확인했다.

공개 서비스 검증은 실제 모델을 사용했다. Codex는 로그인된 OpenAI 계정의 `gpt-6-luna`/max, Claude Code와 Muse Code는 Meta API의 `muse-spark-1.3`을 사용했다. 각 호스트의 도구 결과를 직접 읽어 다음 값을 공개 catalog와 비교했다.

- URL: `https://www.bing.com/webmasters/`
- memory_id: `web:example-bing-webmaster-add-site`
- version_id: `ver-b70ebf360125dae8a60cfb87332655a49586fbe878e6d538`
- expand: 숫자 level `2`, 같은 ID·버전, 비어 있지 않은 steps
- 익명 recall: 영수증 없음

## 발견·수정

`d86d526`의 expand 스키마에서 `level.type`이 빠져 있었다. 같은 Muse·모델·입력으로 wire를 관측했을 때 enum-only 스키마는 문자열 `"2"`, `type:integer`를 명시한 스키마는 숫자 `2`를 보냈다. 전자는 `level must be 1 or 2` 오류를 반복했다.

`fb6aac4`는 정수 타입을 복원하고 설명을 압축해 기존 catalog·instructions의 3,200바이트 예산을 유지했다. 잘못된 문자열·소수·불리언 입력을 거부하는 parser는 유지했다. `tools/list` 계약 테스트의 실패를 먼저 입증한 뒤 수정했고, 운영 Muse의 실제 status·recall·expand와 L2 본문까지 다시 통과했다.

스모크의 오류 기대값도 `1cf739b`에서 현행 계약에 맞췄다. 잘못된 UTF-8 프레임은 JSON-RPC `-32700` 오류를 반환하고 계속 실행한다. 실제 stdout I/O 실패는 종료 코드 1과 stderr trace를 남긴다. CLI `--json` 오류의 trace_id는 stderr·DB와 일치한다.

등록·연결 사전 요청까지 한꺼번에 보내면 익명 제한(분당 60 units, burst 20)으로 429가 발생했다. 등록과 기능 호출을 나눠 재검증했으며 운영 제한은 바꾸지 않았다. Claude stream-json의 tool_result.content는 JSON 문자열 또는 text 블록일 수 있어, 최종 로그 검증기는 두 형식을 모두 읽었다.

## 검증·배포·증빙

- 클라이언트·CLI·MCP 테스트: 99개 통과, 1개 ignored.
- 타입 수정 후 MCP·stdio·HTTP 테스트: 32개 통과. format·Clippy 통과.
- Linux 배포 검사: 441개 통과, 31개 ignored. 일회용 PostgreSQL 검증과 release 빌드 통과.
- 운영 활성화 전 DB 백업, 활성 릴리스 경합 확인, 실패 시 롤백 절차 사용.
- 공개 health·웹 번들 해시·MCP 정수 스키마 확인. 메모리 38개·공식 점수 880·설정 파일 8개의 해시 보존.
- 기존 호스트 설정 파일 5개의 해시 보존. 소유 테스트 서버·fixture·임시 Muse 설치 정리.
- 소유 증빙 파일 283개에서 사용한 Meta 키 원문을 검사했고 일치 0건.

요약 증빙: 작업 공간의 `build/smoke-20261104-303/summary.json`. 세부 로그·독립 검증 결과는 별도 작업 트리의 `build/host-smoke-303/`에 보존했다. 소스 설치 검증기는 [verify_native_install.py](../../tests/env/verify_native_install.py), 로컬 핵심 실행 절차는 [local-smoke.md](local-smoke.md)를 따른다.

범위: 생성·검수·OAuth·replay는 격리 fixture였다. 운영에서는 익명 읽기를 검증했다. 실제 WAM 계정 로그인·운영 게시, Muse 로컬 stdio, 서명 릴리스 설치와 다른 OS의 설치는 이번 검증에 포함하지 않았다. Claude Code 검증의 모델 제공자는 Meta다.
