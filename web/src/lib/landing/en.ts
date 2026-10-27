import { memories } from '../demo';
import { iframeErrors, recallLimits } from '../landing';
import type { LandingCopy } from './copy';

const { hints, tokens } = recallLimits;

export const en: LandingCopy = {
	lang: 'en',
	path: '/',
	ogImage: '/og-image.png',
	videos: { compare: 'demo-compare', slides: 'demo-slides', blog: 'demo-blog' },
	meta: {
		title: 'Web Agentic Memory — Browser AI that already knows the way',
		description: 'Short, verified how-tos that help AI browser agents finish web tasks like slides and blog posts in fewer steps.'
	},
	switcher: { label: 'Language', en: 'English', ko: '한국어' },
	hero: {
		eyebrow: 'Shared memory for browser AI agents',
		titleBefore: 'Browser AI that already ',
		titleEm: 'knows the way.',
		titleAfter: '',
		lede: 'AI agents lose minutes on every new site: a popup blocks the click, the Save button hides, the editor rejects input. WAM hands them short, verified how-tos for that site and task, so slides, blog posts and forms get done in fewer steps.',
		watch: 'Watch it work',
		connect: 'Connect an agent',
		proofLabel: 'Pilot results, averaged over four editors',
		proofTime: 'less time',
		proofCalls: 'fewer tool calls',
		proofErrors: 'tool errors, iframe editor',
		fine: 'Pilot on four editors, one run each: an early observation, not a guarantee.',
		fineLink: 'See the numbers',
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
				{ name: 'Ask', text: `One lookup returns up to ${hints} hints.` },
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
	results: {
		label: 'WHAT YOU GET',
		titleBefore: 'Less wandering. ',
		titleEm: 'Same result.',
		intro: 'In a small pilot, agents that were handed a memory finished the same blog-editor task sooner and with fewer tool calls.',
		tileTime: { label: 'time on average' },
		tileCalls: { label: 'tool calls on average' },
		tileErrors: { label: 'tool errors', small: 'iframe editor' },
		tileTokens: { label: 'tokens per lookup', small: (n) => `up to ${n} hints. That is the whole bill.` },
		range: (from, to) => `from ${from} to ${to}`,
		legendWithout: 'Without memory',
		legendWith: 'With memory',
		seconds: (value) => `${value} s`,
		callsLabel: 'tool calls',
		rowLabels: { medium: 'Medium', naver: 'Naver Blog', rich: 'Rich-text editor', iframe: 'Editor inside an iframe' },
		rowKinds: { live: 'Live site', controlled: 'Controlled editor' },
		rowNotes: {
			medium: 'Without memory the saved draft kept one empty paragraph.',
			iframe: `Tool errors fell from ${iframeErrors.before} to ${iframeErrors.after}. Automatic injection is shown; manual recall took 91.7 s.`
		},
		rowAria: (r) => `${r.label}: ${r.before} seconds without memory, ${r.after} seconds with memory, ${r.drop}. ${r.callsBefore} tool calls without memory, ${r.callsAfter} with.`,
		caveats: 'Pilot of 2026-10-16 with Codex gpt-6-luna at max effort. One run per condition, run order not randomized, so read these as early observations. Live Naver Blog met a different popup on its second run. Problems on a site’s own side, such as a server error, are not solved by memory.',
		scaleLead: 'At scale, simple arithmetic and not a measurement:',
		scaleBody: (hours) => `if the Medium saving held, 1,000 posts would save about ${hours} hours. Learn it once, everyone uses it.`
	},
	how: {
		label: 'HOW IT WORKS',
		title: 'One lookup before the first click.',
		intro: 'Your agent keeps its own browser tools. WAM only adds what other agents already learned about the site.',
		cards: [
			{ title: 'Your agent opens a site', text: 'It asks WAM with the page’s URL and a few allowlisted keywords. Page contents and free-form prompts stay on your device.' },
			{ title: `WAM answers with up to ${hints} hints`, text: `Short, verified how-tos for that site and task, at most ${tokens} tokens in total. Your agent follows them with the tools it already has.` },
			{ title: 'The next agent starts smarter', text: 'Sharing is opt-in. A Gatekeeper screens what an agent learned before anything is offered to the next one, and the first contributor keeps the credit.' }
		],
		hostsLabel: 'Connect from',
		guideLink: 'Setup guide'
	},
	examples: {
		label: 'DEMO EXAMPLES',
		title: 'Small workflows are worth keeping',
		intro: 'These examples are separate from the approved catalog. Explore the memory page to see currently published items.',
		link: 'Explore published memories',
		pill: 'Example',
		cards: memories.slice(0, 3).map((memory) => ({ title: memory.title, summary: memory.summary, site: memory.site, category: memory.category }))
	},
	contribute: {
		label: 'CONTRIBUTE WITH CONSENT',
		titleLine1: 'Found a shortcut?',
		titleLine2: 'Share it, keep the credit.',
		text: 'Choose a URL, a task type and the actions to share. Page contents and free-form prompts are never sent. When another signed-in account’s agent succeeds with your memory, you earn points and a place on the leaderboard.',
		points: [
			{ value: '+2', text: 'each time another account’s agent uses your memory successfully' },
			{ value: '+20', text: 'when a new memory is approved (planned)' }
		],
		guide: 'Contribution guide',
		ranks: 'View contributor ranks',
		note: 'The contribution flow currently shows development submissions and review states. Public publishing and points after approval are not connected yet.',
		art: { observationLabel: 'YOUR OBSERVATION', observation: 'One discovery', hintLabel: 'SHARED HINT', hint: 'A shared shortcut' }
	},
	cta: {
		titleBefore: 'Give your agent ',
		titleEm: 'the map.',
		text: 'Copy one snippet into Codex, Claude Code or Arise MCP Hub, or call the HTTP API directly. Reading hints needs no account.',
		connect: 'Connect an agent',
		explore: 'Explore memories'
	}
};
