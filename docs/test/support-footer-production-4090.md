# 문의 메일 푸터 4090 배포

2026-11-07 15:43 KST(06:43Z), 랜딩 푸터에 `support@webagenticmemory.com` 문의 링크를 운영 반영했다.

| 항목 | 결과 |
| --- | --- |
| 소스 커밋 | `4f098deba1a7f3cfef5a42e539652821391419ea` |
| 릴리스 | `20261107T063922Z-main-4f098de` |
| 이전 릴리스 | `20261107T062322Z-main-9334e66` |
| 활성화 | `2026-11-07T06:43:50Z` |
| API SHA-256 | `4df8489a154f9c329c84f904951c3293f40499416a70e97089c77adc4f7135d6` — 이전 운영과 동일 |
| DB 백업 | `/var/backups/web-agentic-memory/20261107T063922Z-main-4f098de/production-before.dump` |
| 데이터·설정 보존 | 기존 메모리/버전 38개, 공식 점수 880점, 운영 설정·systemd·Caddy 파일 11개 해시 동일 |

## 변경과 검증 기준

`web/src/routes/+layout.svelte`의 푸터 안내문 아래에 `mailto:support@webagenticmemory.com`을 추가했다. 영문은 `Support`, 한국어는 `문의`로 표시하고, 밑줄과 키보드 포커스로 링크를 구별한다. 영문 `/`과 한국어 `/ko`에서 링크가 하나씩 있고, 주소가 정확하며 좁은 화면에서도 가로 넘침 없이 읽혀야 한다는 기준을 편집 전에 정했다.

800px 화면에서 기존 가로 푸터가 문의 주소를 세 줄로 나누는 것을 이미지로 확인하고, 주소의 한 줄 높이 검사로 실패를 재현했다. 761~1100px 구간에서 안내·문의 정보를 다음 행에 배치해 해결했다.

| 검사 | 결과 |
| --- | --- |
| 기존 웹 테스트 | 81 passed, 0 failed |
| Svelte 검사·프로덕션 빌드 | 0 errors, 기존 SetupGuide 경고 2개, build 통과 |
| 실제 렌더링 | 1440/1024/800/768/390/320px × 영문/한국어 12건 통과: 주소 한 줄, 정확한 mailto, 키보드 포커스, 가로 넘침 없음 |
| 시각 검토 | desktop/tablet/mobile 푸터 PNG를 부모가 직접 열어 확인 |
| Arise | 로컬 한국어와 운영 영문 랜딩에서 실제 DOM의 문의 문구·mailto 확인 |
| 운영 소스·무결성 | 운영 전용 미커밋 소스 0개. manifest 638개·source archive 471개 파일을 pinned commit과 대조하고 업로드 SHA-256 검사 통과 |
| 필수 Linux release 검사 | 테스트 요약 59개, 444 passed/0 failed. 기본 ignored 31개 중 PostgreSQL application 18개와 email/admin 2개를 별도 실행한 결과 포함 |
| 활성화 | 공유 잠금·이전 릴리스 확인·DB 백업·API/admin 경계·공식 메모리/점수·정적 페이지 검사 통과, 자동 rollback 없음 |
| 공개 HTTPS | `/`·`/ko` 모두 200, 현지화된 문의 링크 정확히 한 개씩 확인. 제공 CSS 2개 SHA-256이 패키지와 일치. apex/API health 200 |
| 작업 보존 | 공유 원본의 README·데모 등 별도 작업 20개 파일 해시 동일 |

## 증거와 복구

서버 `builds/20261107T063922Z-main-4f098de/`에 소스·웹 archive, manifest, SHA256SUMS, Linux `build.log`와 배포 전 기준선을 보존한다. 로컬 Git 제외 경로 `build/deploy-4090/support-footer-20261107/`에 검사 로그, `footer-qa.json`, 렌더 PNG, 배포 전후 기준선과 `post-deploy-check.json`이 있다.

로컬 미리보기는 `http://127.0.0.1:5193/`, listener PID `19311`로 detached 실행 중이다. 로그는 같은 로컬 경로의 `preview.log`다. 이전 미리보기는 rebuild 후 새 CSS를 찾지 못해 소유 작업 경로를 확인한 뒤 재시작했다.

복구는 [운영 릴리스 절차](../../deploy/4090/release/README.md)를 따른다. 이전 release와 DB 백업을 보존한다.
