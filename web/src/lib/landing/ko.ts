import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

export const ko: LandingCopy = {
	lang: 'ko',
	path: '/ko',
	ogImage: '/og-image-ko.png',
	videos: { compare: 'demo-compare-ko', slides: 'demo-slides-ko', blog: 'demo-blog-ko', docs: 'demo-docs-ko', sheets: 'demo-sheets-ko', email: 'demo-email-ko', forms: 'demo-forms-ko', docsCompare: 'demo-docs-compare-ko' },
	meta: {
		title: 'Web Agentic Memory — 웹 기반 워크플로 공유 메모리',
		description: '웹사이트와 웹앱에서 하는 작업을 위한 검증된 공유 메모리입니다. AI 에이전트가 MCP 또는 WAM CLI로 URL과 작업에 맞는 힌트를 받아, 기존 브라우저 도구로 이어서 수행합니다.'
	},
	switcher: { label: '언어', en: 'English', ko: '한국어' },
	hero: {
		eyebrow: '웹 기반 워크플로를 위한 공유 메모리',
		titleBefore: '이미 ',
		titleEm: '길을 아는',
		titleAfter: ' 브라우저 AI.',
		lede: 'Web Agentic Memory(WAM)는 여러 웹사이트와 웹앱에서 하는 작업에 재사용할 수 있는 공유 메모리입니다. 에이전트는 MCP 또는 WAM CLI로 사이트와 작업에 맞는 짧고 검증된 힌트를 받은 뒤, 이미 쓰는 브라우저 도구로 작업합니다.',
		watch: '영상으로 보기',
		connect: '에이전트 연결하기',
		github: 'GitHub에서 보기',
		availability: '공개 힌트는 계정 없이 볼 수 있습니다. 브라우저 도구는 쓰던 그대로 쓰세요.',
		benchmarks: '실제 서비스 벤치마크 보기',
		videoLabel: '같은 블로그 작업을 두 번 나란히 실행한 영상입니다. 메모리가 없으면 에이전트는 막힌 클릭과 빈 문단을 만나고, WAM 힌트가 있으면 힌트 세 개를 따라 깔끔하게 저장합니다.',
		videoCaption: '같은 작업을 두 번 실행했습니다. 데모 에디터에서 녹화한 스크립트 리플레이입니다.'
	},
	workflows: {
		label: '웹 기반 워크플로',
		title: 'Web Agentic Memory로 어떤 일을 할 수 있을까요?',
		intro: '문서 작성, 디자인, 생산성 도구, 웹 조사 등 브라우저에서 하는 업무를 둘러보세요. 에이전트는 작업 전에 검증된 힌트를 조회하고 기존 브라우저 도구로 이어서 작업합니다. 영상은 실제 서비스 화면이 아닌 데모 에디터를 스크립트로 재생한 예시입니다.',
		askLabel: '요청',
		hintsLabel: 'WAM 힌트',
		getLabel: '결과',
		compareLabel: '힌트 없을 때와 비교하기',
		flowLabel: '힌트 영상으로 돌아가기',
		libraryText: '다른 사이트나 작업이 필요하세요? 문서 템플릿, 업무 일지, 이메일, 캘린더 같은 기본 예시를 둘러보세요.',
		libraryLink: '메모리 라이브러리 열기',
		chips: { label: 'Medium 파일럿', time: (percent) => `시간 ${percent}`, calls: (percent) => `도구 호출 ${percent}`, note: 'Medium 파일럿 · 1회 실행' },
		items: [
			{
				id: 'slides',
				kicker: '슬라이드',
				title: '발표 자료를 부탁하면, 깔끔한 슬라이드가 나옵니다',
				ask: '웹 에이전트를 위한 공유된 길을 주제로 3장짜리 영어 슬라이드를 만들고, 그림을 하나 넣어 줘.',
				hints: ['제목·본문 칸에 바로 입력', '삐져나온 그림은 크기 조절', '“All changes saved”가 뜰 때까지 대기'],
				result: '슬라이드 3장, 그림은 슬라이드 안에, 자동 저장까지 확인.',
				note: '아직 벤치마크 전이라 속도 주장은 하지 않습니다.',
				videoLabel: 'AI 에이전트가 데모 슬라이드 에디터에서 3장짜리 슬라이드를 만드는 영상입니다. WAM에서 힌트 세 개를 받고, 제목·본문 칸에 입력하고, 슬라이드 밖으로 나간 그림을 줄이고, All changes saved가 뜰 때까지 기다립니다.'
			},
			{
				id: 'docs',
				kicker: '문서',
				title: '브리프를 부탁하면, 구조를 갖춘 문서가 나옵니다',
				ask: '계획과 간단한 일정표가 들어간 한 장짜리 영어 브리프를 작성해 줘.',
				hints: ['쓰기 전에 문서 이름부터 정하기', '제목은 스타일 메뉴, 목록은 글머리 버튼', '표는 Tab으로 칸 이동'],
				result: '이름이 있고, 제목·글머리 목록·표를 갖춘 문서. 저장되었고 비공개 그대로입니다.',
				note: 'Google Docs에서 신청서·회의록·제안서 양식을 만드는 메모리도 있습니다. 문서 작업은 아직 벤치마크 전입니다.',
				memoryHref: '/memories',
				memoryLabel: '문서 양식 메모리 둘러보기',
				videoLabel: 'AI 에이전트가 데모 문서 에디터에서 한 장짜리 브리프를 작성하는 영상입니다. WAM에서 힌트 세 개를 받고, 문서 이름을 정하고, 스타일 메뉴로 제목을 넣고, 글머리 목록을 만들고, Tab으로 표를 채운 뒤 저장될 때까지 기다립니다.',
				compareVideo: 'docsCompare',
				compareVideoLabel: '같은 브리프 작업을 데모 문서 에디터에서 나란히 두 번 실행한 영상입니다. 메모리가 없으면 에이전트는 제목을 본문에 입력하고 #과 -로 서식이 적용될 거라 기대했다가 세 번을 돌아가 고칩니다. WAM 힌트가 있으면 문서 이름부터 정하고, 스타일 메뉴와 글머리 버튼을 쓰고, Tab으로 표를 채웁니다. 실제 Google Docs의 마크다운 단축키 동작은 다를 수 있습니다.'
			},
			{
				id: 'blog',
				kicker: '블로그 글',
				title: '글을 부탁하면, 깔끔한 초안이 저장됩니다',
				ask: '공유된 길(trail)에 대한 짧은 영어 글을 쓰고 초안으로 저장해 줘.',
				hints: ['팝업부터 닫기', '문단 사이에는 Enter 한 번', '미리보기 확인 후 초안 저장'],
				result: '빈 문단 없는 초안이 저장됩니다. 발행은 하지 않습니다.',
				videoLabel: 'AI 에이전트가 데모 에디터에서 짧은 블로그 글을 쓰는 영상입니다. WAM에서 힌트 세 개를 받고, 팝업을 먼저 닫고, 문단 사이에 Enter를 한 번만 누르고, 미리보기를 확인한 뒤 발행 없이 초안으로 저장합니다.',
				compareVideo: 'compare',
				compareVideoLabel: '같은 블로그 작업을 두 번 나란히 실행한 영상입니다. 메모리가 없으면 에이전트는 막힌 클릭과 빈 문단을 만나고, WAM 힌트가 있으면 힌트 세 개를 따라 깔끔하게 저장합니다.'
			},
			{
				id: 'sheets',
				kicker: '스프레드시트',
				title: '시트를 부탁하면, 계산된 합계가 나옵니다',
				ask: '1월·2월 항목 3개와 합계가 들어간 간단한 월별 예산표를 만들어 줘.',
				hints: ['Tab은 오른쪽, Enter는 다음 행', '수식은 =로 시작', '“Saved” 확인 뒤 합계 점검'],
				result: '모든 행이 채워지고, 수식은 텍스트가 아닌 계산 결과로, 합계도 맞습니다.',
				note: '아직 벤치마크 전이라 속도 주장은 하지 않습니다.',
				videoLabel: 'AI 에이전트가 데모 스프레드시트에 간단한 예산표를 채우는 영상입니다. WAM에서 힌트 세 개를 받고, Tab과 Enter로 행을 채우고, 수식을 =로 시작하고, Saved가 뜬 뒤 합계를 확인합니다.'
			},
			{
				id: 'email',
				kicker: '이메일',
				title: '메일을 부탁하면, 저장된 초안이 나옵니다',
				ask: '금요일 트레일 데모 안내 메일을 Ana와 Ben에게 보낼 영어 초안으로 써 줘. 전송은 하지 마.',
				hints: ['주소마다 Enter 누르기', '서명 위의 첫 줄을 클릭', '제목을 넣고 임시 저장'],
				result: '받는 사람 2명, 제목, 서명 위의 본문이 들어간 초안이 저장됩니다. 전송은 하지 않습니다.',
				note: '아직 벤치마크 전이라 속도 주장은 하지 않습니다.',
				videoLabel: 'AI 에이전트가 데모 메일 작성기에서 이메일 초안을 쓰는 영상입니다. WAM에서 힌트 세 개를 받고, 주소마다 Enter를 눌러 칩으로 만들고, 서명 위에 본문을 쓰고, 제목을 넣은 뒤 전송 없이 초안으로 저장합니다.'
			},
			{
				id: 'forms',
				kicker: '폼',
				title: '폼을 부탁하면, 작성된 신청서가 나옵니다',
				ask: '휴가 신청 폼을 작성해 줘. 11월 12일부터 휴가, 초안으로 저장.',
				hints: ['휴가 종류는 목록에서 선택', '날짜는 달력으로 지정', '필수 체크박스 선택 뒤 저장'],
				result: '필수 항목을 모두 채우고, 달력으로 날짜를 지정한 뒤 초안으로 저장합니다. 제출은 하지 않습니다.',
				note: '아직 벤치마크 전이라 속도 주장은 하지 않습니다.',
				videoLabel: 'AI 에이전트가 데모 웹 폼에서 휴가 신청서를 작성하는 영상입니다. WAM에서 힌트 세 개를 받고, 목록에서 휴가 종류를 고르고, 달력으로 날짜를 지정하고, 필수 체크박스를 선택한 뒤 제출 없이 초안으로 저장합니다.'
			}
		]
	},
	productivity: {
		label: '실제 서비스 벤치마크',
		title: 'Google Calendar와 Canva를 실제 작업으로 비교했습니다.',
		intro: '각 조건을 한 번씩 실행한 비교에서 힌트 사용 시 Calendar 시간은 45%, Canva는 11% 줄고 외부 도구 호출도 두 사이트 모두 줄었습니다. Notion은 메모리 재측정 전이라 표에서 제외했습니다. 일반적인 속도 보장은 아닙니다.',
		scope: (date, browser, model, limit) => `${date} · macOS의 ${browser} · ${model} · 실행당 ${limit}초 제한 · 조건당 1회`,
		passed: '두 조건 모두 통과',
		metric: '측정 항목',
		taskOnly: '작업만 제공',
		withHints: 'WAM 힌트 제공',
		time: '소요 시간',
		calls: '외부 도구 호출',
		seconds: (value) => `${value.toFixed(3)}초`,
		report: '전체 기록 (.md)',
		workflows: {
			'google-calendar': '시간대를 지정해 매주 반복 일정 세 회차를 만들고, 한 회차만 이동한 뒤 세 일정을 모두 다시 열어 확인했습니다.',
			canva: '무료 템플릿에서 글과 이미지를 교체하고, 두 번째 페이지만 PNG와 한 페이지 PDF로 내보내 실제 파일을 검사했습니다.'
		},
		caveats: '실행 순서는 고정이었고(무작위 아님), 같은 로그인 브라우저 프로필을 공유했습니다. Calendar(R8 · L1)는 짧은 힌트만, Canva(R7 · L1 + L2)는 상세 내용까지 제공해 두 회차를 직접 비교할 수 없습니다. “외부” 도구 호출은 최상위 도구 단계만 셉니다. 준비·검증·정리는 수치에서 제외했습니다. 일반적인 속도·비용 절감이나 자동 연동을 입증한 결과는 아닙니다. Google Docs·Google Flow·Google Slides·Gmail·Figma 메모리 예시는 시간 비교를 측정하지 않았습니다.',
		unmeasured: '— 스크립트 데모만 있습니다. Google Docs·Google Flow·Google Slides·Gmail·Figma 예시도 시간 비교 벤치마크가 아닙니다.',
		delta: '변화',
		candidate: (variant) => variant === 'M' ? 'WAM 힌트' : `후보 ${variant.slice(1)}`
	},
	results: {
		label: '이전 에디터 파일럿',
		titleBefore: '덜 헤매고, ',
		titleEm: '결과는 그대로.',
		intro: '작은 파일럿에서, 힌트를 받은 에이전트는 같은 블로그 에디터 작업을 더 빨리, 더 적은 도구 호출로 끝냈습니다.',
		scope: (editors) => `에디터 ${editors}종 · 조건당 1회 · 참고용 초기 관찰`,
		tileTime: { label: '평균 소요 시간' },
		tileCalls: { label: '평균 도구 호출' },
		tileErrors: { label: '도구 오류', small: 'iframe 에디터' },
		tileTokens: { label: '힌트당 추정 토큰', small: (n) => `조회당 최대 ${n}개. 응답 메타데이터와 상세 조회는 별도입니다.` },
		range: (from, to) => `에디터별 ${from}에서 ${to}까지`,
		legendWithout: '메모리 없음',
		legendWith: '메모리 사용',
		seconds: (value) => `${value}초`,
		callsLabel: '도구 호출',
		rowLabels: { medium: 'Medium', naver: '네이버 블로그', rich: '리치 텍스트 에디터', iframe: 'iframe 안의 에디터' },
		rowKinds: { live: '실제 사이트', controlled: '테스트 에디터' },
		rowNotes: {
			medium: '메모리가 없을 때 저장된 초안에는 빈 문단이 하나 남았습니다.',
			iframe: `도구 오류가 ${iframeErrors.before}건에서 ${iframeErrors.after}건으로 줄었습니다. 막대는 자동 주입 기준이며, 직접 조회했을 때는 91.7초였습니다.`
		},
		rowAria: (r) => `${r.label}: 메모리 없이 ${r.before}초, 메모리 사용 시 ${r.after}초, ${r.drop}. 도구 호출은 메모리 없이 ${r.callsBefore}회, 사용 시 ${r.callsAfter}회.`,
		caveats: '2026-10-16 파일럿, Codex gpt-6-luna(최대 effort) 사용. 실행 순서는 무작위가 아니었습니다. 실제 네이버 블로그는 두 번째 실행에서 다른 팝업을 만나 그 행은 통제된 비교가 아닙니다. 서버 오류처럼 사이트 쪽에서 생기는 문제는 메모리로 해결되지 않습니다.'
	},
	how: {
		label: '시작하기',
		title: '연결하고, 첫 힌트를 받아보세요.',
		intro: '브라우저와 에이전트를 준비한 뒤, WAM으로 공유 작업 힌트를 더하세요.',
		browser: {
			title: '에이전트가 쓸 브라우저부터 준비하세요.',
			text: '내장 에이전트와 브라우저 MCP가 있는 Arise Browser를 내려받거나, Chrome에 브라우저 MCP를 연결하세요. 에이전트가 작업하는 동안 브라우저를 열어 두세요.',
			arise: 'Arise Browser 다운로드',
			chrome: 'Google Chrome 다운로드',
			guide: '브라우저 도구 연결 방법'
		},
		cards: [
			{ title: '에이전트와 브라우저를 고릅니다', text: 'Muse Code·Claude Code·Codex에 브라우저 도구를 연결하거나, Arise의 내장 에이전트를 쓰세요.' },
			{ title: 'WAM을 추가합니다', text: '명령 하나를 붙여 넣고 WAM 도구가 보이는지 확인합니다.' },
			{ title: '작업 전에 힌트를 요청합니다', text: '에이전트에게 지금 사이트와 작업에 맞는 힌트를 찾아 달라고 하세요. 연결만으로는 백그라운드 조회가 시작되지 않습니다.' }
		],
		hostsLabel: '연결 가능',
		guideLink: '설치 안내',
		quick: {
			label: '빠른 설치',
			title: '내 에이전트에 맞는 설정을 복사하세요.',
			intro: '브라우저와 에이전트를 설치한 다음 원격 MCP로 WAM을 추가하세요. 공개 힌트에는 WAM 다운로드나 계정이 필요 없습니다.',
			tabsLabel: '에이전트 선택',
			museInstall: 'Muse Code 설치 · 터미널',
			museConfig: '~/.config/muse/settings.json에 추가',
			museNote: 'muse를 실행하고 /mcp로 web-memory가 보이는지 확인한 뒤 힌트를 요청하세요. Muse Code는 Muse Spark를 쓰는 Meta의 코딩 에이전트입니다. Claude Code에서 Muse Spark를 쓰려면 설치 안내의 Meta Model API 설정을 보세요.',
			claudeLabel: 'Claude Code · 원격 MCP 추가',
			claudeNote: '실행한 뒤 새 세션을 열고 힌트를 요청하세요.',
			codexLabel: 'Codex · 원격 MCP 추가',
			codexNote: '같은 서버, 같은 도구입니다. Codex는 자체 모델로 동작합니다.',
			more: 'Windows, 계정 로그인, 로컬 도구, 확인 방법'
		}
	},
	examples: {
		label: '힌트는 이렇게 생겼습니다',
		title: '짧은 힌트 하나가, 다음 행동으로.',
		intro: '힌트 하나가 초안 저장 작업을 어떻게 이끄는지 보여 드립니다. 설명용 예시이며, 실제 카탈로그 결과가 아닙니다.',
		link: '메모리 라이브러리 둘러보기',
		pill: '설명용 예시',
		contextLabel: '에이전트가 조회하는 정보',
		urlLabel: '공개 URL',
		taskLabel: '작업',
		task: '글을 쓰고 초안으로 저장',
		keywordsLabel: '허용 키워드',
		hintLabel: 'WAM이 돌려주는 힌트',
		hintTitle: '발행 없이 초안 저장하기',
		steps: ['팝업이 편집기를 가리면 먼저 닫습니다.', '제목과 본문을 입력합니다.', '미리보기와 저장 상태를 확인하고 초안으로 남깁니다.'],
		apply: '에이전트는 현재 화면에 맞는 힌트만 적용하고, 자기 브라우저 도구로 작업합니다. 사용자 지시가 우선입니다.',
		limits: (hints, tokensPerHint) => `조회당 최대 ${hints}개, 힌트당 최대 ${tokensPerHint} 추정 토큰입니다.`
	},
	contribute: {
		label: '동의 기반 기여',
		titleLine1: '지름길을 찾으셨나요?',
		titleLine2: '공유하고, 공로는 내 것으로.',
		text: 'URL, 작업 유형, 공유할 동작을 직접 고릅니다. WAM 기여 요청에는 페이지 내용과 자유 입력 프롬프트를 포함하지 않습니다. 다른 로그인 계정의 에이전트가 내 메모리로 성공하면 점수와 리더보드 순위를 얻을 수 있습니다.',
		points: [
			{ value: '+2', text: '다른 계정이 보고한 성공 사용이 지급 조건을 충족할 때' },
			{ value: '+20', text: '새 워크플로가 재현 검증을 통과하고 게시될 때' }
		],
		guide: '기여 가이드',
		ranks: '기여자 순위 보기',
		note: '새 워크플로는 안전 검사와 재현 검증 후 게시됩니다. 중복 기여에는 새 승인 점수가 지급되지 않습니다. 성공 사용 점수에는 명시적 결과 보고와 일일 한도가 적용됩니다.',
		art: { observationLabel: '내가 발견한 것', observation: '하나의 발견', hintLabel: '공유된 힌트', hint: '모두의 지름길' }
	},
	faq: {
		label: '시작 전에',
		title: '궁금한 점을 먼저 확인하세요.',
		items: [
			{ question: '계정이 꼭 필요한가요?', answer: '공개 힌트를 보는 데는 필요 없습니다. 기여·사용 결과 보고·공로 귀속에는 WAM 계정 연결이 필요하고, 기여를 공유할 때는 별도 동의를 받습니다.' },
			{ question: '연결하면 에이전트가 자동으로 조회하나요?', answer: '에이전트가 조회 도구를 호출할 때만 WAM이 동작합니다. 연결만으로 백그라운드 조회가 시작되지는 않습니다. 설치 안내에서 언제 메모리를 쓸지 에이전트에게 알려 줄 지시문도 확인할 수 있습니다.' },
			{ question: '브라우저 제어 도구도 제공하나요?', answer: '아니요. 페이지 이동·클릭·입력은 쓰던 브라우저 도구가 맡고, WAM은 힌트만 제공합니다. 설치 안내에는 에이전트가 힌트를 요청할 때 자기 소유의 Arise 탭을 읽는 선택형 연결 방법도 있습니다.' },
			{ question: '내 작업에 맞는 힌트가 없으면요?', answer: '빈 결과는 아직 일치하는 승인 힌트가 없다는 뜻입니다. 에이전트는 평소 쓰던 도구로 현재 화면을 확인하며 계속 작업할 수 있습니다. 모든 사이트와 작업을 다루지는 않습니다.' },
			{ question: '어떤 정보가 WAM으로 전송되나요?', answer: '조회에는 정규화된 공개 URL 패턴, 작업 유형, 허용 키워드를 사용합니다. WAM 요청에는 페이지 본문과 자유 입력 프롬프트를 포함하지 않습니다. 브라우저·모델 서비스가 처리하는 정보는 각 서비스 설정을 따릅니다.' }
		]
	},
	cta: {
		titleBefore: '에이전트에게 ',
		titleEm: '지도를 쥐여 주세요.',
		text: 'Muse Code·Claude Code·Codex는 설정 하나나 명령 하나로 연결할 수 있습니다. 공개 힌트는 계정 없이 볼 수 있습니다.',
		connect: '에이전트 연결하기',
		explore: '메모리 라이브러리 둘러보기'
	}
};
