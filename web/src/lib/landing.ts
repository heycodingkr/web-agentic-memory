// Facts shown on the landing page, in every language. The pilot numbers come from README.md ("Current evidence",
// 2026-10-16): one run per condition, run order not randomized, Codex gpt-6-luna at max effort. They are an early
// observation, so every place that shows them says so. Words live in src/lib/landing/en.ts and ko.ts.

/** Live productivity pilot: R7, documented in docs/test/benchmarks/{service}.md.
 * One T→M pair per service, shared signed-in profile; explicit retrieved hints, not automatic integration.
 * Keep these observations separate from the 2026-10-16 editor pilot and its averages.
 */
export type ProductivityService = 'google-calendar' | 'notion' | 'canva';

export const productivityPilot = {
	date: '2026-10-31',
	browser: 'Arise 0.1.60',
	model: 'gpt-6-luna / max',
	limitSeconds: 900,
	rows: [
		{ id: 'google-calendar', name: 'Google Calendar', secondsBefore: 367.677, secondsAfter: 380.690, callsBefore: 108, callsAfter: 137 },
		{ id: 'notion', name: 'Notion', secondsBefore: 293.419, secondsAfter: 327.862, callsBefore: 107, callsAfter: 117 },
		{ id: 'canva', name: 'Canva', secondsBefore: 473.976, secondsAfter: 423.193, callsBefore: 123, callsAfter: 106 }
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

export const pilotRows: PilotRow[] = [
	{ id: 'medium', kind: 'live', secondsBefore: 85.5, secondsAfter: 44.3, callsBefore: 10, callsAfter: 7 },
	{ id: 'naver', kind: 'live', secondsBefore: 84.1, secondsAfter: 58.8, callsBefore: 16, callsAfter: 12 },
	{ id: 'rich', kind: 'controlled', secondsBefore: 130.3, secondsAfter: 55.2, callsBefore: 16, callsAfter: 11 },
	{ id: 'iframe', kind: 'controlled', secondsBefore: 195.4, secondsAfter: 130.9, callsBefore: 28, callsAfter: 17 }
];

/** Tool errors on the iframe editor, without and with memory. */
export const iframeErrors = { before: 3, after: 0 } as const;

/** Relative change as a fraction, e.g. -0.48 for 48% less. */
export const change = (before: number, after: number) => (after - before) / before;

/** Whole-number percent with a real minus sign, e.g. "−48%". */
export const percent = (fraction: number) => `${fraction < 0 ? '−' : '+'}${Math.abs(Math.round(fraction * 100))}%`;

const mean = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;

const timeChanges = pilotRows.map((row) => change(row.secondsBefore, row.secondsAfter));
const callChanges = pilotRows.map((row) => change(row.callsBefore, row.callsAfter));

export const averageTimeChange = mean(timeChanges);
export const averageCallChange = mean(callChanges);

/** Smallest and largest drop across the four conditions, e.g. ["−30%", "−58%"]. */
export const timeRange = [percent(Math.max(...timeChanges)), percent(Math.min(...timeChanges))] as const;
export const callRange = [percent(Math.max(...callChanges)), percent(Math.min(...callChanges))] as const;

/** Recall returns at most three hints; the L1 size limit is an estimate PER HINT. */
export const recallLimits = { hints: 3, tokensPerHint: 400 } as const;

/** Illustrative input, not a live catalog request or a verified site workflow. */
export const recallExample = { url: 'https://example.com/guide/editor', keywords: ['blog', 'write'] } as const;

export const hosts = ['Codex', 'Claude Code', 'Arise MCP Hub', 'HTTP API'] as const;
