// Facts shown on the landing page, with sources in docs/test/benchmarks and README.md.
// These are single-run observations, not general speed or cost guarantees.

export type ProductivityService = 'google-calendar' | 'canva';

/** Latest recorded comparisons per service; R7 and R8 used different hint delivery. */
export const productivityPilot = {
	date: '2026-10-31',
	browser: 'Arise 0.1.60',
	model: 'gpt-6-luna / max',
	limitSeconds: 900,
	rows: [
		{ id: 'google-calendar', name: 'Google Calendar', url: 'https://calendar.google.com/', round: 'R8', hints: 'L1', comparisons: [
			{ variant: 'M', secondsBefore: 431.108, secondsAfter: 237.169, callsBefore: 114, callsAfter: 68 }
		] },
		{ id: 'canva', name: 'Canva', url: 'https://www.canva.com/', round: 'R7', hints: 'L1 + L2', comparisons: [
			{ variant: 'M', secondsBefore: 473.976, secondsAfter: 423.193, callsBefore: 123, callsAfter: 106 }
		] }
	]
} as const;

export type PilotId = 'medium' | 'naver' | 'rich' | 'iframe';

export type PilotRow = {
	id: PilotId;
	kind: 'live' | 'controlled';
	secondsBefore: number;
	secondsAfter: number;
	callsBefore: number;
	callsAfter: number;
};

/** Earlier 2026-10-16 pilot, Codex gpt-6-luna / max, one run per condition. */
export const pilotRows: PilotRow[] = [
	{ id: 'medium', kind: 'live', secondsBefore: 85.5, secondsAfter: 44.3, callsBefore: 10, callsAfter: 7 },
	{ id: 'naver', kind: 'live', secondsBefore: 84.1, secondsAfter: 58.8, callsBefore: 16, callsAfter: 12 },
	{ id: 'rich', kind: 'controlled', secondsBefore: 130.3, secondsAfter: 55.2, callsBefore: 16, callsAfter: 11 },
	{ id: 'iframe', kind: 'controlled', secondsBefore: 195.4, secondsAfter: 130.9, callsBefore: 28, callsAfter: 17 }
];

export const pilotSiteUrls: Partial<Record<PilotId, string>> = {
	medium: 'https://medium.com/',
	naver: 'https://blog.naver.com/'
};

/** Tool errors on the iframe editor, without and with memory. */
export const iframeErrors = { before: 3, after: 0 } as const;

/** Relative change as a fraction, e.g. -0.48 for 48% less. */
export const change = (before: number, after: number) => (after - before) / before;

/** Whole-number percent with a real minus sign, e.g. "−48%". */
export const percent = (fraction: number) => `${fraction < 0 ? '−' : '+'}${Math.abs(Math.round(fraction * 100))}%`;

const mean = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;

function summarizePilot(rows: PilotRow[]) {
	const timeChanges = rows.map((row) => change(row.secondsBefore, row.secondsAfter));
	const callChanges = rows.map((row) => change(row.callsBefore, row.callsAfter));
	return {
		rows,
		averageTimeChange: mean(timeChanges),
		averageCallChange: mean(callChanges),
		timeRange: [percent(Math.max(...timeChanges)), percent(Math.min(...timeChanges))],
		callRange: [percent(Math.max(...callChanges)), percent(Math.min(...callChanges))]
	};
}

/** Naver is shown only in Korean; each language's summary uses only its displayed rows. */
export const pilotForLanguage = (lang: 'en' | 'ko') => summarizePilot(pilotRows.filter((row) => lang === 'ko' || row.id !== 'naver'));

// Full Korean pilot summary, retained for consumers of the original four-row dataset.
export const { averageTimeChange, averageCallChange, timeRange, callRange } = summarizePilot(pilotRows);

/** Recall returns at most three hints; the L1 size limit is an estimate PER HINT. */
export const recallLimits = { hints: 3, tokensPerHint: 400 } as const;

/** Illustrative input, not a live catalog request or a verified site workflow. */
export const recallExample = { url: 'https://example.com/guide/editor', keywords: ['blog', 'write'] } as const;

export const hosts = ['Muse Code', 'Claude Code', 'Codex', 'Arise MCP Hub', 'HTTP API'] as const;
