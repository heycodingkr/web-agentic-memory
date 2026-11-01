import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

export const en: LandingCopy = {
	lang: 'en',
	path: '/',
	ogImage: '/og-image.png',
	videos: { compare: 'demo-compare', slides: 'demo-slides', blog: 'demo-blog', docs: 'demo-docs', sheets: 'demo-sheets', email: 'demo-email', forms: 'demo-forms', docsCompare: 'demo-docs-compare' },
	meta: {
		title: 'Web Agentic Memory — Browser AI that already knows the way',
		description: 'Short, verified hints that help your AI agent finish web tasks with fewer wrong turns. Works with Muse Code, Claude Code and Codex through MCP.'
	},
	switcher: { label: 'Language', en: 'English', ko: 'Korean' },
	hero: {
		eyebrow: 'Shared memory for browser AI agents',
		titleBefore: 'Browser AI that already ',
		titleEm: 'knows the way.',
		titleAfter: '',
		lede: 'Web\u00a0Agentic\u00a0Memory (WAM) gives your AI agent short, verified hints for the site and task at hand, so it avoids blocked clicks, hidden Save buttons and other wrong turns. It works with Muse Code, Claude Code and Codex through MCP.',
		watch: 'Watch it work',
		connect: 'Connect an agent',
		github: 'View on GitHub',
		availability: 'Public hints need no account. Keep using your own browser tools.',
		benchmarks: 'See live service benchmarks',
		videoLabel: 'The same blog task run twice, side by side. Without memory the agent hits a blocked click and leaves an empty paragraph. With WAM hints it follows three short hints and saves cleanly.',
		videoCaption: 'Same task, run twice. A scripted replay on a demo editor.'
	},
	workflows: {
		label: 'WORKFLOWS',
		title: 'What can you do with Web Agentic Memory?',
		intro: 'Ask your agent for a deck, a doc, a post, a sheet, an email or a form. It gets a few short hints for that editor first, then works with fewer detours. Clips are scripted replays recorded in the Arise browser on demo editors, and the hints are examples.',
		askLabel: 'You ask',
		hintsLabel: 'WAM hints',
		getLabel: 'You get',
		compareLabel: 'Compare with and without hints',
		flowLabel: 'Back to the hints clip',
		libraryText: 'Need another site or task? Browse built-in examples such as document templates, work logs, email and calendar.',
		libraryLink: 'Open the memory library',
		chips: { label: 'Medium pilot', time: (percent) => `${percent} time`, calls: (percent) => `${percent} tool calls`, note: 'Medium pilot · one run' },
		items: [
			{
				id: 'slides',
				kicker: 'Slide decks',
				title: 'Ask for a deck. Get clean slides.',
				ask: 'Make a 3-slide deck about shared trails for web agents. Add one picture.',
				hints: ['Type straight into the placeholders', 'Resize a picture that spills out', 'Wait for “All changes saved”'],
				result: 'Three slides, the picture inside the slide, autosave confirmed.',
				note: 'Not benchmarked yet, so no speed claim.',
				videoLabel: 'An AI agent builds a three-slide deck in a demo slide editor. It asks WAM for three hints, types into placeholders, resizes a picture that spills past the slide, and waits for All changes saved.'
			},
			{
				id: 'docs',
				kicker: 'Documents',
				title: 'Ask for a brief. Get a structured doc.',
				ask: 'Draft a one-page brief with a plan and a small timeline table.',
				hints: ['Name the doc before you write', 'Headings from the Styles menu, lists from the bullet button', 'Tab between table cells'],
				result: 'A named document with headings, a bullet list and a table, saved and still private.',
				note: 'Not benchmarked yet, so no speed claim.',
				videoLabel: 'An AI agent drafts a one-page brief in a demo document editor. It asks WAM for three hints, names the document, adds headings from the Styles menu, makes a bullet list, fills a table with Tab and waits for Saved.',
				compareVideo: 'docsCompare',
				compareVideoLabel: 'The same brief run twice, side by side, in a demo document editor. Without memory the agent types the title into the page, expects # and - to format text, and goes back to fix all three. With WAM hints it names the document first, uses the Styles menu and the bullet button, and fills the table with Tab.'
			},
			{
				id: 'blog',
				kicker: 'Blog posts',
				title: 'Ask for a post. Get a clean draft.',
				ask: 'Write a short post about shared trails and save it as a draft.',
				hints: ['Close the popup first', 'One Enter between paragraphs', 'Check Preview, then save the draft'],
				result: 'A saved draft with no empty paragraphs. Nothing is published.',
				videoLabel: 'An AI agent writes a short blog post in a demo editor. It asks WAM for three hints, dismisses the popup first, presses Enter once between paragraphs, checks Preview and saves a draft without publishing.',
				compareVideo: 'compare',
				compareVideoLabel: 'The same blog task run twice, side by side. Without memory the agent hits a blocked click and leaves an empty paragraph. With WAM hints it follows three short hints and saves cleanly.'
			},
			{
				id: 'sheets',
				kicker: 'Spreadsheets',
				title: 'Ask for a sheet. Get working totals.',
				ask: 'Make a small monthly budget: three items for Jan and Feb, with totals.',
				hints: ['Tab goes right, Enter drops to the next row', 'Start formulas with =', 'Wait for “Saved”, then check the totals'],
				result: 'Every row filled, formulas computed instead of left as text, and totals that add up.',
				note: 'Not benchmarked yet, so no speed claim.',
				videoLabel: 'An AI agent fills in a small budget in a demo spreadsheet. It asks WAM for three hints, types across rows with Tab and Enter, starts each formula with an equals sign and checks the totals once Saved appears.'
			},
			{
				id: 'email',
				kicker: 'Email',
				title: 'Ask for an email. Get a saved draft.',
				ask: 'Draft an email to Ana and Ben about Friday’s trail demo. Don’t send it.',
				hints: ['Press Enter after each address', 'Click the first line, above the signature', 'Add a subject, then Save draft'],
				result: 'Two recipients, a subject and a body above the signature, saved as a draft. Nothing is sent.',
				note: 'Not benchmarked yet, so no speed claim.',
				videoLabel: 'An AI agent drafts an email in a demo mail composer. It asks WAM for three hints, presses Enter after each address so it becomes a chip, writes above the signature, adds a subject and saves a draft without sending.'
			},
			{
				id: 'forms',
				kicker: 'Forms',
				title: 'Ask for a form. Get it filled in.',
				ask: 'Fill in the time-off request: vacation from Nov 12. Save it as a draft.',
				hints: ['Pick the leave type from the list', 'Set the date through the calendar', 'Tick the required box, then save'],
				result: 'Every required field complete, the date set through the calendar, saved as a draft. Nothing is submitted.',
				note: 'Not benchmarked yet, so no speed claim.',
				videoLabel: 'An AI agent fills in a time-off request in a demo web form. It asks WAM for three hints, picks the leave type from the list, sets the date through the calendar, ticks the required box and saves a draft without submitting.'
			}
		]
	},
	productivity: {
		label: 'LIVE SERVICE BENCHMARKS',
		title: 'Google Calendar and Canva. Tested live.',
		intro: 'In these one-off comparisons, hints reduced Calendar time by 45% and Canva time by 11%, with fewer outer tool calls in both. Notion is omitted while its memory is remeasured. These observations are not a general speed guarantee.',
		scope: (date, browser, model, limit) => `${date} · ${browser} on macOS · ${model} · ${limit} s limit per run · one run per condition`,
		passed: 'Both runs passed',
		metric: 'Metric',
		taskOnly: 'Task only',
		withHints: 'WAM hints',
		time: 'Time',
		calls: 'Tool calls (outer)',
		seconds: (value) => `${value.toFixed(3)} s`,
		report: 'Full report (.md)',
		workflows: {
			'google-calendar': 'Create three weekly events in a chosen time zone, move just one occurrence, then reopen all three to check.',
			canva: 'Start from a free template, replace text and an image, then export just the second page as PNG and a one-page PDF.'
		},
		caveats: 'Runs used a fixed order and one shared signed-in browser profile. Calendar (R8 · L1) got short hints; Canva (R7 · L1 + L2) also got longer details, so the rounds cannot be compared directly. “Outer” tool calls count top-level tool steps. Setup, checks and cleanup are excluded. These observations do not establish a general speed or cost benefit, or automatic integration. Google Docs, Google Flow, Google Slides, Gmail, and Figma examples are not timed benchmarks.',
		unmeasured: 'has a scripted demo only; Google Docs, Google Flow, Google Slides, Gmail, and Figma have workflow examples but no timed comparisons yet.',
		delta: 'Change',
		candidate: (variant) => variant === 'M' ? 'WAM hints' : `Candidate ${variant.slice(1)}`
	},
	results: {
		label: 'EARLIER EDITOR PILOT',
		titleBefore: 'Less wandering. ',
		titleEm: 'Same result.',
		intro: 'In a small pilot, agents with hints finished the same blog-editor task sooner and with fewer tool calls.',
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
		caveats: 'Pilot of 2026-10-16 with Codex gpt-6-luna at max effort. Run order was not randomized. Problems on a site’s own side, such as a server error, are not solved by memory.'
	},
	how: {
		label: 'GET STARTED',
		title: 'Connect once. Ask before you act.',
		intro: 'WAM adds shared hints to the agent and browser tools you already use.',
		cards: [
			{ title: 'Pick your agent', text: 'Muse Code, Claude Code or Codex, through MCP. For Arise, see the setup guide.' },
			{ title: 'Add WAM', text: 'Paste one command, then check that the WAM tools show up.' },
			{ title: 'Ask for hints first', text: 'Ask your agent to look up hints for the current site and task. Connecting alone starts no background lookups.' }
		],
		hostsLabel: 'Connect from',
		guideLink: 'Setup guide',
		quick: {
			label: 'QUICK INSTALL',
			title: 'Copy the setup for your agent.',
			intro: 'Remote MCP: no download and no account. Full steps, checks and account linking are in the setup guide.',
			tabsLabel: 'Choose your agent',
			museInstall: 'Install Muse Code · terminal',
			museConfig: 'Add to ~/.config/muse/settings.json',
			museNote: 'Start muse, run /mcp to see web-memory, then ask for hints. Muse Code is Meta’s coding agent and runs Muse Spark. To use Muse Spark inside Claude Code instead, the setup guide has the Meta Model API settings.',
			claudeLabel: 'Claude Code · add remote MCP',
			claudeNote: 'Run it, open a new session, then ask for hints.',
			codexLabel: 'Codex · add remote MCP',
			codexNote: 'Same server and tools. Codex runs its own models.',
			more: 'Windows, account sign-in, local tools and checks'
		}
	},
	examples: {
		label: 'WHAT A HINT LOOKS LIKE',
		title: 'A short hint. A useful next move.',
		intro: 'How one hint guides a draft-saving task. This example is illustrative, not a live catalog result.',
		link: 'Browse the memory library',
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
		note: 'New workflows are published after safety checks and replay verification. Duplicate contributions earn no approval points. Successful-use points need an explicit report and follow daily limits.',
		art: { observationLabel: 'YOUR OBSERVATION', observation: 'One discovery', hintLabel: 'SHARED HINT', hint: 'A shared shortcut' }
	},
	faq: {
		label: 'BEFORE YOU START',
		title: 'A few things to know.',
		items: [
			{ question: 'Do I need an account?', answer: 'Not to read public hints. Link a WAM account to contribute, report use or get credit. Sharing a contribution also needs your consent.' },
			{ question: 'Will my agent look up hints automatically?', answer: 'Only when your agent calls a lookup tool. Connecting WAM alone starts no background lookups. The setup guide has instructions you can give your agent about when to use memory.' },
			{ question: 'Does WAM include browser controls?', answer: 'No. Your own browser tools open pages, click and type; WAM only supplies hints. The setup guide also covers an optional Arise connection that reads a tab the agent owns when it asks for hints.' },
			{ question: 'What if there are no hints for my task?', answer: 'An empty result means no approved hint matches yet. Your agent can carry on with its usual tools and check the current page. WAM does not cover every site or task.' },
			{ question: 'What information is sent to WAM?', answer: 'Lookups use a normalized public URL pattern, task type and allowed keywords. WAM requests do not include page contents or free-form prompts. Your browser and model services handle their own data under their own settings.' }
		]
	},
	cta: {
		titleBefore: 'Give your agent ',
		titleEm: 'the map.',
		text: 'Connect Muse Code, Claude Code or Codex with one setting or command. Reading public hints needs no account.',
		connect: 'Connect an agent',
		explore: 'Browse the memory library'
	}
};
