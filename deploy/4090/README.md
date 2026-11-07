# 4090 서버 직접 배포

최신 배포는 2026-11-07의 `20261107T063922Z-main-4f098de`이며 [문의 메일 푸터 배포 기록](../../docs/test/support-footer-production-4090.md)을 따른다. 이전 [MCP recall 검증 사유 추적 배포 기록](../../docs/test/recall-invalid-input-production-4090.md), [로그인 네트워크 오류 복구 배포 기록](../../docs/test/auth-network-production-4090.md), [Canva benchmark report 영어화 배포 기록](../../docs/test/canva-report-english-production-4090.md), [브라우저 오류 추적성 개선 배포 기록](../../docs/test/telemetry-traceability-production-4090.md), [Trip.com 항공권 검색 메모리와 예비 관찰 배포 기록](../../docs/test/trip-flights-production-4090.md), [URL 기반 공유 메모리 소개·히어로 배포 기록](../../docs/test/url-memory-copy-production-4090.md), [SEO 메모리·검색 최적화 통합 배포 기록](../../docs/test/integrated-seo-production-4090.md)도 보존한다.

현재 운영은 SSH 별칭 `4090`의 Ubuntu x86_64 서버다. 웹과 API는 서버의 Caddy가 HTTPS로 직접 제공한다. Cloudflare의 apex/API 레코드는 DNS only인 A 레코드이며 Tunnel을 사용하지 않는다.

| 주소 | 서버 내부 대상 |
|---|---|
| https://admin.webagenticmemory.com | 관리자 이메일 로그인·기여 검수 (동일 HTTPS origin의 `/v1/admin/*`) |
| https://webagenticmemory.com | Caddy의 `/srv/web-agentic-memory/current/public` |
| https://webagenticmemory.com/v1/* | `127.0.0.1:4090` API |
| https://api.webagenticmemory.com | `127.0.0.1:4090` API |
| https://api.webagenticmemory.com/mcp | 공개 MCP endpoint |

## 현재 설치

- API: `wam-api.service`, 전용 비관리자 OS 계정 `wam`.
- PostgreSQL: `wam-postgres`, 전용 DB `wam`과 비관리자 role `wam_app`, loopback 55439.
- Redis: `wam-redis`, 비밀번호 인증·AOF, loopback 6399.
- HTTPS: 기존 `caddy-front.service`에 표시된 WAM 블록만 추가. 인증서 발급은 Cloudflare DNS challenge를 사용한다.
- DDNS: `wam-ddns.timer`가 5분마다 apex·api·admin의 세 A 레코드만 확인한다. 레코드가 CNAME/proxied 모드로 바뀌면 임의로 덮어쓰지 않는다.
- 관리자 인증: Git에서 제외되는 로컬 `credentials/admin.env`를 `/etc/web-agentic-memory/admin.env`에 권한 600으로 설치한다. `WAM_ADMIN_EMAIL`은 단일 허용 이메일, `WAM_ADMIN_ORIGIN`은 `https://admin.webagenticmemory.com`이다. systemd의 `wam-api.service.d/admin-auth.conf`에서 추가 `EnvironmentFile`로 읽는다. 이메일 주소는 저장소·프런트엔드 빌드에 넣지 않는다.
- 운영 오류 요약 메일: `/etc/web-agentic-memory/api.env`의 `WAM_DIAGNOSTICS_EMAIL_TO`를 `easycodingkr@gmail.com`으로 설정한다. 기존 `RESEND_API_KEY`와 `WAM_EMAIL_FROM`을 사용하며, 이메일 로그인 secret은 진단 알림만 사용할 때 필요하지 않다.
- 비밀값: `/etc/web-agentic-memory/` root 전용. 이메일 identity secret은 기존 운영 값을 유지한다.
- 릴리스·해시: `/srv/web-agentic-memory/releases/<release>/deployment.json`, `source-hashes.json`. `current` 심볼릭 링크가 활성 릴리스를 가리킨다.

최초 배포는 2026-11-01의 `20261101T065946Z`다. 작업 트리 스냅샷을 Linux에서 빌드했으므로 git HEAD만으로 동일 artifact를 재현했다고 가정하지 않고 파일 해시를 확인한다. Rust builder는 캐시된 image digest `rust@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97`로 고정했다.

