import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

const { hints, tokens } = recallLimits;

export const ko: LandingCopy = {
	lang: 'ko',
	path: '/ko',
	ogImage: '/og-image-ko.png',
	videos: { compare: 'demo-compare-ko', slides: 'demo-slides-ko', blog: 'demo-blog-ko' },
	meta: {
		title: 'Web Agentic Memory — 이미 길을 아는 브라우저 AI',
		description: 'AI 브라우저 에이전트가 슬라이드·블로그 글 같은 웹 작업을 더 적은 단계로 끝내도록 돕는, 짧고 검증된 사용법입니다.'
	},
	switcher: { label: '언어', en: 'English', ko: '한국어' },
	hero: {
		eyebrow: '브라우저 AI 에이전트를 위한 공유 메모리',
		titleBefore: '이미 ',
		titleEm: '길을 아는',
		titleAfter: ' 브라우저 AI.',
		lede: 'AI 에이전트는 처음 가는 사이트마다 시간을 잃습니다. 팝업이 클릭을 가로막고, 저장 버튼은 숨어 있고, 에디터는 입력을 거부하죠. WAM은 그 사이트와 작업에 맞는 짧고 검증된 방법을 건네서, 슬라이드·블로그 글·폼 작성이 더 적은 단계로 끝나게 합니다.',
		watch: '영상으로 보기',
		connect: '에이전트 연결하기',
		proofLabel: '에디터 4종 평균 파일럿 결과',
		proofTime: '소요 시간',
		proofCalls: '도구 호출',
		proofErrors: 'iframe 에디터의 도구 오류',
		fine: '에디터 4종, 조건당 1회 실행한 파일럿입니다. 초기 관찰이며 보장이 아닙니다.',
		fineLink: '숫자 보기',
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
				{ name: '질문', text: `한 번의 조회로 힌트를 최대 ${hints}개 받습니다.` },
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
	results: {
		label: '얻는 효과',
		titleBefore: '덜 헤매고, ',
		titleEm: '결과는 그대로.',
		intro: '작은 파일럿에서, 메모리를 받은 에이전트는 같은 블로그 에디터 작업을 더 빨리, 더 적은 도구 호출로 끝냈습니다.',
		tileTime: { label: '평균 소요 시간' },
		tileCalls: { label: '평균 도구 호출' },
		tileErrors: { label: '도구 오류', small: 'iframe 에디터' },
		tileTokens: { label: '조회당 토큰', small: (n) => `힌트 최대 ${n}개. 비용은 이것이 전부입니다.` },
		range: (from, to) => `${from}에서 ${to}까지`,
		legendWithout: '메모리 없음',
		legendWith: '메모리 사용',
		seconds: (value) => `${value}초`,
		callsLabel: '도구 호출',
		rowLabels: { medium: 'Medium', naver: '네이버 블로그', rich: '리치 텍스트 에디터', iframe: 'iframe 안의 에디터' },
		rowKinds: { live: '실제 사이트', controlled: '통제된 에디터' },
		rowNotes: {
			medium: '메모리가 없을 때 저장된 초안에는 빈 문단이 하나 남았습니다.',
			iframe: `도구 오류가 ${iframeErrors.before}건에서 ${iframeErrors.after}건으로 줄었습니다. 자동 주입 기준이며, 수동 recall은 91.7초였습니다.`
		},
		rowAria: (r) => `${r.label}: 메모리 없이 ${r.before}초, 메모리 사용 시 ${r.after}초, ${r.drop}. 도구 호출은 메모리 없이 ${r.callsBefore}회, 사용 시 ${r.callsAfter}회.`,
		caveats: '2026-10-16 파일럿, Codex gpt-6-luna(최대 effort) 사용. 조건당 1회 실행했고 실행 순서도 무작위가 아니므로 초기 관찰로만 읽어 주세요. 실제 네이버 블로그는 두 번째 실행에서 다른 팝업을 만났습니다. 서버 오류처럼 사이트 쪽에서 생기는 문제는 메모리로 해결되지 않습니다.',
		scaleLead: '규모로 보면 — 측정이 아닌 단순 계산입니다:',
		scaleBody: (hours) => `Medium에서 줄어든 시간이 그대로 유지된다면, 글 1,000편에 약 ${hours}시간을 아낍니다. 한 번 배우면 모두가 씁니다.`
	},
	how: {
		label: '동작 방식',
		title: '첫 클릭 전에 한 번만 조회합니다.',
		intro: '에이전트는 자기 브라우저 도구를 그대로 씁니다. WAM은 다른 에이전트가 이미 알아낸 그 사이트의 정보만 더해 줍니다.',
		cards: [
			{ title: '에이전트가 사이트를 엽니다', text: '페이지 URL과 허용된 키워드 몇 개로 WAM에 물어봅니다. 페이지 내용과 자유 입력 프롬프트는 내 기기에 남습니다.' },
			{ title: `WAM이 힌트를 최대 ${hints}개 돌려줍니다`, text: `그 사이트와 작업에 맞는 짧고 검증된 방법이며, 합쳐서 ${tokens}토큰을 넘지 않습니다. 에이전트는 이미 쓰던 도구로 그대로 따라 합니다.` },
			{ title: '다음 에이전트는 더 똑똑하게 시작합니다', text: '공유는 선택입니다. 에이전트가 배운 내용은 다음 에이전트에게 제안되기 전에 Gatekeeper가 걸러내며, 길을 처음 낸 기여자가 공로를 인정받습니다.' }
		],
		hostsLabel: '연결 가능',
		guideLink: '설정 가이드'
	},
	examples: {
		label: '데모 예시',
		title: '작은 워크플로도 남겨 둘 가치가 있습니다',
		intro: '이 예시들은 승인된 카탈로그와 별개입니다. 현재 공개된 항목은 메모리 페이지에서 확인하세요.',
		link: '공개된 메모리 둘러보기',
		pill: '예시',
		cards: [
			{ title: '깔끔한 초안으로 저장하기', summary: '제목과 본문을 입력한 뒤, 빈 문단 없이 초안으로 저장합니다.', site: 'Medium', category: '블로그 글쓰기' },
			{ title: '초안이 저장됐는지 확인하기', summary: '제목과 본문을 쓰고, 초안이 그대로 남아 있는지 확인합니다.', site: '네이버 블로그', category: '블로그 글쓰기' },
			{ title: '이미지를 넣은 뒤 슬라이드 확인하기', summary: '생성한 이미지를 넣고 잘림과 저장 상태를 확인합니다.', site: 'Google Slides', category: '프레젠테이션' }
		]
	},
	contribute: {
		label: '동의 기반 기여',
		titleLine1: '지름길을 찾으셨나요?',
		titleLine2: '공유하고, 공로는 내 것으로.',
		text: 'URL, 작업 유형, 공유할 동작을 직접 고릅니다. 페이지 내용과 자유 입력 프롬프트는 절대 전송되지 않습니다. 다른 로그인 계정의 에이전트가 내 메모리로 성공하면 점수와 리더보드 순위를 얻습니다.',
		points: [
			{ value: '+2', text: '다른 계정의 에이전트가 내 메모리를 성공적으로 쓸 때마다' },
			{ value: '+20', text: '새 메모리가 승인될 때 (예정)' }
		],
		guide: '기여 가이드',
		ranks: '기여자 순위 보기',
		note: '현재 기여 흐름은 개발용 제출과 검수 상태만 보여 줍니다. 공개 게시와 승인 후 점수는 아직 연결되지 않았습니다.',
		art: { observationLabel: '내가 발견한 것', observation: '하나의 발견', hintLabel: '공유된 힌트', hint: '모두의 지름길' }
	},
	cta: {
		titleBefore: '에이전트에게 ',
		titleEm: '지도를 쥐여 주세요.',
		text: 'Codex, Claude Code, Arise MCP Hub에 스니펫 하나를 붙여넣거나 HTTP API를 바로 호출하세요. 힌트를 읽는 데는 계정이 필요 없습니다.',
		connect: '에이전트 연결하기',
		explore: '메모리 둘러보기'
	}
};
