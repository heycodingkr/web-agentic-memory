# 3분 데모 영상 구성안

*Meta Global AI Developer Hackathon(Cerebral Valley) 제출 영상. 상태: 2026-11-01. **v1 영상을 [`demo/video`](../../demo/video/README.md)에서 렌더했다(무음, 영어 자막).** 이 문서는 구성과 근거를 정한다. 영상에 나오는 수치는 모두 이 저장소의 기록에서 가져왔고, 출처와 허용 표현은 [클레임 원장](#클레임-원장)에 적었다. ⏳는 아직 검증하지 않은 항목이다. 리허설에서 시간과 문구를 조정한다. Muse 호스트 검증 결과(2026-11-01)는 [`docs/test/hosts/meta-muse.md`](../test/hosts/meta-muse.md)에 있다.*

## 한눈에

| 항목 | 내용 |
|---|---|
| 길이 | **목표 2:50, 상한 2:55.** 규정이 "three minute"이고 업로드 재인코딩 여유가 필요하다. 3:00을 넘기지 않는다 |
| 마감 | 2026-11-04 21:00 ET, 곧 **2026-11-05 11:00 KST**. 11-04 KST 저녁까지 제출을 끝내는 것을 목표로 한다 |
| 언어 | 영어 자막을 화면에 굽는다. **v1은 음성 없이 무음**이라 심사자가 소리 없이 봐도 이야기가 이어진다. 직접 녹음하려면 [부록 A](#부록-a-영어-내레이션-최종본)를 읽고 SRT 시간에 맞춘다. 한국어 자막판은 선택 |
| 한 줄 메시지 | *Any agent reads. Meta's Muse Spark decides what's worth remembering.* |
| 링크 노출 | 0:10과 2:40, 두 번. **webagenticmemory.com**, MCP 주소 `https://api.webagenticmemory.com/mcp`, GitHub 저장소 |
| Meta가 보이는 곳 | 쓰는 쪽은 Gatekeeper curator(Muse Spark 1.3, Meta Model API). 읽는 쪽은 Muse를 모델로 쓰는 호스트(Muse Code, Meta 모델을 붙인 Claude Code. 2026-11-01 검증) |
| 증거 | 실사이트 개선 관찰(Calendar·Canva), 이전 파일럿(Medium·Naver), Meta 평가(58/60), 기여 1건 비용(약 $0.022) |

## 제작된 영상 (v1)

`node demo/video/build.mjs`로 렌더한다(약 4분). 결과는 git에서 제외되는 `demo/out/video/`에 생긴다.

| 파일 | 내용 |
|---|---|
| `wam-demo-day.mp4` | 1920×1080, 30fps, H.264(BT.709), **2:50, 무음**, 약 13MB |
| `wam-demo-day.en.srt` | 화면에 굽힌 자막과 같은 배열에서 만든 영어 자막(음성을 넣을 때의 시간 기준) |
| `wam-demo-day-poster.jpg` | 썸네일(씬 5의 Muse 활용 범위 화면) |

위 구성안과 달라진 점:

- **씬 5는 실제 호출이다.** `wam-curate --scenario all -v`를 2026-11-01에 Meta Model API(`muse:muse-spark-1.3`)로 돌린 출력 그대로다(`demo/video/captures/curate-all.txt`). 5개 시나리오 모두 PASS, 모델 턴 20번, 제공자가 보고한 비용 **$0.0492**. 화면에는 "Gatekeeper의 LLM이 Muse Spark 1.3"이라는 점, 도구 10개, 판정 5종, 모델 3종 비교(1.1은 3/5), 추론 강도 조정(−40% 비용), 58/60과 주입 12/12를 한 번에 보여 주는 **Muse 활용 범위 타일 6개**를 넣었다.
- **씬 6은 실제 Muse Code 세션 로그다.** 로컬 개발 서비스에서 `web_memory_status`와 `web_memory_recall`을 호출한 이벤트를 줄여서 보여 준다. 힌트는 내장 `Example:` 항목이라 화면에 "검증된 카탈로그 항목이 아님"이라고 적었다. 씬 5에서 게시한 버전을 씬 6에서 조회하는 연속 흐름은 **만들지 않았다**(운영 카탈로그가 비어 있고 replay worker가 없다).
- **씬 4는 그림(다이어그램)이다.** 로그인 기여 화면과 My Page 녹화는 로컬 리그가 필요해서 넣지 않았다. 녹화로 보이는 목업으로 대체하지 않는다.
- 씬 3에는 Calendar·Canva의 최신 실사이트 비교와 Medium 초기 파일럿(BEFORE)을 쓴다. Notion은 교정 메모리 재측정 전이라 수치를 보여 주지 않는다.
- 자막은 한 줄(약 70자)로 줄였고, 벤치마크 자막은 "faster"를 인과로 읽히지 않게 "single runs"를 붙였다.

## 규칙과 심사 기준에서 나온 설계 원칙

출처는 [참가 가이드](../../Details_–_Meta_Global_AI_Developer_Hackathon_–_Cerebral_Valley.pdf)다.

| 규칙 또는 기준 | 영상에서 지키는 법 |
|---|---|
| 데모는 **해커톤 중(10-27~11-04)에 만든 기능만** 보여야 한다. 구분이 안 되면 실격이다 | 새 기능이 나오는 씬마다 모서리에 **NEW · built Oct 27–Nov 4** 배지, 이전 자료(10-16 파일럿, `reference/`)에는 **BEFORE** 배지. 마지막 씬에 새것·기존 구분표 |
| 1단계 합격·불합격: Meta Model API 또는 오픈 웨이트를 실제로 썼는가 | 가짜 모델 없이 **실제 호출** 트레이스(`provider muse:muse-spark-1.3`)를 화면에 둔다. 씬 5 |
| Impact 30% | 실사이트와 이전 파일럿의 관찰(Calendar·Canva·Medium) 및 한계를 같이 보여 준다. 씬 3, 7 |
| Use of Meta AI Tooling 25% | 기본 연동 이상을 보인다. 도구 10개로 제한한 에이전트, 프롬프트 주입 12/12 에스컬레이션, 모델 3종 비교, 추론 강도 조정(비용 약 40% 감소·2배 빠름). 씬 5 |
| Technical Depth 20% | 서버가 범위·중복·버전·증거를 모델 출력과 무관하게 강제, 불변 버전, 크레딧 소진·장애 때 큐 대기. 씬 4, 5, 7 |
| Demo 25% | 한 흐름으로 이어지는 실제 동작: 기여 → Muse 판정 → 게시 → 다른 사용자 사용 → 점수. 같은 `version_id`가 씬 5와 6에 모두 보이게 한다 |
| 공개 저장소, 영상 링크 접근 가능, 참여자 전원 기재 | [제출 체크리스트](#제출-체크리스트) |
| 보너스: Best Agentic Use-Case, Most Viral(X 게시), Community Vote | 씬 5·6이 자율 판단 근거. X용 컷은 [부록 B](#부록-b-x-게시용-짧은-컷) |

## 이야기 구조

1. **문제 (0:00~0:22).** 브라우저 에이전트는 사이트마다 같은 시행착오를 반복한다. 숫자 하나로 시작한다.
2. **해법 (0:22~2:10).** 한 번 닦은 길을 공유 메모리로 만든다. 무엇을 기억할지는 Meta의 Muse Spark가 판정하고, 쓰는 쪽은 어느 에이전트든 MCP로 읽는다.
3. **증거 (2:10~2:50).** 비용이 얼마이고 한 번 승인한 힌트가 매 실행을 얼마나 줄이는지, 어디서 안 통했는지까지 보여 준다.

## 타임라인

총 2:50. 씬 사이는 컷 전환이고 BGM은 작게 깐다(내레이션 아래 -24 dB 이하, 또는 없음).

| 시간 | 씬 | 화면 | 화면 텍스트 | 노리는 항목 |
|---|---|---|---|---|
| 0:00~0:10 | 1 훅 | Calendar 같은 작업의 두 타이머 막대. 힌트 없음 431초·114호출, 공유 메모리 237초·68호출 | `−45% time · −40% calls` + 작은 글씨로 조건 | Demo, Impact |
| 0:10~0:22 | 2 제품과 주소 | 랜딩 첫 화면, 터미널에서 MCP 한 줄 연결, `/guide`의 호스트 탭 | `webagenticmemory.com` | Demo |
| 0:22~0:50 | 3 업무 사례 | Canva·Google Calendar·Medium의 개선 관찰과 Docs/Slides 데모 카드를 보여 준다. Notion은 재측정 대기로 수치가 없다. | 카드별 수치와 라벨 | Impact |
| 0:50~1:05 | 4 기여 | 로그인, 동의 체크, 닫힌 스키마(URL 패턴·작업·정규화 단계), 결정적 검사 3단 | `NEW` 배지 | Technical Depth |
| 1:05~1:50 | 5 **Muse가 판정** | 터미널 `wam-curate -v` 실제 트레이스, 판정 5종 표, 주입 사례, 58/60 카드 | `muse-spark-1.3 · Meta Model API` | **Meta Tooling**, Technical Depth |
| 1:50~2:10 | 6 재사용과 점수 | Muse 호스트에서 `web_memory_recall`, My Page 원장, 익명 호출은 점수 없음 | `+20` `+2` `anonymous: 0` | Demo |
| 2:10~2:35 | 7 경제성 | 3칸 카드: 비용 / 효과 / 되돌려 받는 구조 | `$0.022 / review` `−194 s · −46 calls per run` | Impact |
| 2:35~2:50 | 8 마무리 | 새것 vs 기존 구분표, 한계 한 줄, 주소와 저장소 | `Built this week` | 규칙 준수 |

## 씬별 상세

각 씬의 상태 표시: ✅ 지금 촬영 가능, 🟡 준비 작업 필요, 🔴 해결 전에는 촬영하면 안 됨.

### 씬 1. 훅 (10초) 🟡

```
Same Google Calendar task, same agent. 431 seconds, 114 tool calls.
With shared memory: 237 seconds, 68 calls.
```

- 화면: 두 막대가 동시에 달리다가 한쪽이 먼저 멈춘다. 같은 화면 하단에 조건을 작게 둔다: `R8 development comparison · one run per condition · shared login · fixed order (task-only first)`. 작은 글씨라도 최소 2초 이상 정지한다.
- 자료: [Google Calendar 보고서](../test/benchmarks/google-calendar.md). 제작은 `demo/record/stage/stage.html` 같은 시간 함수형 무대로 만들면 렌더가 기계 속도에 영향을 받지 않는다.

### 씬 2. 제품과 주소 (12초) 🟡

```
Browser agents relearn every website from zero. Web Agentic Memory is a shared memory
they read over MCP, live now at webagenticmemory dot com.
```

- 화면: 배포된 랜딩 → 터미널에서 `claude mcp add --transport http --scope user web-memory https://api.webagenticmemory.com/mcp`(또는 Codex의 `codex mcp add web-memory --url …`) → `/guide`의 호스트 선택 탭.
- `/guide`에 **Muse 탭을 맨 앞에** 둔다(이번 작업, [Muse 호스트](#muse를-호스트로-쓰기) 참고). 탭이 생기면 여기서 한 컷 보여 준다.
- 주소는 촬영 직전에 헬스체크로 다시 확인한다. 배포 경로가 바뀔 수 있다(#159, #157).

### 씬 3. 업무 사례 (28초) 🟡

```
Think of the work teams already hand to agents. Exporting one Canva page as PNG and PDF.
Scheduling recurring Google Calendar sessions across time zones. Saving a Medium draft.
Single-run observations showed improvements for Calendar, Canva and an earlier Medium pilot.
Notion is awaiting a corrected remeasurement, so no timing is shown.
```

- 화면: 카드 4장이 차례로 올라온다. 카드마다 직무 한 줄, 작업 한 줄, 시간 막대, 라벨. 구성은 [실사용 사례](#실사용-사례)를 따른다.
- Medium에는 `BEFORE · early pilot` 표시를 둔다. Notion은 비교 막대 없이 `Remeasurement pending` 상태만 표시하거나, 별도 카드에서 제외한다.
- 카드 아래 한 줄: `Single observation per condition. Not a general speed guarantee.`
- 브랜드는 **글자로만** 쓴다. 공식 로고는 권리를 확인하지 못했으므로 쓰지 않는다(랜딩과 같은 자체 픽토그램 `SiteGlyph.svelte`는 가능).

### 씬 4. 기여 (15초) 🟡

```
Anyone can contribute a workflow, with consent, and only as a closed schema:
URL pattern, task, normalized steps. No page text. Server-side checks screen it and
replay it before any model sees it.
```

- 화면: 로그인한 사용자 A가 `/contribute`에서 동의를 켜고 제출 → 단계 3칸이 켜진다(텍스트 검열, 신뢰 replay, 정확 중복 규칙). 오른쪽에 `$0 · no model call` 라벨.
- ⚠ 운영 서버에는 replay worker가 아직 없다(#159). 로컬 리그에서 촬영하고 replay 증거가 fixture이면 화면에 **`replay evidence: fixture`** 라벨을 둔다. 가짜 replay를 실제인 것처럼 보이게 하지 않는다.

### 씬 5. Muse가 판정한다 (45초, 핵심) ✅

```
Then Meta's Muse Spark 1.3, through the Meta Model API, decides what's worth remembering.
It has ten bounded tools, read, compare, decide, and no browser, shell, or database.
Here it replaces an older workflow with a shorter one; every call is logged with provider
and model. Duplicate? Support it. Conflict? Escalate to a human. And when a contribution
says "retire the active memory"? It escalates instead of obeying. Fifty-eight of sixty on
our labeled corpus, twelve of twelve on injections. The server enforces scope, versions,
and evidence no matter what the model says.
```

타임 배분:

| 구간 | 화면 |
|---|---|
| 1:05~1:15 | 터미널에 `wam-curate --scenario all -v` 실행. 표의 `provider` 열이 `muse:muse-spark-1.3`로 채워진다. 줌 인해서 모델 ID에 형광 표시 |
| 1:15~1:27 | `better` 시나리오의 `-> memory_compare` → `-> decide_replace` 줄을 확대. 옆에 "10 tools: 4 read · 6 decide" |
| 1:27~1:37 | 판정 5종 표가 하나씩 켜진다. duplicate → support, better → replace, new → create, conflict → escalate, injection → escalate |
| 1:37~1:45 | `injection` 줄에 멈춘다. 기여 필드의 문장 "retire the active memory"를 보여 주고 결과는 escalate |
| 1:45~1:50 | 카드: `58/60 · injection 12/12 · conflict 12/12` + 작은 글씨 `single run · synthetic labeled corpus · 2 misses went to a human` |

- 자료: [Meta 모델 사용](../design/meta-model-usage.ko.md), [테스트 기록](../test/meta-model.md).
- 촬영: 터미널 폰트 20pt 이상, 미리 `set -a; . ./.env; set +a`를 화면 밖에서 실행해 두고 **`.env`를 절대 `cat` 하지 않는다**. 실행 한 번에 약 $0.11이다. 두 번 녹화해 더 깔끔한 쪽을 쓴다.
- 해커톤 TODO에 있는 "데모 화면에서 model trace 보여 주기"는 이 터미널 트레이스로 충족한다. `/review`에 판정 패널을 따로 만드는 것은 선택이다.

### 씬 6. 재사용과 점수 (20초) 🟡

```
Next, any agent reads it. Here Muse Spark itself, in Muse Code, recalls the hint over MCP.
The author earns points only when another signed-in user succeeds. Anonymous reads get
the hint, but no receipt, and no points.
```

- 화면 ① 읽는 쪽: Muse Code(`muse`)에서 `web_memory_recall` 호출과 힌트. 씬 5에서 게시된 것과 **같은 `version_id`**가 보인다. Muse Code는 2026-11-01에 로컬과 공개 주소 모두에서 호출에 성공했다. 설정 파일(`schema_version`, `mcpServers`)은 [가이드](../test/hosts/meta-muse.md)대로 미리 만들어 둔다. Codex에 Meta 모델을 붙인 화면은 쓰지 않는다(동작하지 않음).
- 화면 ② 점수: 사용자 A의 My Page(`/dashboard`) 원장에 게시 `+20`, B의 성공 영수증 `+2`.
- 화면 ③ 비교: 같은 힌트를 익명으로 조회한 경우 영수증·점수 행이 생기지 않는 것을 좌우로 보여 준다.
- ⚠ My Page가 "기여 결과와 게시 메모리·버전 링크"를 보여 주는 기능은 아직 없다(#162). 원장 줄까지만 보여 주고 그 이상을 말하지 않는다.
- ⚠ `Example:`로 시작하는 내장 예시를 검증된 메모리처럼 보여 주지 않는다. 씬 5에서 게시한 실제 버전을 조회한다.

### 씬 7. 경제성 (25초) 🟡

```
The economics: Muse Spark reviews a contribution for about two cents, and nothing for exact
duplicates or for reads. A daily cap and a durable queue keep it safe when credit runs out.
A verified hint then saves every later run: Calendar, a hundred ninety-four seconds and
forty-six fewer calls, each time. Pay once. Reuse every run.
```

화면은 3칸 카드 한 장이다. 자세한 내용은 [경제성 카드](#경제성-카드)에 있다.

### 씬 8. 마무리 (15초) 🟡

```
Built this week, except where labeled: the Gatekeeper, the ledger, the MCP service, the web.
Earlier pilot results are marked. Try it at webagenticmemory dot com.
Muse decides. Every agent follows.
```

- 화면: [새것·기존 구분표](#새로-만든-것과-기존-것)를 3초간 정지, 이어서 큰 글씨의 주소와 저장소 주소.
- 한계 한 줄(작은 글씨): `Benchmarks: one run per condition. Model evaluation: synthetic scenarios. Not yet verified: real replay worker in production, live GitHub accounts.`

## 경제성 카드

영상에는 **측정한 수치만** 쓴다. 화면 구성은 3칸이다.

| 칸 | 화면 문구 | 근거와 단서 |
|---|---|---|
| 비용 | `$0.022 / review` · `$0 exact duplicate` · `$0 per read` · `daily cap + queue when credit ends` | 검토한 기여 1건(모델 호출 약 4번), `reasoning.effort=low`, 콘솔 정가로 환산한 **추정**. 기본 강도는 $0.036이라 강도 조정으로 약 40% 절감, 호출당 3.6초 → 1.8초. 정확 중복은 규칙이 먼저 처리해 모델을 부르지 않는다. recall은 서버 코드에서 모델을 부르지 않는다(`LlmProvider` 사용처는 큐 worker 한 곳) |
| 효과 | `−194 s · −46 calls` per run (Google Calendar) | R8, 431.108초/114호출 → 237.169초/68호출. 단일 관찰이다. Canva는 `−51 s · −17 calls`(−11%·−14%) |
| 구조 | `One review → every later run` | 비용은 기여할 때 한 번, 효과는 그 힌트를 쓰는 모든 실행에서 반복된다. 그림은 막대 하나(비용)와 같은 크기의 막대가 반복(효과)되는 형태 |

모델을 쓰는 곳이 **쓰기 쪽 판정 한 곳**이라는 점이 경제성의 핵심이다. 읽는 쪽 에이전트는 어떤 모델이든 된다.

**Q&A용 환산 (영상에는 쓰지 않는다).** Calendar 한 작업의 단일 관찰값(−193.9초, −46호출)을 같은 작업이 같은 화면에서 N번 반복된다고 가정해 곱한 예시다. 예측이 아니다.

| 재사용 횟수 N | 절감 에이전트 시간 | 절감 도구 호출 | 큐레이터 비용 | 재사용당 큐레이터 비용 |
|---:|---:|---:|---:|---:|
| 10 | 32분 | 460 | $0.022 | $0.0022 |
| 100 | 5.4시간 | 4,600 | $0.022 | $0.00022 |
| 1,000 | 53.9시간 | 46,000 | $0.022 | $0.000022 |

랜딩은 "1,000편 → 약 11시간" 추산을 과장 위험 때문에 뺀 전례가 있다(#39). 영상에 곱셈을 넣으려면 `illustrative · single observation` 라벨을 같은 화면에 두고, 아니면 위의 3칸만 쓴다. 소비 쪽 **달러 절감은 측정하지 않았다.** 호출 수는 비용의 대리 지표일 뿐이다. 반복 측정을 하게 되면 토큰 수를 같이 기록해 이 칸을 채운다([갭](#촬영-전-갭)).

## Meta 모델은 어디에 쓰였나

| 위치 | Meta 구성요소 | 하는 일 | 화면 증거 |
|---|---|---|---|
| Gatekeeper curator | Muse Spark 1.3, `POST api.meta.ai/v1/responses`, `reasoning.effort=low`, `store: false` | 기여마다 중복·개선·신규·상충·주입을 판정. 읽기 4개와 판정 6개, 도구 10개로 행동 | `wam-curate -v` 트레이스와 감사 기록의 `muse:muse-spark-1.3` |
| 읽는 쪽 호스트 | Muse Code CLI(`muse-spark-1.3`), Claude Code를 Meta Model API로 돌린 구성. Codex 구성은 동작하지 않음 | MCP로 `web_memory_status`·`web_memory_recall` 호출. 한 번에 모델 호출 3회, 정가 환산 약 $0.05 | 씬 6 |
| 모델 선택 | Spark 1.1·1.2·1.3 비교, 추론 강도 조정 | 시드 5개 통과 수 3/5·5/5·5/5, 호출 비용과 속도 | 씬 5의 카드 |
| 운영 안전장치 | 일일 지출 상한, 크레딧 소진·장애 때 큐 대기 | 모델이 막혀도 기여가 유실되지 않음 | 씬 7 한 줄 |
| **쓰지 않은 것** | Contributor 등급(입출력이 학습에 쓰임), Muse Wonder(우리 키에서 사용 불가), 오픈 웨이트(미평가) | 사용했다고 말하지 않는다 | 마지막 씬 한 줄(선택) |

## Muse를 호스트로 쓰기

WAM의 MCP는 호스트를 가리지 않으므로 **Muse 모델을 쓰는 에이전트도 같은 `web-memory` 서버를 그대로 쓴다.** 심사 항목 "Use of Meta AI Tooling"에 쓰는 쪽도 Meta로 보이게 하는 것이 목적이다. 연결 경로는 CLI 세 가지와 Arise 하나이며, 이번 작업에서 가이드와 랜딩에 올리고 실제로 검증한다.

| 경로 | 모델 연결 | WAM 연결 | 상태 |
|---|---|---|---|
| **Muse Code** (Meta의 터미널 코딩 에이전트) | `muse` 로그인 또는 `META_API_KEY` | `~/.config/muse/settings.json`의 `mcpServers`(`streamable_http`, `schema_version` 필수) | ✅ 로컬·공개 주소. OAuth 로그인은 미확인 |
| **Codex** + Meta Model API | `[model_providers.meta]`, `base_url=https://api.meta.ai/v1`, `wire_api="responses"`, `MODEL_API_KEY` | `codex mcp add web-memory --url …` | ❌ MCP 호출이 `unsupported call`(Codex의 namespace 도구와 Meta Responses API의 이름 형식 불일치) |
| **Claude Code** + Meta Model API | `ANTHROPIC_BASE_URL=https://api.meta.ai`, `ANTHROPIC_AUTH_TOKEN`, 모델 별칭 고정, `ENABLE_TOOL_SEARCH=false` | `claude mcp add --transport http …` | ✅ tool search를 끄면 3턴, 켜면 7턴 |
| Arise 브라우저 | Meta Model API 키(API 키 provider) | Arise MCP Hub로 같은 서버 | 구현·커밋됨(`feat/muse-provider`), **아직 빌드·배포 전**이라 영상에 쓰지 않는다 |

모델 설정은 [Meta Model API의 코딩 에이전트 안내](https://dev.meta.ai/docs/coding-agents)와 [Muse Code의 확장 안내](https://dev.meta.ai/docs/muse-code/extending)에서 가져왔다. 검증 결과와 날짜는 [`docs/test/hosts/meta-muse.md`](../test/hosts/meta-muse.md)에 기록했다. 영상과 랜딩은 거기에 ✅로 적힌 경로만 "동작한다"고 쓴다.

## 실사용 사례

촬영과 카드에 쓰는 직무·브랜드 매핑이다. 브랜드 이름은 글자로만 표기한다.

| 직무 | 사이트 | 작업 | 측정 | 카드에 쓰는 문구 |
|---|---|---|---|---|
| 마케팅·디자인 | **Canva** | 무료 템플릿에서 글과 이미지 교체, 2페이지만 PNG로, 한 페이지는 PDF로 내보내기, 실제 파일 검사 | R7: 473.976초/123 → 423.193초/106 (−11%/−14%) | `Export the right page, in the right format` |
| 운영·비서 | **Google Calendar** | 시간대, 매주 반복 세 회차, 한 회차만 이동, 세 일정 재열기 | R8: 431.108초/114 → 237.169초/68 (−45%/−40%) | `Recurring sessions, one moved` |
| 기획·지식 업무 | **Notion** | 비공개 작업 DB, 날짜와 상태, 표 정렬, 보드 필터 | 교정 메모리 재측정 대기 · 현재 속도 비교 없음 | `Remeasurement pending` |
| 콘텐츠·홍보 | **Medium**, **네이버 블로그** | 초안만 저장 | 2026-10-16 파일럿(**BEFORE**): −48%/−30%, −30%/−25% | `Before the hackathon · pilot` |
| 문서·발표·정산·메일 | Docs, **Google Slides**, Sheets, Gmail류 | 제목·목록·표, 3장 덱, 합계 수식, 수신자 칩 | **미측정.** 비슷하게 만든 데모 에디터에서 스크립트로 재현 | `Scripted demo · not measured` |

Google Slides와 Google Docs, Gmail, Figma는 이 자료에 전후 측정이 없다. 카드에 속도 수치를 붙이지 않는다. 데모 에디터는 Docs·Slides를 닮은 **자체 제작 화면**이므로 "demo document editor"처럼 실체대로 부른다([demo/README.md](../../demo/README.md)).

## 벤치마크 표

README의 "현재 증거" 표를 영상 기준으로 줄인 것이다. 전체 표와 방법은 [README](../../README.ko.md#현재-증거)와 각 보고서에 있다. 모델은 Codex `gpt-6-luna`, effort `max`, 실행당 상한 900초다. **조건당 한 번의 관찰**이며 일반적인 속도·비용 절감을 입증한 결과가 아니다.

| 사이트 / fixture | 회차·조건 | 작업만 | 힌트 사용 | 시간 / 호출 변화 | 영상에서의 취급 |
|---|---|---:|---:|---:|---|
| Google Calendar | R8 (10-31, L1), 조건당 1회 | 431.108초 / 114 | **237.169초 / 68** | −45% / −40% | 씬 1·3·7 |
| Canva | R7 (10-31, L1 + L2), 조건당 1회 | 473.976초 / 123 | **423.193초 / 106** | −11% / −14% | 씬 3 |
| Medium | 10-16 초기 파일럿, 조건당 1회 | 85.5초 / 10 | **44.3초 / 7** | −48% / −30% | `BEFORE` 배지 |
| 네이버 블로그 | 10-16 초기 파일럿, 조건당 1회 | 84.1초 / 16 | **58.8초 / 12** | −30% / −25% | 팝업 상태가 달라 직접 비교 한계 |
| Rich editor fixture | 수동 recall | 130.3초 / 16 | 66.7초 / 12 | −49% / −25% | README의 통제 에디터 결과 |
| Rich editor fixture | 자동 주입 | 130.3초 / 16 | **55.2초 / 11** | −58% / −31% | README의 통제 에디터 결과 |
| iframe editor fixture | 수동 recall | 195.4초 / 28 | **91.7초 / 18** | −53% / −36% | README의 통제 에디터 결과 |
| iframe editor fixture | 자동 주입 | 195.4초 / 28 | 130.9초 / 17 | −33% / −39%, 오류 3 → 0 | README의 통제 에디터 결과 |
| Google Slides | 스크립트 데모만 | — | — | 미측정 | 속도 수치 금지 |

초기 R7 힌트가 Calendar를 느리게 했고 압축한 R8 힌트가 −45%를 냈다는 점은, **무엇을 기억할지 고르는 문지기(Gatekeeper)가 필요한 이유**를 한 문장으로 설명한다. 시간이 허용하면 씬 7 앞이나 씬 5 도입에 한 컷으로 넣는다.

## 새로 만든 것과 기존 것

마지막 씬에 보이는 표이고, 새 기능이 나오는 모든 씬에는 모서리 배지를 둔다. git 이력의 첫 커밋은 2026-10-27(`e519baa`)이다.

| 해커톤 중에 새로 만든 것 (10-27~11-04) | 그 전부터 있던 것 |
|---|---|
| 독립 프로젝트 전체: Rust 워크스페이스(`wam-core`·`contracts`·`client`·`cli`·`mcp`·`server`·`gatekeeper`), SvelteKit 웹 | 2026-10-16 에디터 파일럿 결과(Medium, 네이버, fixture) |
| Gatekeeper curator와 Meta Model API provider, 도구 10개, 감사 기록, 추론 강도·비용 보고·일일 상한·큐 대기 | `reference/`의 읽기 전용 스냅샷(이전 prototype) |
| 로그인 기반 기여, 점수 원장, 불변 버전 게시, My Page, 원격 OAuth·이메일 로그인 | Arise 브라우저 본체(별도 제품) |
| Calendar·Canva 실사이트 비교, 이전 Medium 파일럿과 통제 에디터 관찰, 데모 에디터와 클립 | |
| 공개 도메인 배포(webagenticmemory.com) | |

해커톤 중 추가된 기능을 말할 때는 "built this week"이라고 하고, 이전 자료에는 `BEFORE`를 붙인다. 트랙(Indie·Startup)에 따라 기존 코드 사용 허용 범위가 다르므로 제출 설명에도 같은 구분을 쓴다.

## 클레임 원장

영상 문구는 이 표 밖으로 나가지 않는다. "허용 표현"에 없는 일반화는 쓰지 않는다.

| # | 영상에 나오는 말 | 근거 | 허용 표현 · 금지 |
|---|---|---|---|
| 1 | Calendar 431초/114 → 237초/68 (−45%/−40%) | [google-calendar.md](../test/benchmarks/google-calendar.md) | 조건(1회·고정 순서·공유 로그인) 병기. 금지: "항상 45% 빠르다" |
| 2 | Canva −11%/−14% | [canva.md](../test/benchmarks/canva.md) | 같은 단서 |
| 3 | Notion은 교정 메모리 재측정 전이라 속도 수치를 공개하지 않는다 | [notion.md](../test/benchmarks/notion.md) | 비교 결과가 나오기 전까지 상태 문구만 사용 |
| 4 | Medium −48%, 네이버 −30% | [benchmarks.md](../test/benchmarks.md) | `BEFORE · 10-16 pilot`. 네이버는 통제된 A/B가 아님 |
| 5 | 검토 1건 약 $0.022 | [meta-model-usage](../design/meta-model-usage.ko.md) 비용 절 | "콘솔 정가 기준 추정, 합성 작업". 금지: 실제 청구서라고 말하기 |
| 6 | 58/60, 주입 12/12, 상충 12/12 | 같은 문서 평가 절 | "단일 실행, 합성·템플릿 기반 코퍼스, 놓친 2건은 사람에게 넘김". 금지: 독립 라벨 품질 게이트 통과 |
| 7 | 도구 10개, 브라우저·셸·DB 권한 없음 | 같은 문서 요약 절 | 그대로 |
| 8 | 읽기에는 모델 비용이 없다 | 서버에서 `LlmProvider`를 쓰는 곳은 `durable/worker.rs` 한 곳 | "recall은 카탈로그 조회" |
| 9 | 게시 +20, 다른 로그인 사용자의 성공 +2, 익명은 점수 없음 | [README 구현 상태](../../README.ko.md#구현-상태), [score.md](../spec/score.md) | 일일 상한이 있다는 점은 빼먹지 않는다 |
| 10 | Muse로 돌린 에이전트가 WAM을 호출한다 | [`hosts/meta-muse.md`](../test/hosts/meta-muse.md) (2026-11-01, Muse Code 1.4.14, Claude Code 2.1.316) | 호스트 이름과 "읽기 전용 시험 호출"을 같이 말한다. 금지: Codex에 Muse를 붙인 구성이 된다고 말하기, OAuth·브라우저 작업까지 검증했다고 말하기 |
| 11 | webagenticmemory.com이 열려 있다 | #159 (2026-11-01 공개 HTTPS·MCP 10개·status·recall 검사) | 촬영 직전 재확인 |
| 12 | 이번 주에 만들었다 | 첫 커밋 2026-10-27, [meta-model-usage](../design/meta-model-usage.ko.md) 해커톤 메모 | `BEFORE` 자료는 따로 표시 |
| 13 | 실제 호출 5건, Muse Spark 모델 턴 20번, 모델 비용 $0.049, 전부 PASS | `demo/video/captures/curate-all.txt`(2026-11-01 실행 출력) | "합성 시나리오, 1회 실행"을 같이 적는다. 이 비용은 시드 5건의 값이라 문서의 $0.022(검토 1건 평균 추정)와 다르다. 캐시 적중과 작은 시드 때문이며 둘을 섞어 말하지 않는다 |

## 촬영 전 갭

| 우선 | 갭 | 영향 | 처리 |
|---|---|---|---|
| 🔴 | **운영 카탈로그가 비어 있다**(#159, 2026-11-01에 Muse Code로 공개 주소를 호출해 `hits: []` 확인). 개발 Example을 승인 메모리로 복사하지 않았다 | 공개 주소에서 recall 장면이 힌트 없이 나온다 | 씬 5에서 게시한 실제 버전을 쓰거나, 검증한 메모리를 운영에 승인해 둔다. `Example:`을 검증 지식처럼 보이면 안 된다 |
| 🔴 | **운영에 replay worker가 없다**(#159). fake replay로 대체 금지 | 씬 4·5의 기여→게시 흐름을 운영에서 못 돌린다 | 로컬 리그에서 촬영하고 `replay evidence: fixture` 라벨. 또는 worker를 가동 |
| 🟡 | 저장소 공개와 라이선스. 저장소는 공개이고 Apache-2.0 LICENSE·NOTICE·CODE_OF_CONDUCT를 추가했다(2026-11-01). `reference/`는 라이선스 대상에서 제외했고 재배포 권리 검토는 아직 미완(#78) | 제출 요건이 "공개 저장소"다 | 제출 전에 `reference/`를 남길지 결정 |
| 🟡 | My Page에 기여 결과·메모리/버전 링크가 없다(#162, #164) | 씬 6에서 보여 줄 수 있는 범위가 원장까지다 | 원장 줄까지만 보이고 말하지 않는다 |
| 🟡 | 이메일 로그인 실수신, GitHub 실계정 여정 미검증(#157, #159) | 로그인 장면에 쓸 방식 | 로컬 리그의 합성 OAuth로 촬영하고 화면에 라벨 |
| 🟡 | Muse 호스트 가이드 탭과 랜딩 설치 안내, Arise의 Muse 지원 (검증은 끝남) | 씬 2, 6 | 이번 작업(#170). 끝나면 이 줄을 갱신 |
| 🟡 | 모델 트레이스를 웹 UI에서 보여 주는 화면 없음 | 해커톤 TODO 항목 | 터미널 트레이스로 대체(씬 5). UI 패널은 선택 |
| 🟡 | 반복 측정(조건별 3~5회)과 토큰 기록 | 경제성 칸에 달러 값을 못 쓴다 | 시간이 되면 Calendar·Canva를 3회씩. 못 하면 단일 관찰 라벨 유지 |
| 🟡 | 배포 경로 변경 가능성(#159 macOS Tunnel, #157 4090 직결) | 촬영한 주소가 바뀔 수 있다 | 촬영 직전 헬스체크 |

## 제작 사양과 보안

- **포맷.** 1920×1080, 30fps, H.264, 영어 자막 굽기, 마스터 −14 LUFS 안팎. 자막은 한 번에 2줄 이하, 글자 크기는 1080p에서 최소 36px. 한계·라벨 문구도 2초 이상 정지.
- **터미널.** 폰트 20pt 이상, 어두운 배경, 명령은 미리 입력해 두지 않고 짧게 친다(길면 붙여 넣기 후 컷).
- **녹화 환경.** 알림 끄기, 개인 계정·메일 주소·API 키·`.env`가 화면에 나오지 않는 새 터미널과 새 브라우저 프로필. `demo/record`의 녹화 방식처럼 영역만 잘라 녹화한다. 프레임을 훑어서 시스템 알림이 안 찍혔는지 확인한다.
- **키.** `MODEL_API_KEY`는 화면·자막·로그에 나오지 않는다. 로그를 보여야 하면 키가 없는 줄만 잘라서 쓴다.
- **외부 자산.** 제3자 로고, 로그인된 개인 계정 화면, 제3자 사이트의 비공개 콘텐츠는 쓰지 않는다(규정의 "권리 없는 자산" 금지). 폰트는 OFL(Manrope, DM Sans, DM Mono), 기존 클립과 브랜드 자산은 `web/static/media`, `docs/assets`에 있다.
- **스크립트 클립 표시.** `demo-*.mp4`는 스크립트 재생이지 에이전트 실행이 아니다. 쓰면 `Scripted demo` 라벨을 붙인다.
- **쓰지 말 것.** "항상 더 빠르다", 실제 에이전트가 한 것처럼 보이는 스크립트 클립, 가짜 모델로 만든 트레이스, Wonder·오픈 웨이트 사용 주장, 운영 replay가 돌고 있다는 암시.

## 일정

오늘은 2026-11-01(일)이다. 마감은 11-05 11:00 KST이고 11-04 KST 18:00까지 제출을 목표로 한다.

| 날짜 (KST) | 할 일 |
|---|---|
| 11-01 (일) | 구성안 확정. Muse 호스트 검증, `/guide` Muse 탭, 랜딩 설치 안내. 갭 표의 🔴 착수 |
| 11-02 (월) | 로컬 리그 구성과 시드, 씬 4~6 촬영(터미널 트레이스·UI·호스트), 필요하면 Muse 호스트 B-roll |
| 11-03 (화) | 씬 1·3·7·8 모션 카드 제작, 내레이션 녹음, 러프컷으로 2:50 확인 |
| 11-04 (수) | 최종 편집·자막·렌더, 업로드와 링크 접근 확인, 제출, X 게시 |

### 제출 체크리스트

- [ ] 영상 길이 3:00 미만(2:55 이하), 업로드 후 재생 길이를 다시 확인
- [ ] 영상 링크가 로그아웃 상태에서 열린다
- [ ] 저장소가 공개이고 라이선스·`reference/` 권리 결정이 끝났다(#78)
- [ ] 제출 설명에 새것·기존 구분표를 같이 쓴다. 트랙(Indie·Startup)을 확인한다
- [ ] 참여자 전원이 제출 페이지에 기재돼 있다. 대표자 한 명이 제출한다
- [ ] 영상의 모든 수치가 [클레임 원장](#클레임-원장)과 일치한다
- [ ] Meta Model API 크레딧을 청구했다(마감 11-04 21:00 ET). 평가와 데모 호출 비용을 확인했다
- [ ] 공개 주소(웹·API·MCP)가 영상 공개 시점에 열려 있다
- [ ] X에 데모를 올리고 링크를 제출 플랫폼에 등록했다(Most Viral)

## 부록 A. 영어 내레이션 최종본

복사해서 녹음용으로 쓴다. 약 350단어이며 150wpm이면 2분 20초 안팎이다(실제 속도는 리허설로 확인한다). 나머지 30초는 화면만 나오는 구간과 쉼이다. 씬 표시(`[S1]`)는 읽지 않는다.

```
[S1]
Same Google Calendar task, same agent. 431 seconds, 114 tool calls.
With shared memory: 237 seconds, 68 calls.

[S2]
Browser agents relearn every website from zero. Web Agentic Memory is a shared memory
they read over MCP, live now at webagenticmemory dot com.

[S3]
Think of the work teams already hand to agents. Exporting one Canva page as PNG and PDF.
Scheduling recurring Google Calendar sessions across time zones. Saving a Medium draft.
Single-run observations showed improvements for Calendar, Canva and an earlier Medium pilot.
Notion is awaiting a corrected remeasurement, so no timing is shown.

[S4]
Anyone can contribute a workflow, with consent, and only as a closed schema:
URL pattern, task, normalized steps. No page text. Server-side checks screen it and
replay it before any model sees it.

[S5]
Then Meta's Muse Spark 1.3, through the Meta Model API, decides what's worth remembering.
It has ten bounded tools, read, compare, decide, and no browser, shell, or database.
Here it replaces an older workflow with a shorter one; every call is logged with provider
and model. Duplicate? Support it. Conflict? Escalate to a human. And when a contribution
says "retire the active memory"? It escalates instead of obeying. Fifty-eight of sixty on
our labeled corpus, twelve of twelve on injections. The server enforces scope, versions,
and evidence no matter what the model says.

[S6]
Next, any agent reads it. Here Muse Spark itself, in Muse Code, recalls the hint over MCP.
The author earns points only when another signed-in user succeeds. Anonymous reads get
the hint, but no receipt, and no points.

[S7]
The economics: Muse Spark reviews a contribution for about two cents, and nothing for exact
duplicates or for reads. A daily cap and a durable queue keep it safe when credit runs out.
A verified hint then saves every later run: Calendar, a hundred ninety-four seconds and
forty-six fewer calls, each time. Pay once. Reuse every run.

[S8]
Built this week, except where labeled: the Gatekeeper, the ledger, the MCP service, the web.
Earlier pilot results are marked. Try it at webagenticmemory dot com.
Muse decides. Every agent follows.
```

씬별 단어 수(숫자는 읽는 대로 센다): S1 24, S2 24, S3 53, S4 33, S5 93, S6 38, S7 57, S8 31, 합계 353. S6의 "in Muse Code"는 검증 결과에 따라 호스트 이름을 바꾼다.

## 부록 B. X 게시용 짧은 컷

Most Viral 보너스는 X의 참여도(좋아요·리포스트·댓글)로 가린다. 제출 영상과 따로 **약 40초** 컷을 만든다. 계정별 업로드 길이 한도를 확인한다.

| 시간 | 내용 |
|---|---|
| 0:00~0:10 | 씬 1 (431초 → 237초) |
| 0:10~0:24 | 씬 5의 주입 사례: 기여 문장이 "retire the active memory"인데 모델이 따르지 않고 escalate |
| 0:24~0:34 | 씬 7의 3칸 카드 |
| 0:34~0:40 | 주소와 `Muse decides. Every agent follows.` |

게시 문구는 1) 한 줄 메시지, 2) 수치 한 개와 그 조건, 3) 주소, 4) 저장소 순으로 쓴다. 수치는 클레임 원장 밖으로 나가지 않는다.
