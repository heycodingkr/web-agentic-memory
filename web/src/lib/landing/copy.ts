import type { PilotId, ProductivityService } from '../landing';

// Every word on the landing page, so the page can be shown in more than one language without a second copy of
// the markup. Numbers do not live here: they come from ../landing and are formatted by the functions below.

/** One card of the workflows section: what you ask, the hints the agent gets, what you end up with. */
export type WorkflowCard = {
	id: 'slides' | 'docs' | 'blog' | 'sheets' | 'email' | 'forms';
	kicker: string;
	title: string;
	ask: string;
	hints: string[];
	result: string;
	/** Fine print under the card, e.g. that the clip makes no speed claim. */
	note?: string;
	videoLabel: string;
	/** Key of `videos` for a second clip of the same task, run without and with hints side by side. */
	compareVideo?: 'compare' | 'docsCompare';
	compareVideoLabel?: string;
};

export type LandingCopy = {
	lang: 'en' | 'ko';
	/** Route of this language's page, e.g. "/" or "/ko". */
	path: string;
	/** File under static/ used as og:image. */
	ogImage: string;
	/** Clip names under static/media (without extension; the poster is <name>-poster.jpg). */
	videos: { compare: string; slides: string; blog: string; docs: string; sheets: string; email: string; forms: string; docsCompare: string };
	meta: { title: string; description: string };
	switcher: { label: string; en: string; ko: string };
	hero: {
		eyebrow: string;
		titleBefore: string;
		titleEm: string;
		titleAfter: string;
		lede: string;
		watch: string;
		connect: string;
		availability: string;
		github: string;
		benchmarks: string;
		videoLabel: string;
		videoCaption: string;
	};
	workflows: {
		label: string;
		title: string;
		intro: string;
		askLabel: string;
		hintsLabel: string;
		getLabel: string;
		/** Buttons that swap a card between its hints clip and its side-by-side clip. */
		compareLabel: string;
		flowLabel: string;
		/** Line under the cards that points to the searchable memory library. */
		libraryText: string;
		libraryLink: string;
		/** Pilot numbers under the blog card, the one task here with a measured result. */
		chips: { label: string; time: (percent: string) => string; calls: (percent: string) => string; note: string };
		/** Cards in display order. */
		items: WorkflowCard[];
	};
	productivity: {
		label: string;
		title: string;
		intro: string;
		scope: (date: string, browser: string, model: string, limit: number) => string;
		passed: string;
		metric: string;
		taskOnly: string;
		withHints: string;
		time: string;
		calls: string;
		seconds: (value: number) => string;
		report: string;
		workflows: Record<ProductivityService, string>;
		caveats: string;
		unmeasured: string;
		delta: string;
		candidate: (variant: string) => string;
	};
	results: {
		label: string;
		titleBefore: string;
		titleEm: string;
		intro: string;
		scope: (editors: number) => string;
		tileTime: { label: string };
		tileCalls: { label: string };
		tileErrors: { label: string; small: string };
		tileTokens: { label: string; small: (hints: number) => string };
		range: (from: string, to: string) => string;
		legendWithout: string;
		legendWith: string;
		seconds: (value: number) => string;
		callsLabel: string;
		rowLabels: Record<PilotId, string>;
		rowKinds: { live: string; controlled: string };
		rowNotes: Partial<Record<PilotId, string>>;
		rowAria: (row: { label: string; before: number; after: number; drop: string; callsBefore: number; callsAfter: number }) => string;
		caveats: string;
	};
	how: {
		label: string;
		title: string;
		intro: string;
		cards: { title: string; text: string }[];
		hostsLabel: string;
		guideLink: string;
		/** The install commands pulled out of the setup guide (Muse Code, Claude Code, Codex). */
		quick: {
			label: string;
			title: string;
			intro: string;
			tabsLabel: string;
			museInstall: string;
			museConfig: string;
			museNote: string;
			claudeLabel: string;
			claudeNote: string;
			codexLabel: string;
			codexNote: string;
			more: string;
		};
	};
	examples: {
		label: string;
		title: string;
		intro: string;
		link: string;
		pill: string;
		contextLabel: string;
		urlLabel: string;
		taskLabel: string;
		task: string;
		keywordsLabel: string;
		hintLabel: string;
		hintTitle: string;
		steps: string[];
		apply: string;
		limits: (hints: number, tokensPerHint: number) => string;
	};
	contribute: {
		label: string;
		titleLine1: string;
		titleLine2: string;
		text: string;
		points: { value: string; text: string }[];
		guide: string;
		ranks: string;
		note: string;
		art: { observationLabel: string; observation: string; hintLabel: string; hint: string };
	};
	faq: { label: string; title: string; items: { question: string; answer: string }[] };
	cta: { titleBefore: string; titleEm: string; text: string; connect: string; explore: string };
};
