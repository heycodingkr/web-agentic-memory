import type { CatalogMemory } from './api';

// Helpers for the /memories pages: which product an entry is about, how to read its text, and how to find it.
// Pure functions so they can be tested without a browser (web/tests/memories.test.mjs).

export type Glyph = 'doc' | 'sheet' | 'slides' | 'design' | 'form' | 'mail' | 'calendar' | 'folder' | 'draft' | 'blog' | 'search' | 'route';
export type Product = { key: string; label: string; glyph: Glyph };
type Scope = Pick<CatalogMemory, 'domain' | 'canonical_url_patterns' | 'task_category'>;

const PRODUCTS: { host: RegExp; path?: string; product: Product }[] = [
	{ host: /^docs\.google\.com$/, path: '/document', product: { key: 'docs', label: 'Google Docs', glyph: 'doc' } },
	{ host: /^docs\.google\.com$/, path: '/spreadsheets', product: { key: 'sheets', label: 'Google Sheets', glyph: 'sheet' } },
	{ host: /^docs\.google\.com$/, path: '/presentation', product: { key: 'slides', label: 'Google Slides', glyph: 'slides' } },
	{ host: /^docs\.google\.com$/, path: '/forms', product: { key: 'forms', label: 'Google Forms', glyph: 'form' } },
	{ host: /^mail\.google\.com$/, product: { key: 'gmail', label: 'Gmail', glyph: 'mail' } },
	{ host: /^calendar\.google\.com$/, product: { key: 'calendar', label: 'Google Calendar', glyph: 'calendar' } },
	{ host: /^drive\.google\.com$/, product: { key: 'drive', label: 'Google Drive', glyph: 'folder' } },
	// The app lives on notion.so (older) or app.notion.com; the notion.com marketing and help pages are not the app.
	{ host: /^((www\.)?notion\.so|app\.notion\.com)$/, product: { key: 'notion', label: 'Notion', glyph: 'draft' } },
	{ host: /^(www\.)?canva\.com$/, product: { key: 'canva', label: 'Canva', glyph: 'slides' } },
	{ host: /^(www\.)?figma\.com$/, product: { key: 'figma', label: 'Figma', glyph: 'design' } },
	{ host: /^stitch\.withgoogle\.com$/, product: { key: 'stitch', label: 'Google Stitch', glyph: 'design' } },
	{ host: /^flow\.google\.com$/, product: { key: 'flow', label: 'Google Flow', glyph: 'slides' } },
	{ host: /^medium\.com$/, product: { key: 'medium', label: 'Medium', glyph: 'blog' } },
	{ host: /(^|\.)wikipedia\.org$/, product: { key: 'wikipedia', label: 'Wikipedia', glyph: 'search' } }
];

export const TASKS = [
	{ code: 'compose-draft', label: 'Create & write' },
	{ code: 'navigate', label: 'Navigate' },
	{ code: 'research', label: 'Research' }
] as const;

export function taskLabel(code: string): string {
	return TASKS.find((task) => task.code === code)?.label ?? code;
}

