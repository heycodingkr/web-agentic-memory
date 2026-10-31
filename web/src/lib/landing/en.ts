import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

export const en: LandingCopy = {
	lang: 'en',
	path: '/',
	ogImage: '/og-image.png',
	videos: { compare: 'demo-compare', slides: 'demo-slides', blog: 'demo-blog' },
	meta: {
		title: 'Web Agentic Memory — Browser AI that already knows the way',
		description: 'Short, verified how-tos that help AI browser agents write blog posts and build slide decks in web editors with fewer detours.'
	},
	switcher: { label: 'Language', en: 'English', ko: '한국어' },
	hero: {
		eyebrow: 'Shared memory for browser AI agents',
		titleBefore: 'Browser AI that already ',
		titleEm: 'knows the way.',
		titleAfter: '',
		lede: 'Web\u00a0Agentic\u00a0Memory (WAM) connects to Codex or Claude Code through MCP. It gives your agent short, verified tips for a website and task, helping it avoid blocked clicks, hidden Save buttons and other wrong turns.',
		watch: 'Watch it work',
		connect: 'Connect an agent',
		availability: 'Public hints need no account. Keep using your own browser tools.',
		benchmarks: 'See live service benchmarks',
		videoLabel: 'The same blog task run twice, side by side. Without memory the agent hits a blocked click and leaves an empty paragraph. With WAM hints it follows three short hints and saves cleanly.',
		videoCaption: 'Same task, two runs. A scripted replay recorded in the Arise browser on a demo editor.'
	},
	demo: {
		label: 'SEE IT WORK',
		title: 'Two everyday jobs, done with a map.',
		intro: 'Each clip is a scripted replay recorded in the Arise browser on a local demo editor. The hints are examples of what an agent receives, not live catalog entries.',
		slides: {
			kicker: 'Slides',
			title: 'Make a deck without babysitting the editor',
			text: 'Ask for a short deck. The agent checks WAM first, then types straight into the placeholders, shrinks the picture that spills off the slide, and waits for autosave before it calls the job done.',
			steps: [
				{ name: 'Ask', text: `One lookup returns up to ${recallLimits.hints} hints.` },
				{ name: 'Build', text: 'Three slides, typed into placeholders.' },
				{ name: 'Fix', text: 'Picture resized from a corner to fit.' },
				{ name: 'Confirm', text: '“All changes saved” before it finishes.' }
			],
			fine: 'Slide tasks have not been benchmarked yet, so this clip makes no speed claim.',
			videoLabel: 'An AI agent builds a three-slide deck in a demo slide editor. It asks WAM for three hints, types into placeholders, resizes a picture that spills past the slide, and waits for All changes saved.'
		},
		blog: {
			kicker: 'Blog posts',
			title: 'Let AI write the post, and save a clean draft',
			text: 'The agent closes the popup first, presses Enter once between paragraphs, checks Preview and saves a draft. Nothing is published.',
			steps: [
				{ name: 'Ask', text: 'Hints for this editor, before the first click.' },
				{ name: 'Write', text: 'Title, then paragraphs, one Enter apart.' },
				{ name: 'Check', text: 'Preview shows no empty blocks.' },
				{ name: 'Save', text: 'Draft saved, not published.' }
			],
			chipsLabel: 'Medium pilot',
			chipTime: (percent) => `${percent} time`,
			chipCalls: (percent) => `${percent} tool calls`,
			chipNote: 'Medium pilot · one run',
			videoLabel: 'An AI agent writes a short blog post in a demo editor. It asks WAM for three hints, dismisses the popup first, presses Enter once between paragraphs, checks Preview and saves a draft without publishing.'
		}
	},
	productivity: {
		label: 'LIVE SERVICE BENCHMARKS',
		title: 'Google Calendar, Notion and Canva. Tested live.',
		intro: 'The R8 follow-up observed less time and fewer calls with revised Calendar hints. Two slower Notion candidates were rejected. The cards below retain the earlier R7 comparison; each service report includes its latest results and limitations.',
		scope: (date, browser, model, limit) => `R7 · ${date} · ${browser} on macOS · ${model} · ${limit} s limit per run`,
		passed: 'Both runs passed',
		metric: 'Metric',
		taskOnly: 'Task only',
		withHints: 'WAM hints',
		time: 'Time',
		calls: 'Tool calls',
		seconds: (value) => `${value.toFixed(3)} s`,
		report: 'Full report (.md)',
		workflows: {
			'google-calendar': 'Create three weekly events in a chosen time zone, move just one occurrence, then reopen all three to check.',
			notion: 'Build a private task database with dates and statuses, sort the table, and filter a board to hide completed tasks.',
			canva: 'Start from a free template, replace text and an image, then export just the second page as PNG and a one-page PDF.'
		},
		interpretation: 'In R7, all six workflows passed independent checks. Canva used less time and fewer tool calls with hints; Google Calendar and Notion used more.',
		caveats: 'R7 had one run per condition, task-only first, with a shared signed-in profile. Hints were retrieved and supplied explicitly; this does not test automatic WAM integration. Setup, independent checks and cleanup are excluded from the figures. These observations do not establish a general speed or cost benefit; service-specific limitations are in each report.'
	},
	results: {
		label: 'EARLIER EDITOR PILOT',
		titleBefore: 'Less wandering. ',
		titleEm: 'Same result.',
		intro: 'In a small pilot, agents given memory hints finished the same blog-editor task sooner and with fewer tool calls.',
		scope: (editors) => `${editors} editors · one run per condition · early observations for reference`,
		tileTime: { label: 'time on average' },
		tileCalls: { label: 'tool calls on average' },
		tileErrors: { label: 'tool errors', small: 'iframe editor' },
		tileTokens: { label: 'estimated tokens per hint', small: (n) => `Up to ${n} hints per lookup. Response metadata and detail lookups add to this.` },
		range: (from, to) => `${from} to ${to} across editors`,
		legendWithout: 'Without memory',
		legendWith: 'With memory',
		seconds: (value) => `${value} s`,
		callsLabel: 'tool calls',
		rowLabels: { medium: 'Medium', naver: 'Naver Blog', rich: 'Rich-text editor', iframe: 'Editor inside an iframe' },
		rowKinds: { live: 'Live site', controlled: 'Test editor' },
		rowNotes: {
			medium: 'Without memory the saved draft kept one empty paragraph.',
			iframe: `Tool errors fell from ${iframeErrors.before} to ${iframeErrors.after}. The bar shows hints injected automatically; looking them up by hand took 91.7 s.`
		},
		rowAria: (r) => `${r.label}: ${r.before} seconds without memory, ${r.after} seconds with memory, ${r.drop}. ${r.callsBefore} tool calls without memory, ${r.callsAfter} with.`,
		caveats: 'Pilot of 2026-10-16 with Codex gpt-6-luna at max effort. One run per condition, run order not randomized, so read these as early observations for reference. On live Naver Blog the second run met a different popup, so that row is not a controlled comparison. Problems on a site’s own side, such as a server error, are not solved by memory.'
	},
	how: {
		label: 'GET STARTED',
		title: 'Connect once. Ask before you act.',
		intro: 'Start with public hints, without an account. WAM adds shared know-how to the agent and browser tools you already use.',
		cards: [
			{ title: 'Choose your host', text: 'Connect Codex or Claude Code through MCP. For Arise, the guide covers MCP Hub and the optional local browser connection.' },
			{ title: 'Add WAM with the setup guide', text: 'Follow the command for your host, then check that its WAM tools appear. Keep your browser tools for opening pages, typing and clicking.' },
			{ title: 'Ask for hints before the task', text: 'Ask your agent to look up WAM hints for the current site and task. Connecting WAM alone does not start background lookups.' }
		],
		hostsLabel: 'Connect from',
		guideLink: 'Setup guide'
	},
	examples: {
		label: 'FROM A PAGE TO A NEXT STEP',
		title: 'A short hint. A useful next move.',
		intro: 'Here is how a draft-saving task can use a hint. This is an illustrative example, separate from live catalog results.',
		link: 'Explore published memories',
		pill: 'Illustrative example',
		contextLabel: 'YOUR AGENT ASKS WITH',
		urlLabel: 'Public URL',
		taskLabel: 'Task',
		task: 'Write and save a draft',
		keywordsLabel: 'Allowed keywords',
		hintLabel: 'WAM RETURNS A HINT',
		hintTitle: 'Save a draft without publishing',
		steps: ['Dismiss a popup if it blocks the editor.', 'Enter the title and body.', 'Check Preview and the saved state, then leave it as a draft.'],
		apply: 'Your agent checks the hint against the current page, then acts with its browser tools. Your instructions take precedence.',
		limits: (hints, tokensPerHint) => `Up to ${hints} hints per lookup, each at most ${tokensPerHint} estimated tokens.`
	},
	contribute: {
		label: 'CONTRIBUTE WITH CONSENT',
		titleLine1: 'Found a shortcut?',
		titleLine2: 'Share it, keep the credit.',
		text: 'Choose a URL, a task type and the actions to share. Contribution requests to WAM do not include page contents or free-form prompts. When another signed-in account’s agent succeeds with your memory, you can earn points and a place on the leaderboard.',
		points: [
			{ value: '+2', text: 'for eligible successful use reported by another account' },
			{ value: '+20', text: 'when a new workflow passes verification and is published' }
		],
		guide: 'Contribution guide',
		ranks: 'View contributor ranks',
		note: 'New workflows are published after safety checks and replay verification. Duplicate support earns no approval points. Successful-use awards require an explicit report and follow daily limits.',
		art: { observationLabel: 'YOUR OBSERVATION', observation: 'One discovery', hintLabel: 'SHARED HINT', hint: 'A shared shortcut' }
	},
	faq: {
		label: 'BEFORE YOU START',
		title: 'A few things to know.',
		items: [
			{ question: 'Do I need an account?', answer: 'Public hints can be read without an account. Link a WAM account to contribute, report use or receive credit. Sharing a contribution also requires your consent.' },
			{ question: 'Will my agent look up hints automatically?', answer: 'WAM runs when your agent calls a lookup tool. Connecting it alone does not start background lookups. The setup guide includes instructions you can give your agent about when to use memory.' },
			{ question: 'Does WAM include browser controls?', answer: 'Your existing browser tools handle navigation, clicks and typing. WAM supplies shared hints. The setup guide also explains the optional Arise connection that reads an owned tab when the agent requests recall.' },
			{ question: 'What if there are no hints for my task?', answer: 'An empty result means there is no matching approved hint. Your agent can continue with its usual tools and check the current page. WAM does not guarantee coverage of every site or task.' },
			{ question: 'What information is sent to WAM?', answer: 'Lookups use a normalized public URL pattern, task type and allowed keywords. WAM requests do not include page contents or free-form prompts. Your browser and model services handle their own data under their respective settings.' }
		]
	},
	cta: {
		titleBefore: 'Give your agent ',
		titleEm: 'the map.',
		text: 'Connect Codex or Claude Code with a remote MCP command. For Arise MCP Hub and direct HTTP, follow the setup guide. Public reads need no account.',
		connect: 'Connect an agent',
		explore: 'Browse shared workflows'
	}
};