## 관리자 로그인과 검수

`https://admin.webagenticmemory.com`에서 허용 이메일을 입력하면 로그인 버튼이 포함된 메일을 보낸다. 10분 안에 요청한 브라우저에서 열고 로그인 완료를 누른다. 링크는 한 번만 사용할 수 있고 관리자 전용 세션은 2시간 뒤 만료된다. 일반 고객 이메일 로그인·GitHub·CLI 토큰은 관리자 권한을 주지 않는다.

관리자는 상태별 기여 목록·검색·계정별 요청 상세·제안 메모리와 재실행 결과·처리 이력을 볼 수 있다. 검수 시작 후 승인 또는 반려를 확인해서 처리한다. 검증된 제안이 없는 승인은 자동 검증을 요청하며 검증을 생략해 게시하지 않는다. 기존 `/v1/review/*`도 같은 관리자 권한·origin·CSRF 검사를 적용한다. 운영 환경에서는 기존 개발용 review token을 허용하지 않는다.

`admin-dns.py`는 apex의 직접 IPv4를 읽어 admin A 레코드만 준비한다. 기본 실행은 사전 확인이며 `--apply`가 DNS를 반영한다. 메일 DNS와 apex/api 레코드는 변경하지 않는다. 새 관리자 호스트는 Caddy의 WAM marker 안에서 추가한다.

## 상태 확인

```sh
ssh 4090 'sudo systemctl status wam-api wam-ddns.timer --no-pager'
ssh 4090 'sudo journalctl -u wam-api -n 50 --no-pager'
ssh 4090 'sudo docker ps --filter name=wam'
dig +short api.webagenticmemory.com A
curl --fail https://api.webagenticmemory.com/healthz
```

API stdout/stderr는 `journalctl -u wam-api`, HTTPS는 `/var/log/caddy/caddy.log`, DDNS는 `journalctl -u wam-ddns`에서 확인한다. 운영 환경 전체나 credential 파일을 로그에 출력하지 않는다.

## 재배포와 복구

새 소스를 별도 경로로 고정하고 동일 Linux builder에서 build·email/OAuth/MCP/shutdown 및 disposable PostgreSQL 검사를 수행한다. 프런트엔드는 production origin으로 빌드하고 샘플 데이터·server secret 부재를 검사한다. 새 release 디렉터리를 만들고 기존 `api.env`와 DB를 유지한 채 `current`를 전환한 다음 API를 재시작한다. 기존 release와 DB backup은 검증 후에도 보존한다.

`setup-private-config.py`와 `create-database.py`는 최초 provisioning용이다. `cutover.py`는 이전 Mac 운영 DB를 최종 백업·복원하고 DNS를 전환한 일회성 이관 도구다. 이미 생성된 production-final.dump가 있거나 이전 API가 없으면 신규 API를 정지하기 전에 거부한다. 이후 배포에 이 도구를 재실행하지 않는다.

공유 Caddy 변경은 `prepare-caddy.py`가 기존 파일을 백업하고 WAM marker 내부만 교체한다. 검증 후 reload하며 다른 vhost를 중단하지 않는다. 이전 Caddyfile은 `/etc/caddy/Caddyfile.pre-wam-*`에 남아 있다.

`cloudflare-dns.py`는 해당 zone을 확인한 뒤 apex와 api의 address record만 변경하며 MX/TXT/CAA를 보존한다. 첫 DNS backup은 작업 호스트의 `build/deploy-4090/dns-before.json`이다. 이관 후 새 계정이나 기여가 생겼다면 이전 서버로 DNS를 되돌리기 전에 최신 DB를 안전하게 동기화해야 한다. API artifact 롤백도 schema 호환성을 먼저 확인한다.

이전 Mac의 WAM api/web/redis/tunnel LaunchAgent는 중지·비활성화했다. 공유 PostgreSQL이나 다른 프로젝트 서비스는 중지하지 않았다. 실제 provider·독립 replay worker와 기여 결과 owner API의 운영 전체 여정 수용은 #159/#162의 후속 작업이다.
