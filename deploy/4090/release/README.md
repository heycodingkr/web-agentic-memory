# 운영 릴리스 절차 (API + 정적 웹)

[`../README.md`](../README.md)의 "재배포와 복구"를 한 커밋 단위로 실행하는 스크립트와 점검이다. 2026-11-01에 `20261101T140344Z-main-301c5a1`을 이 절차로 배포했다. 비밀값은 어느 스크립트에도 없다. 주소·키는 호스트의 `/etc/web-agentic-memory/`에만 있고 스크립트는 그것을 출력하지 않는다.

## 먼저: 운영에만 있는 코드가 없는지 확인한다

릴리스는 작업 트리 스냅샷에서 만들어진 적이 있어서(`source-hashes.json`의 "verified working-tree snapshot") 운영에 올라간 파일이 어떤 커밋에도 없을 수 있다. 깨끗한 커밋을 그대로 올리면 그 기능이 조용히 사라진다. 실제로 `20261101T123749Z-memory-authors`(공식 작성자, #205)가 커밋 없이 운영 중이었고, 이를 합치지 않고 `origin/main`을 올렸다면 작성자 표시와 리더보드가 사라졌을 것이다.

```sh
deploy/4090/release/compare-with-production.py <commit>      # 저장소 안에서 실행. 운영에만 있는 파일이 있으면 종료 코드 1
```

`ONLY IN PRODUCTION`이 0이 될 때까지 배포하지 않는다. 그 파일은 커밋하거나(소유 세션의 변경이면 그 세션과 확인) 빌드할 트리에 합친다. 운영 호스트의 `/srv/web-agentic-memory/builds/<release>/source`에 해당 릴리스의 Rust 소스가 남아 있다.

## 단계

1. 배포할 커밋을 한 개로 고정한 깨끗한 checkout에서 `package.sh <commit> <out>`: Linux 빌드용 소스, production 웹 빌드(`VITE_SITE_URL`, `VITE_WAM_MCP_ENDPOINT`), 해시 목록을 만든다. 다른 세션의 미완료 작업을 함께 올리지 않는다.
2. 호스트의 `/srv/web-agentic-memory/builds/<release>/`에 올리고 웹 아카이브를 `public/`에 풀고 `expected-previous.txt`에 지금 활성인 릴리스 경로를 적는다(`readlink -f /srv/web-agentic-memory/current`).
3. `build-remote.sh <release>`: 호스트의 고정된 builder 이미지에서 `wam-server`·`wam-gatekeeper`·`wam-core`·`wam-contracts` 테스트, 일회용 PostgreSQL의 ignored 테스트, release 빌드를 한다. 활성화는 하지 않는다. `DEPLOY_BUILD_AND_TESTS_PASSED`가 나와야 한다.
4. `sudo -n python3 activate-remote.py <release>`: 공유 배포 잠금(`.deploy.lock`)을 잡고, 다른 릴리스가 활성화되지 않았는지 확인하고, 운영 DB를 `pg_dump`(`/var/backups/web-agentic-memory/<release>/`)하고, 새 release 디렉터리를 만들고(이전 빌드의 `_app/immutable`을 함께 둬서 열려 있는 페이지가 깨지지 않게 한다), `current`를 바꾸고 API를 재시작한 뒤 점검한다. 점검이 하나라도 실패하면 이전 릴리스로 되돌린다.
5. 활성화 점검: `/healthz`, 관리자 host의 session·목록 401과 review 403, 공식 작성자 카탈로그·점수 일관성(공식 메모리 수×20점), 정적 페이지 여덟 개의 핵심 문구.
6. 관리자 로그인 링크를 바꾼 릴리스는 `sudo -n bash -s < admin-outbox-smoke.sh`를 호스트에서 실행한다. 설정된 관리자 주소로 로그인 메일 한 통이 간다(주소는 출력하지 않는다). 전달 항목이 사라지고 challenge가 남으면 제공자가 접수한 것이다.

## 되돌리기

활성화 스크립트는 점검 실패 때만 자동으로 되돌린다. 나중에 되돌릴 때는 호스트에서 이전 릴리스로 `current`를 바꾸고 `systemctl restart wam-api`를 한다(스키마 호환을 먼저 확인한다). 이전 release와 `production-before.dump`는 보존된다.

## 이 환경에서 알아 둘 것

- `build-remote.sh`는 컴파일 캐시로 `builds/20261101T075251Z-admin/target`을 재사용한다.
- 운영은 `WAM_EXAMPLES=on`이다. 새 바이너리는 시작할 때 번들 예제 중 DB에 없는 것만 추가하고(기존 행은 건드리지 않는다) 공식 작성자에게 +20을 기록한다. 기존 예제의 내용 개정(예: Notion 메모리)은 `WAM_EXAMPLES=refresh`로 한 번 시작해야 반영되며 이 절차에는 포함하지 않는다.
- `WAM_RUN_WORKER`와 모델 설정이 없으므로 운영 API는 큐레이션 모델 worker를 돌리지 않는다.