/** Host and path of a stored scope such as "https://docs.google.com/document/**". */
export function patternParts(pattern: string): { host: string; path: string } | null {
	const match = /^https?:\/\/([^/]+)(\/[^*?#]*)?/.exec(pattern);
	if (!match) return null;
	return { host: match[1].toLowerCase(), path: (match[2] ?? '/').replace(/\/$/, '') || '/' };
}

export function productOf(memory: Scope): Product {
	const parts = memory.canonical_url_patterns.map(patternParts).find(Boolean);
	const host = (parts?.host ?? memory.domain).toLowerCase();
	const path = parts?.path ?? '/';
	for (const entry of PRODUCTS) {
		if (entry.host.test(host) && (!entry.path || path === entry.path || path.startsWith(entry.path + '/'))) return entry.product;
	}
	const glyph: Glyph = memory.task_category === 'navigate' ? 'route' : memory.task_category === 'research' ? 'search' : 'draft';
	return { key: host, label: host.replace(/^www\./, ''), glyph };
}

const EXAMPLE_PREFIX = /^Example:\s*/;

/** Built-in examples start their title with "Example:" so nobody can mistake them for verified entries. */
export function isExample(memory: Pick<CatalogMemory, 'title'>): boolean {
	return EXAMPLE_PREFIX.test(memory.title);
}

export function displayTitle(memory: Pick<CatalogMemory, 'title'>): string {
	return memory.title.replace(EXAMPLE_PREFIX, '');
}

/** The API sends the verification time as unix seconds in a string; older fixtures send a label. */
export function formatDate(value: string): string {
	if (/^\d{9,11}$/.test(value)) return new Date(Number(value) * 1000).toISOString().slice(0, 10);
	return value;
}

export type Source = 'all' | 'examples' | 'contributed';

export const WORKFLOW_CATEGORIES = [
	{ key: 'documents', label: 'Documents & writing', ko: '문서·글쓰기', glyph: 'doc', description: 'Documents, forms, notes and articles.' },
	{ key: 'design', label: 'Design & media', ko: '디자인·미디어', glyph: 'slides', description: 'Web and mobile UI, prototypes, slides and video.' },
	{ key: 'productivity', label: 'Planning & data', ko: '업무·데이터', glyph: 'sheet', description: 'Spreadsheets, task databases, calendars and files.' },
	{ key: 'communication', label: 'Communication', ko: '커뮤니케이션', glyph: 'mail', description: 'Email and communication workflows.' },
	{ key: 'research', label: 'Research', ko: '조사·탐색', glyph: 'search', description: 'Find information and check sources.' },
	{ key: 'other', label: 'Other workflows', ko: '기타 워크플로', glyph: 'route', description: 'Workflows for other websites.' }
] as const;

export type WorkflowCategory = typeof WORKFLOW_CATEGORIES[number]['key'];

/** A browse category is separate from the API's closed task vocabulary. */
export function categoryOf(memory: CatalogMemory): WorkflowCategory {
	const product = productOf(memory).key;
	if (['figma', 'stitch', 'canva', 'slides', 'flow'].includes(product)) return 'design';
	if (product === 'gmail') return 'communication';
	if (['sheets', 'calendar', 'drive'].includes(product)) return 'productivity';
	if (product === 'notion') return /database|kanban|task tracker|데이터베이스|칸반|할 일 관리/i.test(memory.title + ' ' + memory.summary) ? 'productivity' : 'documents';
	if (['docs', 'forms', 'medium'].includes(product)) return 'documents';
	if (memory.task_category === 'research') return 'research';
	return 'other';
}

export function categoryCounts(memories: CatalogMemory[]) {
	const counts = new Map<WorkflowCategory, number>();
	for (const memory of memories) {
		const category = categoryOf(memory);
		counts.set(category, (counts.get(category) ?? 0) + 1);
	}
	return WORKFLOW_CATEGORIES.flatMap(category => {
		const count = counts.get(category.key) ?? 0;
		return count ? [{ ...category, count }] : [];
	});
}

export type Filters = { query: string; product: string; task: string; source: Source; category?: WorkflowCategory | 'all' };

/** A few spoken synonyms so "mail" finds Gmail and "sheet" finds spreadsheets. */
const ALIASES: Record<string, string[]> = {
	양식: ['template', 'request'],
	신청서: ['request', 'form'],
	구매: ['purchase', 'request'],
	회의록: ['meeting', 'notes'],
	제안서: ['proposal', 'template'],
	템플릿: ['template'],
	form: ['template', 'request'],
	forms: ['template', 'request'],
	mail: ['email', 'gmail'],
	email: ['gmail'],
	gmail: ['email'],
	sheet: ['spreadsheet', 'sheets'],
	sheets: ['spreadsheet'],
	spreadsheet: ['sheets'],
	excel: ['spreadsheet', 'sheets'],
	doc: ['document', 'docs'],
	docs: ['document'],
	document: ['docs'],
	timesheet: ['work', 'log'],
	worklog: ['work', 'log'],
	log: ['work'],
	expense: ['expenses'],
	meeting: ['notes'],
	minutes: ['notes', 'meeting'],
	slide: ['slides'],
	presentation: ['slides'],
	deck: ['slides'],
	survey: ['form', 'forms'],
	event: ['calendar'],
	schedule: ['calendar', 'event'],
	file: ['drive'],
	files: ['drive']
};

/** Singular and plural of a word, so "formulas" finds "formula" and "notes" finds "note". */
function variants(word: string): string[] {
	if (word.length > 3 && word.endsWith('s')) return [word.slice(0, -1)];
	return word.length > 2 ? [word + 's'] : [];
}

/** A pasted website address scopes the catalog locally; query strings and fragments are ignored. */
export function siteScope(query: string): { host: string; path: string } | null {
	const input = query.trim();
	if (!input || /\s/.test(input)) return null;
	try {
		const url = new URL(/^https?:\/\//i.test(input) ? input : `https://${input.replace(/^\/\//, '')}`);
		if (!['http:', 'https:'].includes(url.protocol) || !url.hostname.includes('.') || url.username || url.password) return null;
		return { host: url.hostname.toLowerCase().replace(/^www\./, ''), path: url.pathname.replace(/\/+$/, '') || '/' };
	} catch {
		return null;
	}
}

/** Use a fragment so a search, including any pasted document URL, is not sent to the web server. */
export function librarySearchHref(query: string): string {
	return query.trim() ? `/memories#q=${encodeURIComponent(query.trim())}` : '/memories';
}

function matchesSite(memory: CatalogMemory, scope: { host: string; path: string }): boolean {
	return memory.canonical_url_patterns.some(pattern => {
		const parts = patternParts(pattern);
		if (!parts || parts.host.replace(/^www\./, '') !== scope.host) return false;
		const prefix = parts.path.split(/[*:]/)[0].replace(/\/+$/, '') || '/';
		return scope.path === '/' || prefix === '/' || scope.path === prefix || scope.path.startsWith(prefix + '/') || prefix.startsWith(scope.path + '/');
	});
}

export function expandQuery(query: string): string {
	if (siteScope(query)) return '';
	const words: string[] = query.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];
	const related = words.flatMap((word) => [...variants(word), ...(ALIASES[word] ?? [])]);
	const extra = related.flatMap((word) => [word, ...(ALIASES[word] ?? [])]).filter((word) => !words.includes(word));
	return [query, ...new Set(extra)].join(' ').trim();
}

export function applyFilters(memories: CatalogMemory[], filters: Filters): CatalogMemory[] {
	const scope = siteScope(filters.query);
	return memories.filter((memory) => {
		if (scope && !matchesSite(memory, scope)) return false;
		if (filters.category && filters.category !== 'all' && categoryOf(memory) !== filters.category) return false;
		if (filters.product !== 'all' && productOf(memory).key !== filters.product) return false;
		if (filters.task !== 'all' && memory.task_category !== filters.task) return false;
		if (filters.source === 'examples' && !isExample(memory)) return false;
		if (filters.source === 'contributed' && isExample(memory)) return false;
		return true;
	});
}

// The everyday tools come first, in this order; anything else follows alphabetically.
const FEATURED = ['docs', 'figma', 'stitch', 'sheets', 'gmail', 'calendar', 'slides', 'forms', 'drive', 'notion', 'canva', 'flow', 'medium', 'wikipedia'];
const featuredRank = (key: string) => {
	const index = FEATURED.indexOf(key);
	return index === -1 ? FEATURED.length : index;
};

/** The library order when nobody has searched yet: by product, then by title. */
export function libraryOrder(memories: CatalogMemory[]): CatalogMemory[] {
	return [...memories].sort((a, b) => featuredRank(productOf(a).key) - featuredRank(productOf(b).key) || a.title.localeCompare(b.title));
}

/** Product chips in library order, with how many entries each has. */
export function productCounts(memories: CatalogMemory[]): { key: string; label: string; count: number }[] {
	const seen = new Map<string, { key: string; label: string; count: number }>();
	for (const memory of memories) {
		const product = productOf(memory);
		const row = seen.get(product.key) ?? { key: product.key, label: product.label, count: 0 };
		row.count += 1;
		seen.set(product.key, row);
	}
	return [...seen.values()].sort((a, b) => featuredRank(a.key) - featuredRank(b.key) || a.label.localeCompare(b.label));
}

export const SUGGESTIONS = ['Google Docs template', 'request form', 'project proposal', 'work log', 'meeting notes', 'spreadsheet formulas'] as const;

export const DOCUMENT_TEMPLATES = [
    { id: 'web:example-docs-request-form', label: 'Request form', description: 'Purchase, leave or expense requests with fields, a table and an approval area.', glyph: 'form', prompt: 'Create a purchase request template in Google Docs with requester fields, an item table and an approval section.' },
    { id: 'web:example-docs-meeting-notes', label: 'Meeting notes', description: 'A meeting agenda, decisions and action items with owners and dates.', glyph: 'doc', prompt: 'Create meeting notes in Google Docs with an agenda, decisions and action items with owners and due dates.' },
    { id: 'web:example-docs-template-gallery', label: 'Project proposal', description: 'Start from the template gallery and keep its headings and layout.', glyph: 'draft', prompt: 'Create a project proposal in Google Docs from a built-in template, keeping the headings and replacing the sample text.' }
] as const;

export type StepRow = { action: string; when?: string; note_code?: string };
export type Level2 = { steps: StepRow[]; preconditions: string[]; recovery: string[]; lines: string[]; notes: string };

const strings = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []);

/** Level 2 is the stored layer as JSON; a plain text detail from older data is shown as lines. */
export function parseLevel2(content: string): Level2 {
	let detail = content;
	let steps: StepRow[] = [];
	let preconditions: string[] = [];
	let recovery: string[] = [];
	try {
		const value = JSON.parse(content);
		if (value && typeof value === 'object') {
			steps = (Array.isArray(value.steps) ? value.steps : []).filter((step: unknown): step is StepRow => !!step && typeof (step as StepRow).action === 'string');
			preconditions = strings(value.preconditions);
			recovery = strings(value.recovery);
			detail = typeof value.detail === 'string' ? value.detail : '';
		}
	} catch {
		// not JSON: keep the text as the detail
	}
	const { lines, notes } = splitDetail(detail);
	return { steps, preconditions, recovery, lines, notes };
}

/** "1. Open\n2. Click\n\nNotes: ..." (or the older single-line "1. Open 2. Click") into numbered lines and notes. */
export function splitDetail(detail: string): { lines: string[]; notes: string } {
	const [body, ...rest] = detail.split(/\n\s*\n\s*Notes:\s*/);
	let lines = body.split('\n').map((line) => line.trim()).filter(Boolean);
	if (lines.length === 1 && /^\d+\.\s/.test(lines[0])) lines = lines[0].split(/\s+(?=\d+\.\s)/);
	return { lines: lines.map((line) => line.replace(/^\d+\.\s*/, '')), notes: rest.join(' ').trim() };
}

const ACTIONS: Record<string, string> = {
	navigate: 'Open',
	wait: 'Wait',
	click: 'Click',
	type: 'Type',
	key: 'Press',
	snapshot: 'Look',
	scroll: 'Scroll',
	select: 'Choose',
	upload: 'Upload',
	dialog: 'Dialog',
	other: 'Do'
};

export function actionLabel(action: string): string {
	return ACTIONS[action] ?? action;
}

/** "saved-indicator" as "Saved indicator". */
export function humanize(code: string): string {
	const text = code.replace(/[-_]+/g, ' ').trim();
	return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Splits the level 1 hint ("Title\nSummary\n- point\npitfalls: a, b") into its parts. */
export function parseHint(content: string): { title: string; summary: string; points: string[]; pitfalls: string[] } {
	const lines = content.split('\n');
	const points = lines.filter((line) => line.startsWith('- ')).map((line) => line.slice(2));
	const pitfalls = (lines.find((line) => line.startsWith('pitfalls: ')) ?? '').slice(10).split(',').map((item) => item.trim()).filter(Boolean);
	const plain = lines.filter((line) => !line.startsWith('- ') && !line.startsWith('pitfalls: '));
	return { title: plain[0] ?? '', summary: plain.slice(1).join(' '), points, pitfalls };
}

/** A shell command that asks WAM about the first URL scope of an entry. */
export function recallCommand(memory: Pick<CatalogMemory, 'canonical_url_patterns' | 'task_category'>): string {
	const pattern = memory.canonical_url_patterns[0] ?? 'https://example.com/';
	const url = pattern.replace(/\*+$/, '');
	return `wam recall ${url} --task ${memory.task_category} --json`;
}
