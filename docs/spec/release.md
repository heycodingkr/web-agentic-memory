# Release candidate와 증거 판정 계약

상태: 2026-10-29 첫 계약과 로컬 구현. 이 도구의 판정은 입력 증거의 coverage/provenance/file integrity 검사이며, 실제 시험의 진실성·권리 검토·배포 승인을 새로 부여하지 않는다.

## 목표

공개 client 바이너리와 hosted service 출시 전에 필요한 시험을 빠뜨리지 않고, 다른 commit·dirty tree·합성 대역·누락 결과를 실제 release candidate의 통과로 취급하지 않는다. 정확한 case와 절차는 [`../test/release-readiness.md`](../test/release-readiness.md), 기계 정책은 [`../test/release-gates.json`](../test/release-gates.json), 임계값은 [`../test/metrics.md`](../test/metrics.md)를 따른다.

## 요구사항

| ID | 요구사항 | 관련 테스트 |
|---|---|---|
| F-REL-01 | 전체 Git commit SHA와 policy SHA256에 증거를 바인딩한다. CLI는 실제 HEAD와 clean working tree를 확인한다. 각 실행의 source commit과 dirty 여부도 일치해야 한다 | REL-01/13 |
| F-REL-02 | `client`/`service` 프로필과 선택한 `--claims`의 필수 case가 모두 pass여야 한다. pending/blocked/skipped/fail, 중복/unknown case, 잘못된 schema/duplicate JSON key는 pass가 될 수 없다 | REL-12/13/14 |
| F-REL-03 | 실제 host/native/staging/manual 결과가 필요한 gate를 synthetic fixture나 다른 종류의 실행으로 대체하지 않는다. 관측 UTC는 14일 이내이고 미래 허용은 최대 5분이다. 수동 검토에는 reviewer를 기록한다 | REL-03~11/13 |
| F-REL-04 | artifact는 manifest 폴더 안의 일반 파일이며 SHA256가 같아야 한다. traversal, absolute path, symlink, 빈/누락 파일, 64 MiB 초과를 거부한다. 원문 credential, tool argument/result, 비공개 계정 자료를 판정 출력에 echo하지 않는다 | REL-07/13 |
| F-REL-05 | `--init`은 pending template만 mode 0600/exclusive creation으로 만들고 기존 파일을 덮어쓰지 않는다. 판정은 0(pass), 1(blocked), 2(invalid input)를 반환하며 publish를 수행하지 않는다 | REL-13 |
| F-REL-06 | 로컬 release/package/installer는 Linux x64/ARM64, macOS Intel/ARM64, Windows x64의 같은 타깃 집합을 사용한다. 실제 runtime·signed artifact 설치는 별도 증거로 요구한다 | REL-03/04/06/14 |
| F-REL-07 | 공개 release에는 LICENSE 원문과 reference 권리·보안 신고 경로·공개 범위 검토가 필요하다. 임의의 라이선스/검토자/실계정 성공을 생성하지 않는다 | REL-11/13 |
| F-REL-08 | 로컬 데이터 서비스 시험은 새 cluster/loopback port/전용 임시 경로를 만들고 PostgreSQL data_directory/Redis PID를 검증한 뒤 URL을 전달한다. 자신의 process group과 데이터만 정리하고 timeout/missing tool을 pass로 계산하지 않는다 | REL-01/08/13 |

## 인터페이스

`python3 tools/release/readiness.py --init <new.json> --candidate <sha> --profile client|service`는 template를 만든다. `--evidence <json>`은 증거를 검사하고, `--json`은 안전한 gate/case/reason 보고서를 출력한다. `--claims`는 성능 주장용 추가 case를 활성화한다. 사용자 입력으로 gate를 끄거나 signature 검사를 우회하는 옵션은 없다.

각 record에는 gate/case/status/source_commit/dirty/kind/fixture_only/observed_at/artifact/sha256가 있으며 manual에는 reviewer가 추가된다. 정확한 closed schema와 boundary failure 검사는 `tools/release/readiness.py`, `tests/lib/selftest/test_release_readiness.py`가 구현한다. 새로운 요구사항은 policy와 이 명세 및 테스트에 함께 반영하고 candidate 실행 전에 고정한다.

## 실증 범위

자동 판정은 metadata와 artifact 무결성을 확인한다. 시험이 적절한 oracle·실제 환경·독립 라벨로 수행됐는지는 원본 증거를 검토한다. 현재 개발 트리의 로컬 테스트 결과를 clean release candidate의 검증으로 자동 승격하지 않는다. draft 생성 이후 실제 공개 전에도 같은 candidate와 증거를 다시 확인한다.
