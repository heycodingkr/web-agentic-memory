# webagenticmemory.com 배포

**현재 운영은 [4090 서버 직접 배포](4090/README.md)**다. 웹과 API의 DNS only A 레코드가 4090 공인 IPv4를 가리키고, 해당 서버의 Caddy가 HTTPS를 직접 처리한다. 기존 Mac Tunnel stack은 중지했다. 실제 설치·서비스·백업·검증은 4090 절차를 따른다.

아래 `Caddyfile`·`wam-server.service`·env 예제는 새 Linux 단독 호스트에 설치할 때의 참고 설정이다.

## 준비할 입력

- 배포할 Linux 호스트의 주소·접속 권한. `/opt/web-agentic-memory`에 배포할 소스를 고정하고 다른 세션의 미완료 변경은 제외한다.
- Cloudflare `webagenticmemory.com` DNS 편집 권한. 현재 운영의 apex/api A 레코드와 기존 메일 DNS를 보존하고 대상 호스트를 검증한 뒤 변경한다.
- 전용 PostgreSQL과 공유 Redis 연결. 원격 연결은 인증서 검증 TLS를 사용한다. 기존 [운영 배포 절차](../docs/operations/deployment.md)의 backup/restore와 worker 요구사항을 따른다.
- `credentials/email-auth.env`의 서버 전용 이메일 설정. 생성한 `WAM_EMAIL_IDENTITY_SECRET`은 이메일 계정 식별에 쓰이므로 백업하고 임의 교체하지 않는다. 진단 알림 수신 주소는 `WAM_DIAGNOSTICS_EMAIL_TO`로 지정하며 예제 값은 `easycodingkr@gmail.com`이다. 저장소에는 비밀값을 넣지 않는다.

## 설치

1. 대상 서버에서 lockfile을 고정해 `cargo build --locked --release -p wam-server --bin wam-server`로 API를 빌드한다. `VITE_SITE_URL=https://webagenticmemory.com VITE_WAM_MCP_ENDPOINT=https://webagenticmemory.com/mcp pnpm --dir web build`로 웹을 빌드한다. Node와 Rust는 해당 lockfile을 지원하는 도구를 사용한다.
2. `deploy/server.env.example`을 `/etc/wam/server.env`로 복사하고 빠진 DB/Redis/secret을 채운다. `WAM_RATE_LIMIT_KEY_SECRET`도 별도 32바이트 이상 난수로 생성한다. 파일 권한은 600으로 제한한다. 진단 메일은 `WAM_DIAGNOSTICS_EMAIL_TO`로 보내며 `RESEND_API_KEY`와 `WAM_EMAIL_FROM`을 재사용한다. 실제 Resend 키와 생성된 email identity secret은 비공개 전송 경로로 옮긴다. GitHub OAuth는 선택 설정이다.
3. API 실행 전용 `wam` 사용자와 그룹을 만들고 API 바이너리를 실행·읽기 가능하게 설치한다. `deploy/wam-server.service`를 `/etc/systemd/system/wam-server.service`에 설치하고 `systemctl daemon-reload`, `systemctl enable --now wam-server`를 실행한다. 애플리케이션 계정은 새 사용자로 실행하며 root로 실행하지 않는다.
4. Caddy를 설치하고 `deploy/Caddyfile`을 적용한다. Caddy 실행 사용자에게 `/opt/web-agentic-memory/web/build` 읽기 권한을 준다. `caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile`로 검사한 뒤 reload한다. API는 loopback 3000에서만, HTTPS는 443에서 공개한다. ACME HTTP 검증을 위해 80도 허용한다.
5. Cloudflare에 apex A 레코드를 서버의 실제 IPv4로 추가한다. 초기 구성은 DNS only로 HTTPS를 확인한다. 프록시를 켜려면 Cloudflare의 TLS와 신뢰 proxy chain, 실제 client IP, 인증 cache 제외를 별도로 검증한다. 임의 IP·기존 MX/SPF 레코드를 덮어쓰지 않는다.
6. Resend 도메인 상태를 확인하고 Resend가 제공한 DKIM·SPF/MX를 Cloudflare DNS에 등록·verify한다. 발송 전용 key로는 도메인 조회/등록·DNS 편집을 할 수 없다. 공식 테스트 수신 주소로 API 접수가 성공해도 실제 사용자 수신·도메인 상태는 별도로 확인한다.

## 배포 후 확인

`https://webagenticmemory.com/v1/auth/config`가 email_auth_enabled=true인 JSON을 반환해야 한다. `/login`의 실제 이메일 수신·코드 검증·세션 유지·로그아웃을 확인하고, `/oauth/approve`가 정적 웹으로 연결되는지와 `/mcp` discovery를 검사한다. 배포 후 가입/로그인/미수신·복구 안내/세션/앱 연결은 [운영 수용 케이스 EMAIL-PROD-01~34](../docs/test/email-auth-production.md)를 순서대로 실행한다. 비밀번호는 없으므로 비밀번호 찾기·재설정 기능은 제공하지 않는다. 자세한 실패/보안 검사와 이전 로컬 결과는 [이메일 테스트 기록](../docs/test/email-auth.md)과 [운영 배포 절차](../docs/operations/deployment.md)를 따른다.

이 설정에는 contribution publication worker가 포함되지 않는다. 기여 자동 게시까지 공개하려면 실제 model/replay와 별도 worker를 기존 운영 절차에 따라 연결한다.
