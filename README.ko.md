# Web Agentic Memory

[English](README.md) | 한국어

> 누군가 한 번 닦은 길은, 모든 에이전트가 따라갈 수 있어야 한다.

Web Agentic Memory(WAM)는 Codex·Claude Code 등 MCP 클라이언트에 연결하는 웹 작업용 공유 메모리다. 에이전트가 공개 URL과 작업에 맞는 짧고 검증된 힌트를 조회한 뒤, 이미 쓰던 브라우저 도구로 작업한다.

**공개 조회는 계정 없이 시작한다.** 기여·사용 영수증·공로 귀속에는 WAM 계정 연결이 필요하다. 메모리 MCP 연결만으로 백그라운드 조회나 브라우저 제어 도구 설치가 시작되지는 않는다.

## 시작하기

### 이미 운영 중인 WAM 서비스에 연결

먼저 [Codex CLI](https://developers.openai.com/codex/cli) 또는 [Claude Code](https://code.claude.com/docs/en/quickstart)를 설치하고 해당 에이전트에 로그인한다. 터미널에서 `codex --version` 또는 `claude --version`으로 확인한다. 원격 MCP에는 WAM 다운로드나 Rust 설치가 필요 없다.

서비스의 `/mcp` 주소를 사용한다. 아래 `https://wam.example/mcp`는 예시 주소이므로 실제 배포 주소로 바꾸고, 사용하는 호스트 하나를 선택한다.

| 호스트 | 원격 HTTP MCP 등록 | 등록 확인 |
|---|---|---|
| Codex | `codex mcp add web-memory --url https://wam.example/mcp` | `codex mcp get web-memory --json` |
| Claude Code | `claude mcp add --transport http --scope user web-memory https://wam.example/mcp` | `claude mcp get web-memory` |

에이전트에게 `web_memory_status` 호출을 요청한 뒤 현재 공개 사이트와 작업에 맞는 힌트를 조회한다. 등록 성공, 실제 도구 호출, 계정 인증은 각각 확인해야 한다. 웹의 `/ko/guide`에서 호스트 선택부터 첫 조회, 선택적 계정 연결과 해제까지 안내한다.

원격 계정 기능은 서비스가 OAuth를 설정한 경우 호스트의 인증 흐름을 사용한다. 로컬 stdio는 `wam link`의 기기 승인과 OS 보안 저장소를 사용한다. 브라우저·사이트 로그인은 WAM 인증과 별개다. [계정·호스트 검증](docs/test/cli-link.md), [원격 OAuth 설정](docs/test/remote-oauth.md)을 참고한다.

### 이 저장소에서 직접 실행

운영자가 제공한 소스 체크아웃을 사용한다. 체크아웃이 없다면 위 원격 MCP로 기존 서비스에 연결한다. 원격 연결에는 소스 다운로드가 필요 없다. Rust와 Cargo를 설치한 뒤 체크아웃의 루트에서 두 로컬 도구를 설치한다.

```sh
# 소스 체크아웃의 루트 디렉터리에서:
cargo install --path crates/wam-cli --locked
cargo install --path crates/wam-mcp --locked
wam --version
```

Cargo의 bin 디렉터리를 PATH에 추가한다. Linux 빌드는 D-Bus 개발 패키지(Debian/Ubuntu의 `libdbus-1-dev`, `pkg-config`)를 사용한다. 연결된 자격증명은 macOS Keychain, Windows Credential Manager, Linux Secret Service에 저장하며 평문 파일 fallback은 없다. 보안 저장소가 없는 headless Linux에서도 공개 익명 조회는 가능하다.

별도 터미널에서 개발 API를 계속 실행한다.

```sh
cargo run -p wam-server --locked
```

기본 주소는 `http://127.0.0.1:3000`, 로컬 데이터는 `.wam-data/`다. 공개 조회에는 개발용 토큰이 필요 없다. 다음 명령으로 서비스 상태와 기본 예시를 확인한다.

```sh
wam --endpoint http://127.0.0.1:3000 --anonymous status --json
wam --endpoint http://127.0.0.1:3000 --anonymous recall https://docs.google.com/document/ --task compose-draft --keyword document --json
```

설치한 `wam-mcp`를 에이전트에서 쓰려면 로컬 등록 명령 하나를 선택한다.

| 전송 | Codex | Claude Code |
|---|---|---|
| Local stdio | `wam --endpoint http://127.0.0.1:3000 install codex --local` | `wam --endpoint http://127.0.0.1:3000 install claude-code --local --scope user` |

`wam --endpoint`에는 API 기본 주소를, 호스트의 원격 등록에는 `/mcp` 주소를 사용한다. `wam install`의 기본값은 원격 HTTP이며 **`--local`을 넣어야 stdio가 선택된다.** `--dry-run --json`을 추가하면 호스트 설정을 바꾸지 않고 등록 계획을 볼 수 있다. 도우미가 `wam` 옆의 `wam-mcp`를 찾을 수 있도록 두 바이너리를 함께 설치한다.

MCP만 연결했다면 웹의 `/ko/guide`에서 프로젝트 지시문을 복사해 `AGENTS.md` 또는 `CLAUDE.md`에 추가한다. 이 지시문은 연결한 MCP 도구를 호출하므로 로컬 WAM CLI가 필요 없다.

`wam guide print --host codex` 또는 `wam guide print --host claude-code`로 조회 시점에 관한 지시문을 출력한다. `--write`를 추가할 때만 현재 프로젝트의 `AGENTS.md` 또는 `CLAUDE.md`를 수정한다. 사용자가 메모리를 쓰지 말라고 하면 그 지시를 따른다. 상세 정보가 필요할 때는 recall에서 받은 버전 ID로 `wam expand <id> --version-id <version-id> --level 2 --json`을 호출한다.

웹 앱은 CI에 지정한 Node 22와 pnpm 10을 기준으로 실행한다.

```sh
pnpm --dir web install --frozen-lockfile
pnpm --dir web dev
```

`http://localhost:5173`에서 랜딩·랭킹·설치 가이드를 확인한다. Vite가 API/MCP 요청을 로컬 서버로 전달한다. API origin, GitHub OAuth, 운영 저장소와 신뢰할 재현 실행기는 [웹 실행법](web/README.md), [서버 설정](crates/wam-server/README.md)을 참고한다. 계정 연결에는 인증 설정이 필요하며 공개 조회에는 필수가 아니다.

### 연결 제거

사용한 호스트에 맞춰 `wam uninstall codex` 또는 `wam uninstall claude-code --scope user`로 `web-memory` 등록을 지운다. 다른 호스트와 공유 자격증명·캐시는 유지된다. 로컬 기기 해지는 `wam unlink`, 파생 힌트 삭제는 `wam cache clear`를 사용하며 연결할 때와 같은 endpoint/config namespace를 사용한다. 원격 OAuth는 웹의 연결된 기기 화면에서 해지한다. 소스 설치 바이너리는 `cargo uninstall wam-cli`, `cargo uninstall wam-mcp`로 제거한다.

## 구현 상태

| 영역 | 현재 동작 |
|---|---|
| 공개 메모리 조회 | URL·작업·키워드 recall, 최대 3개 힌트, 버전 지정 상세 조회, 상태와 공개 랭킹 |
| MCP | stdio와 stateless Streamable HTTP가 같은 메모리 도구 10개를 제공. GET/POST SSE와 endpoint·인증·동의·catalog 상태 조회 |
| 계정 | 기기 연결, OS 자격증명 저장, 호스트 등록, 원격 OAuth discovery/DCR/PKCE·refresh·해지 |
| 기여·공로 | 명시적 공유 동의, Gatekeeper·신뢰할 재현 검증, immutable 게시, 조건을 충족한 작성자 +20·명시적 성공 사용 +2, 철회·reversal |
| 저장소·개인정보 | 트랜잭션 PostgreSQL 또는 잠긴 로컬 snapshot, 본인 usage·영향 통계, 익명 공유 cache, rate limit, 보관기한·계정 삭제 |
| 브라우저 연동 | 기존 브라우저 MCP와 WAM을 함께 사용. 선택형 지속 Arise 연결에서 소유 탭 recall을 명시적으로 호출 |

macOS 보안 저장소, 격리된 Codex/Claude 등록, 합성 OAuth 흐름, native Arise 소유 탭 recall, PostgreSQL/Redis 통합, 웹·CLI·MCP 회귀를 로컬에서 검증했다. 실계정 GitHub 여정, Claude/Arise Hub 원격 OAuth, 다른 OS runtime, 실제 서명 artifact 설치, 운영 부하·출시 증거는 별도 게이트다. [출시 수용 기준](docs/test/release-readiness.md)을 참고한다.

### 브라우저와 데이터의 경계

WAM은 힌트를 제공하고, 페이지 이동·클릭·입력은 브라우저 MCP가 맡는다. Chrome에서는 선택한 브라우저 연결 도구를 그대로 사용한다. Arise는 MCP Hub 설정이나 선택형 `wam browser-mcp` 연결을 [호스트 문서](docs/test/hosts/arise.md)에서 안내한다. 이 연결은 browser MCP 프로세스 하나를 유지하며 선택된 소유 lease를 확인하고 URL을 로컬에서 정규화한 뒤, 에이전트가 도구를 호출할 때만 조회한다. WAM이 Arise 내부 제어 socket에 직접 연결하지는 않는다.

WAM 요청에는 정규화된 공개 URL 패턴, 작업 코드, 허용된 키워드·동작을 사용한다. WAM의 조회·기여 요청에는 페이지 본문과 자유 입력 프롬프트를 포함하지 않는다. 브라우저·모델 서비스가 처리하는 정보는 각 서비스 설정을 따른다. 원격 호스트는 WAM에 요청하기 전에 도구 인자를 모델에 보여줄 수 있으므로 서버의 정규화를 기기 전체의 개인정보 보장으로 해석하면 안 된다.

조회당 **최대 3개, 힌트당 최대 400 추정 토큰**이다. 응답 메타데이터와 상세 조회는 별도다. 힌트는 참고자료이며 현재 화면과 사용자 지시를 확인하고 적용한다. 기여는 동의가 있을 때만 제출한다. 성공 사용 점수에는 본인 인증 영수증과 실제 사용한 버전 ID가 필요하며 익명·cache 힌트만으로는 점수를 만들지 않는다.

## Meta 모델 (Muse Spark)

WAM의 핵심 신뢰 판정, 곧 기여된 작업 방법 중 무엇을 공유 메모리로 올릴지는 **Meta Model API**로 호출하는 **Muse Spark 1.3**이 맡는다. WAM은 Meta Global AI Developer Hackathon(Cerebral Valley)을 위해 만들었다.

| | |
|---|---|
| 모델 | `muse-spark-1.3`, `POST https://api.meta.ai/v1/responses`, `reasoning.effort=low`, `store: false` |
| 역할 | Gatekeeper curator. 제한된 도구 10개로 기여마다 중복·개선·신규·상충·프롬프트 주입을 판정한다. 범위, 중복, 버전, 증거는 모델이 무엇을 말하든 서버 코드가 강제하고, 기여된 텍스트는 데이터로만 다룬다 |
| 실제 결과, 2026-11-01 | 시드 시나리오 5개가 모두 통과한다(여러 번 반복). 60건 starter 코퍼스는 58/60이고 상충·프롬프트 주입은 각각 12/12이며, 놓친 2건은 게시하지 않고 사람에게 넘겼다 |
| 비용 | 검토한 기여 1건당 약 **$0.02**(콘솔 정가 기준 추정: 100만 토큰당 입력 $1.25, 캐시 입력 $0.15, 출력 $4.25). 정확한 중복은 규칙으로 처리하므로 비용이 없다 |
| 크레딧이 떨어지면 | 기여는 큐에 남고 유실이 없다. 크레딧이 돌아올 때까지 어떤 worker도 모델을 부르지 않으며, 콘솔에 없는 차단형 일일 상한 `WAM_LLM_DAILY_BUDGET_USD`를 둘 수 있다 |
| 데이터 사용 | Meta 콘솔은 무료 크레딧을 쓰는 동안 콘텐츠가 제품 개선에 쓰일 수 있다고 표시하므로 실제 기여는 유료 키로 처리해야 한다. contributor 등급 모델은 쓰지 않는다 |

Gatekeeper의 git 이력은 빌드 기간 첫날인 2026-10-27에 시작한다. 아래 2026-10-16 에디터 파일럿과 [`reference/`](reference/README.md)의 스냅샷은 그 이전의 것이다. 자세한 내용, 평가 표, 한계는 [WAM의 Meta 모델 사용](docs/design/meta-model-usage.ko.md)에 있다.

## 데모

[웹 앱](web/README.md)을 실행해 `/`의 영문 랜딩이나 `/ko`의 한국어 랜딩에서 연결 방법과 구체적인 힌트 예시를 확인한다. 아래 영상은 **로컬 데모 에디터의 스크립트 리플레이**이며 현재 WAM MCP를 통해 모델이 실제 작업을 완료한 기록은 아니다.

| 영상 | English | 한국어 |
|---|---|---|
| 같은 블로그 작업 비교 | [Watch](web/static/media/demo-compare.mp4) | [보기](web/static/media/demo-compare-ko.mp4) |
| 블로그 초안 | [Watch](web/static/media/demo-blog.mp4) | [보기](web/static/media/demo-blog-ko.mp4) |
| 슬라이드 에디터 | [Watch](web/static/media/demo-slides.mp4) | [보기](web/static/media/demo-slides-ko.mp4) |

## 현재 증거

### 수정 메모리 — R8 개발 비교, 2026-10-31

[Google Calendar](docs/test/benchmarks/google-calendar.md): L1 세 개를 짧게 줄이고 불필요한 경로를 제거한 뒤, 작업만 준 조건은 **431.108초 / 외부 도구 호출 114회**, 수정 힌트를 준 조건은 **237.169초 / 68회**였다. 개별 브라우저 호출도 176회에서 98회로 줄었고, 두 조건의 전체 작업을 독립 확인했다.

[Notion](docs/test/benchmarks/notion.md): 짧은 후보 두 개는 **375.414초**, **406.530초**로 작업만 준 조건의 **301.862초**보다 느렸다. 두 후보는 반영하지 않고 원래 Notion seed로 복원했으며, 본문은 요청한 경우에만 작성하도록 하는 수정만 남겼다. 세 조건의 전체 작업은 독립 확인했으나 Notion 속도 개선은 입증하지 못했다.

조건당 한 번, 공유 로그인과 고정 순서(Calendar M→T, Notion T→M1→M2)의 개발 관찰이다. 실제 조회한 L1만 명시적으로 제공했고 L2는 대조용으로 조회했다. 일반적인 속도·비용 절감, 인과 효과나 자동 연동을 입증한 결과는 아니다. QA 자료를 정리하고 원래 메모리 설정을 복원했다. 수정 seed는 격리 로컬 catalog에서 시험했으며 운영 서비스와 기존 저장 메모리는 갱신하지 않았다. 같은 example ID의 별도 갱신 경로는 workspace 이슈 #127로 추적한다.

### 이전 실제 생산성 서비스 작업 — R7, 2026-10-31

**Google Calendar, Notion, Canva:** macOS에 설치한 Arise 0.1.60에서 여섯 실행이 전체 작업과 독립 확인을 모두 통과했다. 서비스마다 작업만 제공한 T, 실제 WAM에서 조회한 힌트 세 개와 버전이 고정된 L2 상세를 함께 제공한 M을 한 번씩 실행했다. 모델은 `gpt-6-luna`, effort `max`, 실행당 제한은 900초다.

| 서비스·전체 보고서 | 검증한 작업 | T 시간 / 도구 호출 | M 시간 / 도구 호출 | 결과 |
|---|---|---:|---:|---|
| [Google Calendar](docs/test/benchmarks/google-calendar.md) | 시간대·매주 반복 세 회차·한 회차만 이동·세 일정 재열기 | 367.677초 / 108 | 380.690초 / 137 | 두 조건 모두 통과 |
| [Notion](docs/test/benchmarks/notion.md) | 비공개 작업 DB·날짜와 상태·표 정렬·보드 필터 | 293.419초 / 107 | 327.862초 / 117 | 두 조건 모두 통과 |
| [Canva](docs/test/benchmarks/canva.md) | 무료 템플릿·글과 이미지 교체·2페이지만 PNG 및 한 페이지 PDF로 내보내기·실제 파일 검사 | 473.976초 / 123 | 423.193초 / 106 | 두 조건 모두 통과 |

이번 실행에서 Canva는 힌트 조건의 시간과 도구 호출이 줄었고, Google Calendar와 Notion은 둘 다 늘었다. 조건당 한 번, T→M 고정 순서, 공유 로그인 상태이므로 **일반적인 속도·비용 절감 효과를 입증한 결과는 아니다.** 힌트를 명시적으로 제공했으며 Arise–WAM 자동 연동 검증도 아니다. 준비·독립 확인·정리는 수치에서 제외했다. 조건별 방법 차이와 제약은 각 보고서에 기록했다.

영문·한국어 랜딩의 **실제 서비스 벤치마크**에서도 같은 비교와 원본 Markdown 보고서를 확인할 수 있다.

### 이전 에디터 파일럿 — 2026-10-16

증거 등급은 **C**(조건당 단일 관찰, 참고용)다. 아래 수치는 이전 메모리 힌트 파일럿이며 현재 WAM MCP 전체나 모든 호스트의 end-to-end 벤치마크는 아니다. 슬라이드 작업은 아직 벤치마크하지 않았다. 로컬 테스트는 특정 계약과 fixture를 확인하며 일반적인 속도·토큰 절감 보장을 뜻하지 않는다.

2026-10-16 파일럿 측정. 같은 태스크를 메모리 없이(tools only) 하는 경우와 메모리를 주입한 경우로 비교했다.
모델: Codex `gpt-6-luna`, effort `max`. 조건마다 새 격리 환경에서 실행했다.
상세 방법과 원자료: [평가 원자료](reference/agent-browser/docs/specs/browser-agent-memory-evaluation.md).

### 실제 사이트 (로그인된 실사용 에디터, 발행 없이 초안만 저장)

| 사이트 | 메모리 | 시간 | 툴 호출 | 시간 변화 | 호출 변화 |
|---|---|---:|---:|---:|---:|
| Medium | 없음 | 85.5초 | 10 | | |
| Medium | 주입 | **44.3초** | **7** | **−48%** | **−30%** |
| Naver Blog | 없음 | 84.1초 | 16 | | |
| Naver Blog | 주입 | **58.8초** | **12** | **−30%** | **−25%** |

품질 변화도 있었다. Medium은 메모리 없이 했을 때 불필요한 빈 문단이 하나 생겼지만, 주입 후에는 제목과 정확히 두 문단만 저장됐다. (새 브라우저 세션에서 편집 URL을 다시 열어 확인)

### 통제된 에디터 fixture (독립 저장 oracle로 결과 검증)

| 에디터 | 메모리 | 시간 | 툴 호출 | 오류 | 시간 변화 |
|---|---|---:|---:|---:|---:|
| 리치 에디터 | 없음 | 130.3초 | 16 | 0 | |
| 리치 에디터 | 수동 recall | 66.7초 | 12 | 0 | −49% |
| 리치 에디터 | 자동 주입 | **55.2초** | **11** | 0 | **−58%** |
| iframe 에디터 | 없음 | 195.4초 | 28 | 3 | |
| iframe 에디터 | 수동 recall | **91.7초** | 18 | 0 | **−53%** |
| iframe 에디터 | 자동 주입 | 130.9초 | 17 | 0 | −33% |

### 요약

| 지표 | 개선 범위 | 4개 조건 단순 평균 |
|---|---|---|
| 소요 시간 | −30% ~ −58% | 약 −42% |
| 툴 호출 수 | −25% ~ −39% | 약 −31% |
| 오류 | iframe에서 3 → 0 | |

### 정직하게 밝혀둘 한계

- 조건당 **1회 시행**이고 실행 순서도 무작위가 아니다. 일반적인 지연 보장이 아니라 **파일럿 관찰**이다.
- Naver 실사이트는 두 번째 실행에서 이어쓰기 팝업과 임시저장 개수가 달라져, 통제된 A/B가 아니다.
- iframe fixture에서는 자동 주입이 수동 recall보다 느렸다. 자동 주입이 항상 가장 빠른 것은 아니다.
- 서버가 503을 반환하는 등 사이트 자체 문제는 메모리로 해결되지 않는다. 이 경우 에이전트가 실패를 정확히 보고하는 것까지 확인했다.
- **TODO:** 조건별 3~5회 반복 측정으로 평균과 편차를 보고한다. 호스트(Arise 자체 메모리 자동 주입 vs Chrome MCP + WAM 수동 recall)별로 분리해서 측정한다.

## 구조와 개발

![Web Agentic Memory 아키텍처 개요](docs/assets/architecture-overview.ko.png)

```text
에이전트와 기존 브라우저 도구
   | MCP 또는 CLI: 공개 URL 패턴 + 작업 + 허용 키워드
   v
Web Agentic Memory 서버 (Rust)
   ├─ 사이트별 승인된 워크플로 카탈로그 ──▶ recall: 최대 3개, 각 ≤ 400 추정 토큰
   └─ 동의한 기여 ──▶ Gatekeeper + 신뢰할 재현 검증 ──▶ immutable 게시
```

Rust workspace는 `wam-core`, `wam-contracts`, `wam-client`, `wam-cli`, `wam-mcp`, `wam-server`, `wam-gatekeeper`로 나뉜다. 웹은 SvelteKit/TypeScript다. 공유 application store는 PostgreSQL, 공유 quota는 Redis를 사용하고 로컬 모드는 잠긴 snapshot과 in-memory limiter를 쓴다. 모델 provider와 신뢰할 브라우저 재현 실행기는 명시적인 인터페이스로 분리한다.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
pnpm --dir web test
pnpm --dir web check
pnpm --dir web build
```

검사는 로컬에서 실행하며 GitHub Actions 워크플로우는 포함하지 않는다.

DB·Redis·실제 provider·OS 보안 저장소 테스트에는 별도 환경이 필요하다. [테스트 인덱스](docs/test/README.md), [소유한 로컬 데이터 서비스 실행기](tests/env/README.md), [출시 검사](docs/test/release-readiness.md)에서 조건을 확인한다. Fixture 성공과 실제 호스트·계정 검증을 구분해 기록한다.

## 왜 길을 공유하는가

에이전트는 사이트마다 같은 사용법을 다시 찾고, 세션이 끝나면 그 비용을 다시 낸다. WAM은 한 번 검증한 발견이 다음 에이전트에게 도움이 되도록 하면서, 공유는 선택으로 남기고 기여자의 공로를 기록하려는 프로젝트다. 배경과 원문 자료는 [사상의 계보와 설계 원칙](docs/history.md)에 정리했다.

## 문서

| 문서 | 내용 |
|---|---|
| [CLI 안내](crates/wam-cli/README.md) · [MCP 안내](crates/wam-mcp/README.md) | 명령, 자격증명, 호스트 등록과 도구 계약 |
| [웹 실행법](web/README.md) · [서버 설정](crates/wam-server/README.md) | 로컬 개발과 서비스 구성 |
| [시스템 개요](docs/design/system-overview.ko.md) · [모듈 경계](docs/design/module-boundaries.md) | 구성요소, 데이터 흐름, 테스트 경계 |
| [브라우저·호스트 연동](docs/design/browser-host-integration.md) | 브라우저 소유권과 WAM 계정의 경계 |
| [WAM의 Meta 모델 사용](docs/design/meta-model-usage.ko.md) | Gatekeeper 모델 Muse Spark, 실제 호출 평가, 기여당 비용, 크레딧 소진 처리, 데이터 사용, 해커톤 메모 |
| [요구사항](docs/spec/README.md) · [테스트 인덱스](docs/test/README.md) | 계약과 검증 절차 |
| [Google Calendar](docs/test/benchmarks/google-calendar.md) · [Canva](docs/test/benchmarks/canva.md) · [Notion](docs/test/benchmarks/notion.md) | 서비스별 실제 작업 벤치마크·독립 확인·T/M 결과와 해석 한계 |
| [출시 수용 기준](docs/test/release-readiness.md) · [서명 archive 검증](docs/test/releases.md) | 증거 게이트와 다운로드 artifact의 안전한 설치 |
| [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md) | 저장소 규칙과 데이터 취급 |
| [공개 준비 점검](docs/open-source-readiness.md) · [reference 안내](reference/README.md) | 라이선스·reference 권리·공개 범위 |

## 다음 단계

- [x] 공개 recall, MCP/CLI와 웹 설치 안내
- [x] 동의 기반 기여, 결정적 검사와 중복 처리
- [x] 트랜잭션 게시, 버전 귀속과 점수 원장
- [x] 로컬 통합·브라우저 fixture·회귀 검사
- [ ] 실제 모델 + WAM 작업 영상과 호스트별 반복 측정
- [ ] 실계정·다른 OS·실제 서명 artifact 수용 검증
- [ ] 운영 서비스·부하 증거와 공개 출시 결정

## 라이선스

아직 선택하지 않았다. LICENSE 파일이 추가되기 전까지 이 저장소의 재사용 권한은 부여되지 않는다. `reference/` 안의 파일은 별도 권리 검토가 필요하다. 소스 반영은 바이너리 릴리스의 수용 게이트 통과를 뜻하지 않는다.
