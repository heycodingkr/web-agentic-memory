# 4090 직접 배포 검증

2026-11-01 웹·이메일 인증 API·My Page를 4090 서버에 직접 배포했다. apex와 api의 기존 Tunnel CNAME을 DNS only A 레코드로 바꾸고 이전 Mac의 WAM Tunnel stack을 중지했다. 실제 이메일 수신과 신규/기존 로그인·로그아웃을 Arise에서 확인했다. 전체 EMAIL-PROD-01~34나 기여 worker 수용 완료를 뜻하지 않는다.

## 배포 구성과 증거

| 항목 | 확인 결과 |
|---|---|
| 웹 | https://webagenticmemory.com |
| API | https://api.webagenticmemory.com |
| 직접 DNS | 두 hostname 모두 A 121.167.167.102, proxied=false, TTL 60 |
| HTTPS | 두 hostname의 공개 인증서 정상, HTTP→HTTPS. 공개 resolver 요청의 API peer 121.167.167.102·200 |
| 릴리스 | 20261101T065946Z, Linux x86_64, API SHA-256 8e9f1bd060ef740ca4558f0af6b41457a05a52091b87080e512f99f552e044a8 |
| 소스 기록 | 작업 트리 snapshot. 서버 release의 source-hashes.json에 212개 파일 hash 보존 |
| 영속성 | 전용 PostgreSQL·비관리자 role, Redis 인증/AOF. 이전 DB backup·restore와 이메일 identity secret 유지 |
| 상시 실행 | wam-api.service, Docker restart 정책, wam-ddns.timer enabled |
| 이전 Mac 서비스 | WAM api/web/redis/tunnel LaunchAgent 중지·비활성화. 공유 PostgreSQL 유지 |

배포 시점 계정·메모리·기여·원장 기준값은 모두 0이었다. 실제 메일 인증으로 소유 QA 계정 1개를 만들고, 로그아웃 후 두 번째 실메일 코드로 재로그인해 계정 수가 1개로 유지됨을 확인했다. 메모리·기여·점수 원장은 0이다. 공개 프로필 설정이나 기여 동의를 임의로 켜지 않았다.

API stdout/stderr는 4090의 `journalctl -u wam-api`, Caddy 로그는 `/var/log/caddy/caddy.log`, DDNS는 `journalctl -u wam-ddns`에 있다. 실행 명령·복구·비밀값 경로는 [4090 배포 절차](../../deploy/4090/README.md)를 따른다. 배포 호스트의 `build/deploy-4090/`에는 build/API/PostgreSQL/public-smoke 결과와 보호된 DB·DNS backup이 있다.

## 실행한 검사

| 검사 | 결과 | 범위 |
|---|---|---|
| Linux API email/OAuth/MCP/shutdown | 26건 통과 | 배포와 같은 고정 소스·Rust image에서 release 검사 |
| disposable PostgreSQL 이메일 | 1건 통과 | 운영 DB와 분리. 공유 발송 제한·동시 코드 소비·세션/계정 재조회, 완료 후 테스트 DB 삭제 |
| production 웹 build | 통과 | 실제 origin/MCP URL, dummy preview off |
| public artifact secret 검사 | 통과 | 현재 Resend/identity/rate-limit secret 값이 공개 파일에 없음 |
| DNS 변경 | 통과 | 두 address record만 변경. 기존 MX/TXT/CAA 유지 |
| 공유 ingress 보존 | 통과 | WAM 블록을 제외한 기존 Caddyfile 내용의 공백 정규화 비교 일치 |
| HTTPS·라우팅 | 통과 | 두 공개 인증서, login/deep link, API health·JSON 401, 직접 origin marker 4090 |
| 공개 MCP/API smoke | 통과 | initialize, tools/list 10개, 익명 status/recall, OAuth metadata |
| 실제 최초 가입 | 통과 | Resend 요청→Gmail 수신→같은 Arise 브라우저 코드 검증→My Page 자동 이동 |
| 실제 세션 유지 | 통과 | dashboard 전체 새로고침 후 로그인 상태 유지 |
| 실제 로그아웃 | 통과 | 계정 메뉴 Sign out→login 이동→dashboard 직접 접근 시 비로그인 안내 |
| 실제 기존 계정 로그인 | 통과 | 두 번째 수신 코드로 성공. 계정 수 1개 유지 |
| 일회성 이관 재실행 방지 | 통과 | cutover 재실행은 변경 전 거부, 공개 API 계속 200 |

코드·이메일 주소·session/CSRF·API key는 공개 증거에 저장하지 않았다. 실제 가입 요청·수신·로그인·로그아웃 관찰 시각은 비공개 실행 기록에 두며, 사용자 메일이 받은편지함에 실제 도착한 것을 확인했다. 운영 코드를 고정하거나 서버 저장소에서 추출하지 않았다.

[실제 운영 My Page 캡처](../design/assets/my-page-production-4090.png)는 새 QA 계정의 0점·빈 기여 상태다. 샘플 banner가 없고 author 버전/기여 상태는 실제 API 응답을 사용한다. 중간 데스크톱 폭에서 헤더 문구가 줄바꿈되는 시각 문제는 #169에 등록했다.

## 남은 수용 범위

SPF/DKIM/DMARC의 수신 원본 헤더 전체 검사, 실제 모바일 Safari·보조기기, 장시간 만료·다중 replica·장애/복원 행렬 및 EMAIL-PROD 전체 P0 수용은 별도다. 실제 수신 성공을 이 전체 행렬의 통과로 확대하지 않는다.

실제 model/replay worker·자동 게시와 owner progress/reflection의 운영 전체 여정 수용은 #159/#162의 후속 작업이다. #162의 후속 소스·합성 검사 완료 상태와 이 고정 배포 artifact의 실제 worker 수용은 구분한다. My Page UI 배포와 첫 로그인 통과를 기여 대기→검증→게시→다른 계정 호출/성공 보고→점수의 전체 운영 여정 통과로 기록하지 않는다.
