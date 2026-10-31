import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

export const ko: LandingCopy = {
	lang: 'ko',
	path: '/ko',
	ogImage: '/og-image-ko.png',
	videos: { compare: 'demo-compare-ko', slides: 'demo-slides-ko', blog: 'demo-blog-ko' },
	meta: {
		title: 'Web Agentic Memory — 이미 길을 아는 브라우저 AI',
		description: '브라우저 AI 에이전트가 웹 에디터에서 블로그 글을 쓰고 슬라이드를 만들 때 덜 헤매도록 돕는, 짧고 검증된 사용법입니다.'
	},
	switcher: { label: '언어', en: 'English', ko: '한국어' },
	hero: {
		eyebrow: '브라우저 AI 에이전트를 위한 공유 메모리',
		titleBefore: '이미 ',
		titleEm: '길을 아는',
		titleAfter: ' 브라우저 AI.',
		lede: 'Web Agentic Memory(WAM)는 Codex·Claude Code에 MCP로 연결하는 웹 작업용 공유 메모리입니다. 사이트와 작업에 맞는 짧고 검증된 팁으로, 막힌 클릭과 숨은 저장 버튼 앞에서 덜 헤매도록 돕습니다.',
		watch: '영상으로 보기',
		connect: '에이전트 연결하기',
		availability: '공개 조회는 계정 없이. 브라우저 도구는 쓰던 그대로.',
		benchmarks: '실제 서비스 벤치마크 보기',
		videoLabel: '같은 블로그 작업을 두 번 나란히 실행한 영상입니다. 메모리가 없으면 에이전트는 막힌 클릭과 빈 문단을 만나고, WAM 힌트가 있으면 힌트 세 개를 따라 깔끔하게 저장합니다.',
		videoCaption: '같은 작업, 두 번의 실행. 데모 에디터를 Arise 브라우저에서 녹화한 스크립트 리플레이입니다.'
	},
	demo: {
		label: '작동 영상',
		title: '자주 하는 두 가지 일, 지도를 들고 해냅니다.',
		intro: '각 영상은 로컬 데모 에디터를 Arise 브라우저에서 녹화한 스크립트 리플레이입니다. 힌트는 에이전트가 받는 내용의 예시이며, 실제 카탈로그 항목이 아닙니다.',
		slides: {
			kicker: '슬라이드',
			title: '에디터를 붙들고 있지 않아도 덱이 완성됩니다',
			text: '짧은 덱을 요청하세요. 에이전트는 먼저 WAM에 물어보고, 자리표시자에 바로 입력하고, 슬라이드 밖으로 삐져나온 그림을 줄이고, 자동 저장까지 확인한 뒤에야 작업을 끝냅니다.',
			steps: [
				{ name: '질문', text: `한 번의 조회로 힌트를 최대 ${recallLimits.hints}개 받습니다.` },
				{ name: '작성', text: '자리표시자에 입력해 슬라이드 3장을 만듭니다.' },
				{ name: '수정', text: '그림을 모서리에서 줄여 슬라이드 안에 맞춥니다.' },
				{ name: '확인', text: '끝내기 전에 “All changes saved”를 확인합니다.' }
			],
			fine: '슬라이드 작업은 아직 벤치마크하지 않았으므로, 이 영상에는 속도에 관한 주장이 없습니다.',
			videoLabel: 'AI 에이전트가 데모 슬라이드 에디터에서 3장짜리 덱을 만드는 영상입니다. WAM에서 힌트 세 개를 받고, 자리표시자에 입력하고, 슬라이드 밖으로 나간 그림을 줄이고, All changes saved가 뜰 때까지 기다립니다.'
		},
		blog: {
			kicker: '블로그 글',
			title: 'AI가 글을 쓰고, 깔끔한 초안으로 저장합니다',
			text: '에이전트는 팝업을 먼저 닫고, 문단 사이에 Enter를 한 번만 누르고, 미리보기를 확인한 뒤 초안으로 저장합니다. 발행은 하지 않습니다.',
			steps: [
				{ name: '질문', text: '첫 클릭 전에 이 에디터의 힌트를 받습니다.' },
				{ name: '작성', text: '제목, 그다음 Enter 한 번씩 띄운 문단.' },
				{ name: '확인', text: '미리보기에 빈 블록이 없는지 봅니다.' },
				{ name: '저장', text: '초안으로 저장하고, 발행은 하지 않습니다.' }
			],
			chipsLabel: 'Medium 파일럿',
			chipTime: (percent) => `시간 ${percent}`,
			chipCalls: (percent) => `도구 호출 ${percent}`,
			chipNote: 'Medium 파일럿 · 1회 실행',
			videoLabel: 'AI 에이전트가 데모 에디터에서 짧은 블로그 글을 쓰는 영상입니다. WAM에서 힌트 세 개를 받고, 팝업을 먼저 닫고, 문단 사이에 Enter를 한 번만 누르고, 미리보기를 확인한 뒤 발행 없이 초안으로 저장합니다.'
		}
	},
	productivity: {
		label: '실제 서비스 벤치마크',
		title: 'Google Calendar, Notion, Canva에서 직접 확인했습니다.',
		intro: '후속 R8에서는 수정한 Calendar 힌트로 시간과 호출 감소를 관찰했습니다. 더 느려진 Notion 후보 두 개는 반영하지 않았습니다. 아래 카드는 이전 R7 비교이며, 서비스별 보고서에서 최신 결과와 한계를 확인할 수 있습니다.',
		scope: (date, browser, model, limit) => `R7 · ${date} · macOS의 ${browser} · ${model} · 실행당 ${limit}초 제한`,
		passed: '두 조건 모두 통과',
		metric: '측정 항목',
		taskOnly: '작업만 제공',
		withHints: 'WAM 힌트 제공',
		time: '소요 시간',
		calls: '도구 호출',
		seconds: (value) => `${value.toFixed(3)}초`,
		report: '전체 기록 (.md)',
		workflows: {
			'google-calendar': '시간대를 지정해 매주 반복 일정 세 회차를 만들고, 한 회차만 이동한 뒤 세 일정을 모두 다시 열어 확인했습니다.',
			notion: '비공개 작업 DB에 날짜와 상태를 넣고, 표 정렬과 완료 항목을 제외한 보드 필터를 확인했습니다.',
			canva: '무료 템플릿에서 글과 이미지를 교체하고, 두 번째 페이지만 PNG와 한 페이지 PDF로 내보내 실제 파일을 검사했습니다.'
		},
		interpretation: 'R7의 여섯 작업은 모두 독립 확인을 통과했습니다. Canva는 힌트 조건의 시간과 도구 호출이 줄었고, Google Calendar와 Notion은 늘었습니다.',
		caveats: 'R7은 조건당 한 번씩, 작업만 제공한 조건을 먼저 실행했고 로그인 상태를 공유했습니다. 힌트를 직접 조회해 제공했으므로 WAM 자동 연동 시험은 아닙니다. 준비·독립 확인·정리는 수치에서 제외했습니다. 일반적인 속도·비용 절감 효과를 입증한 결과는 아니며, 서비스별 제약은 각 보고서에 기록했습니다.'
	},
	results: {
		label: '이전 에디터 파일럿',
		titleBefore: '덜 헤매고, ',
		titleEm: '결과는 그대로.',
		intro: '작은 파일럿에서, 메모리를 받은 에이전트는 같은 블로그 에디터 작업을 더 빨리, 더 적은 도구 호출로 끝냈습니다.',
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
		caveats: '2026-10-16 파일럿, Codex gpt-6-luna(최대 effort) 사용. 조건당 1회 실행했고 실행 순서도 무작위가 아니므로 참고용 초기 관찰로만 읽어 주세요. 실제 네이버 블로그는 두 번째 실행에서 다른 팝업을 만났기 때문에, 그 행은 통제된 비교가 아닙니다. 서버 오류처럼 사이트 쪽에서 생기는 문제는 메모리로 해결되지 않습니다.'
	},
	how: {
		label: '시작하기',
		title: '연결하고, 첫 힌트를 받아보세요.',
		intro: '공개 조회는 계정 없이 시작합니다. 이미 쓰는 에이전트와 브라우저 도구에, 다른 에이전트가 알아낸 방법을 더하세요.',
		cards: [
			{ title: '호스트를 고릅니다', text: 'Codex·Claude Code는 MCP로 연결합니다. Arise는 MCP Hub와 선택형 로컬 연결 방법을 가이드에서 확인하세요.' },
			{ title: '가이드대로 WAM을 연결합니다', text: '호스트에 맞는 연결 명령을 추가하고 WAM 도구가 보이는지 확인합니다. 페이지 열기·입력·클릭은 기존 브라우저 도구를 그대로 씁니다.' },
			{ title: '작업 전에 조회를 요청합니다', text: '에이전트에게 현재 사이트와 작업에 맞는 힌트를 찾아 달라고 하세요. 연결만으로 백그라운드 조회가 시작되지는 않습니다.' }
		],
		hostsLabel: '연결 가능',
		guideLink: '설정 가이드'
	},
	examples: {
		label: '페이지에서 다음 행동까지',
		title: '짧은 힌트 하나가, 다음 행동으로.',
		intro: '초안 저장 작업에 힌트를 적용하는 흐름입니다. 실시간 카탈로그 결과와 별개인 설명용 예시입니다.',
		link: '공개된 메모리 둘러보기',
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
			{ question: '계정이 꼭 필요한가요?', answer: '공개 힌트는 계정 없이 조회할 수 있습니다. 기여·사용 결과 보고·공로 귀속에는 WAM 계정 연결이 필요하며, 기여를 공유할 때는 별도 동의를 받습니다.' },
			{ question: '연결하면 에이전트가 자동으로 조회하나요?', answer: 'WAM은 에이전트가 조회 도구를 호출할 때 동작합니다. 연결만으로 백그라운드 조회가 시작되지는 않습니다. 설치 가이드에서 언제 메모리를 사용할지 에이전트에게 알려줄 지시문도 확인할 수 있습니다.' },
			{ question: '브라우저 제어 도구도 제공하나요?', answer: '페이지 이동·클릭·입력은 기존 브라우저 도구가 맡고, WAM은 공유 힌트를 제공합니다. 에이전트가 요청할 때 소유한 Arise 탭을 읽는 선택형 연결 방법도 설치 가이드에서 안내합니다.' },
			{ question: '내 작업에 맞는 힌트가 없으면요?', answer: '빈 결과는 일치하는 승인 힌트가 없다는 뜻입니다. 에이전트는 기존 도구로 현재 화면을 확인하며 작업을 계속할 수 있습니다. 모든 사이트와 작업의 메모리를 보장하지는 않습니다.' },
			{ question: '어떤 정보가 WAM으로 전송되나요?', answer: '조회에는 정규화된 공개 URL 패턴, 작업 유형, 허용 키워드를 사용합니다. WAM 요청에는 페이지 본문과 자유 입력 프롬프트를 포함하지 않습니다. 브라우저·모델 서비스가 처리하는 정보는 각 서비스 설정을 따릅니다.' }
		]
	},
	cta: {
		titleBefore: '에이전트에게 ',
		titleEm: '지도를 쥐여 주세요.',
		text: 'Codex·Claude Code는 원격 MCP 명령으로 연결하세요. Arise MCP Hub와 HTTP API 연결은 설치 안내에서 확인할 수 있습니다. 공개 조회에는 계정이 필요 없습니다.',
		connect: '에이전트 연결하기',
		explore: '공유 워크플로 둘러보기'
	}
};
