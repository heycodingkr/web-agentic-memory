import type { PilotId } from '../landing';

// Every word on the landing page, so the page can be shown in more than one language without a second copy of
// the markup. Numbers do not live here: they come from ../landing and are formatted by the functions below.

export type Step = { name: string; text: string };

export type LandingCopy = {
	lang: 'en' | 'ko';
	/** Route of this language's page, e.g. "/" or "/ko". */
	path: string;
	/** File under static/ used as og:image. */
	ogImage: string;
	/** Clip names under static/media (without extension; the poster is <name>-poster.jpg). */
	videos: { compare: string; slides: string; blog: string };
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
		videoLabel: string;
		videoCaption: string;
	};
	demo: {
		label: string;
		title: string;
		intro: string;
		slides: { kicker: string; title: string; text: string; steps: Step[]; fine: string; videoLabel: string };
		blog: {
			kicker: string;
			title: string;
			text: string;
			steps: Step[];
			chipsLabel: string;
			chipTime: (percent: string) => string;
			chipCalls: (percent: string) => string;
			chipNote: string;
			videoLabel: string;
		};
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
