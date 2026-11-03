# Trip.com 항공권 검색 메모리와 예비 관찰 배포

2026-11-03 17:11 KST(08:11Z), Trip.com 항공권 검색 예제 메모리와 README·랜딩의 예비 관찰 표시를 4090 운영 서버에 배포했다.

## 배포 결과

| 항목 | 결과 |
| --- | --- |
| 운영 주소 | https://webagenticmemory.com/ · https://webagenticmemory.com/ko · https://webagenticmemory.com/benchmarks/trip-flights.md |
| 릴리스 | `20261103T080921Z-main-17dff80` |
| 배포 소스 | `17dff809d29622165fee3dc39173f53bd3407ac6` |
| 이전 릴리스 | `20261102T050515Z-main-6c32d42` |
| API 바이너리 | sha256 `d4e401ea6db1049e3e0d6983ad95a82bbd357f1843012a506feb7eba6c5b05bb` |
| 공개 메모리·공식 점수 | 37개·860점 → 38개·880점. 기존 memory/version ID 37개 전부 유지, 추가는 `web:example-trip-flight-search` 하나 |
| DB schema·운영 환경 설정 | 변경 없음. `/etc/web-agentic-memory/`의 설정 파일 8개는 이름과 해시만 비교했고 전후 동일 |

제품 변경은 세 가지다. 기본 seed에 Trip.com 항공권 검색 예제(`Example:`)를 넣었고, README(한·영)·랜딩(한·영)·`llms.txt`에 같은 작업의 개별 실행을 "예비 관찰, 비교 아님"으로 표시했으며, 보고서를 `/benchmarks/trip-flights.md`로 제공한다. 이 메모리는 [보고서](benchmarks/trip-flights.md)의 사전 등록 규칙(M 두 번 통과, 짝 T보다 느리지 않을 것)을 충족하지 못했고 소유자 결정으로 포함했다. 속도 향상 주장은 하지 않는다. 같은 구간(`6c32d42..17dff80`, 12개 커밋)의 나머지는 문서·테스트·데모 변경이며 마이그레이션과 `Cargo` 변경은 없고, `wam-server` 소스 변경은 `examples.rs` 두 줄뿐이다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 운영 소스 보존 | `compare-with-production.py 17dff80`: 운영 전용 미커밋 파일 0개. 운영 파일 17개는 모두 이 커밋이 더 새로운 버전 |
| 깨끗한 checkout·패키징 | `17dff80`의 새 clone에서 `package.sh`. 623개 파일 해시 목록 |
| 웹 검사(배포 전) | node 단위 72/73(실패 1개는 이번 변경 이전부터 깨진 `demo/video/stage.html` 단언), svelte-check 오류 0개, 랜딩 Chrome 검사 10/10 |
| 고정 Linux builder | 59개 suite, 437 pass, 0 fail, ignored 31개. 일회용 PostgreSQL application·email auth ignored 검사와 release 빌드 포함. 새 `travel_examples` 4개 포함 |
| 업로드 무결성 | 소스·웹 archive, manifest, 빌드·활성화 스크립트의 SHA-256 전부 일치 |
| 공개 artifact 점검 | 서버 비밀값 이름 0개. 로컬 개발 주소는 설치 가이드 본문의 `127.0.0.1:3000` 안내뿐이며 기존 운영 빌드와 같다 |
| 활성화 점검(스크립트) | API readiness, 관리자 session·목록 401, review 403, 공식 작성자·점수 일관성(880점, 20의 배수, 38개×20 이상), 정적 페이지 8개 핵심 문구 통과 |
| 운영 API 시작 로그 | `built-in example memories: 1 added`, 오류·경고 없음 |
| 운영 보존 | 기존 메모리 버전 전부 유지, 제거 0개, 설정 파일 해시 동일, `wam-api`·`caddy-front` 활성 |
| 공개 HTTPS | `/`·`/ko`에 예비 관찰 블록, `/benchmarks/trip-flights.md`·`/llms.txt`·`/guide`·`/memories` 200과 핵심 문구, API health 200 |
| 운영 recall | `https://www.trip.com/flights/`(research, `flights`) → 새 메모리 1건과 L1. `https://www.trip.com/hotels/`와 `www.trip.com.evil.example.org`는 0건 |
| 운영 Chrome 검사 | 랜딩 spec 10/10(영·한 × 1440·390·320px, API 모의). 새 블록과 보고서 경로 검사 포함 |

## 증거와 복구

소스·웹 archive, 빌드 로그와 요약, 배포 전 기준선(`pre-deploy-baseline.json`), 활성화 로그, 배포 후 점검(`post-deploy-check.json`)은 서버 `builds/20261103T080921Z-main-17dff80/`에, 작은 파일은 gitignored `target/deploy-trip-flights-production-20261103/`에 보관한다. 운영 DB 백업은 root 전용 `/var/backups/web-agentic-memory/20261103T080921Z-main-17dff80/production-before.dump`다.

활성화 스크립트는 `.deploy.lock`을 잡고 예상한 이전 릴리스와 바이너리 해시를 확인한 뒤 실행했다(이번에는 `deployment.lock`을 따로 잡지 않았다). 이전 릴리스와 기존 정적 immutable asset을 보존했고, 점검 실패 시 이전 릴리스로 자동 복귀하는 절차를 사용했다. 실제 활성화와 후속 점검은 모두 성공했고 복귀는 일어나지 않았다.

복구가 필요하면 [운영 릴리스 절차](../../deploy/4090/release/README.md)에 따라 이전 릴리스로 `current`를 전환하고 API를 재시작한다. 이 릴리스는 DB schema를 바꾸지 않았다. 메모리 `web:example-trip-flight-search`는 DB에 한 행으로 추가되었으므로 이전 릴리스로 돌아가도 남는다. 공개 목록에서 빼야 하면 DB 백업을 복원하기 전에 그 행만 내릴 방법이 있는지 먼저 확인한다(복원하면 그 뒤의 데이터가 사라진다).
