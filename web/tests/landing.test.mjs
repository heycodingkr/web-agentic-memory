import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';
import ts from 'typescript';

// Load the landing data and both language files straight from the TypeScript sources: transpile each file,
// flatten them into one temp folder and point their relative imports at the flattened copies.
const root = new URL('../src/lib/', import.meta.url);
const sources = { landing: 'landing.ts', en: 'landing/en.ts', ko: 'landing/ko.ts' };
const dir = await mkdtemp(join(tmpdir(), 'wam-landing-test-'));
for (const [name, path] of Object.entries(sources)) {
	const source = await readFile(new URL(path, root), 'utf8');
	const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
	await writeFile(join(dir, `${name}.mjs`), js.replace(/from '(?:\.{1,2}\/)+(?:landing\/)?([\w-]+)'/g, "from './$1.mjs'"));
}
const load = (name) => import(pathToFileURL(join(dir, `${name}.mjs`)).href);
const data = await load('landing');
const { en } = await load('en');
const { ko } = await load('ko');

test('pilot numbers on the page match the README table', () => {
	const { pilotRows, change, percent, averageTimeChange, averageCallChange, timeRange, callRange, recallLimits, iframeErrors } = data;
	assert.deepEqual(pilotRows.map((row) => percent(change(row.secondsBefore, row.secondsAfter))), ['−48%', '−30%', '−58%', '−33%']);
	assert.deepEqual(pilotRows.map((row) => percent(change(row.callsBefore, row.callsAfter))), ['−30%', '−25%', '−31%', '−39%']);
	assert.equal(percent(averageTimeChange), '−42%');
	assert.equal(percent(averageCallChange), '−31%');
	assert.deepEqual([...timeRange], ['−30%', '−58%']);
	assert.deepEqual([...callRange], ['−25%', '−39%']);
	assert.deepEqual({ ...recallLimits }, { hints: 3, tokensPerHint: 400 });
	assert.deepEqual({ ...iframeErrors }, { before: 3, after: 0 });
});

test('each language averages only the pilot rows it displays', async () => {
	const english = data.pilotForLanguage('en');
	const korean = data.pilotForLanguage('ko');
	assert.deepEqual(english.rows.map(row => row.id), ['medium', 'rich', 'iframe']);
	assert.equal(korean.rows.length, 4);
	assert.equal(data.percent(english.averageTimeChange), '−46%');
	assert.equal(data.percent(english.averageCallChange), '−34%');
	const readme = await readFile(new URL('../../README.md', import.meta.url), 'utf8');
	assert.doesNotMatch(readme, /Naver|네이버/i);
	assert.match(readme, /about −46%/);
	assert.match(readme, /about −34%/);
});

test('displayed live measurements occur in their source reports', async () => {
	for (const row of data.productivityPilot.rows) {
		const report = await readFile(new URL(`../../docs/test/benchmarks/${row.id}.md`, import.meta.url), 'utf8');
		assert.match(row.url, /^https:\/\//);
		for (const comparison of row.comparisons) {
			for (const key of ['secondsBefore', 'secondsAfter']) assert.ok(report.includes(comparison[key].toFixed(3)), `${row.id} ${comparison.variant} ${key}`);
			for (const key of ['callsBefore', 'callsAfter']) assert.ok(report.includes(`| ${comparison[key]} |`), `${row.id} ${comparison.variant} ${key}`);
		}
	}
});

function leaves(value, path = '') {
	if (typeof value === 'string') return [[path, value]];
	if (typeof value === 'function') return [[path, value]];
	if (Array.isArray(value)) return value.flatMap((item, index) => leaves(item, `${path}${path ? '.' : ''}${index}`));
	if (value && typeof value === 'object') return Object.entries(value).flatMap(([key, item]) => leaves(item, `${path}${path ? '.' : ''}${key}`));
	return [[path, value]];
}

test('English and Korean copy have the same shape', () => {
	const shape = (copy) => leaves(copy).map(([path, value]) => `${path}:${typeof value === 'function' ? `fn${value.length}` : typeof value}`);
	assert.deepEqual(shape(ko), shape(en));
});

test('every string is filled in, with no leftover placeholders', () => {
	for (const copy of [en, ko]) {
		for (const [path, value] of leaves(copy)) {
			if (typeof value !== 'string') continue;
			// The English headline has nothing after its emphasised words; every other string must say something.
			if (path !== 'hero.titleAfter') assert.ok(value.trim().length > 0, `${copy.lang} ${path} is empty`);
			assert.ok(!/\$\{|undefined|NaN|\bnull\b/.test(value), `${copy.lang} ${path} has a placeholder: ${value}`);
		}
	}
});

test('functions in the copy use the numbers they are given', () => {
	for (const copy of [en, ko]) {
		assert.ok(copy.workflows.chips.time('−48%').includes('−48%'));
		assert.ok(copy.workflows.chips.calls('−30%').includes('−30%'));
		assert.ok(copy.results.range('−30%', '−58%').includes('−30%') && copy.results.range('−30%', '−58%').includes('−58%'));
		assert.ok(copy.results.seconds(85.5).includes('85.5'));
		assert.ok(copy.results.tileTokens.small(3).includes('3'));
		assert.ok(copy.results.scope(4).includes('4'));
		assert.ok(copy.examples.limits(3, 400).includes('3') && copy.examples.limits(3, 400).includes('400'));
		const aria = copy.results.rowAria({ label: 'Medium', before: 85.5, after: 44.3, drop: '−48%', callsBefore: 10, callsAfter: 7 });
		for (const part of ['85.5', '44.3', '−48%', '10', '7']) assert.ok(aria.includes(part), `${copy.lang} rowAria misses ${part}`);
	}
});

test('every pilot row has a label and a kind in both languages', () => {
	for (const copy of [en, ko]) {
		for (const row of data.pilotRows) {
			assert.ok(copy.results.rowLabels[row.id], `${copy.lang} label for ${row.id}`);
			assert.ok(copy.results.rowKinds[row.kind], `${copy.lang} kind for ${row.kind}`);
		}
		for (const id of Object.keys(copy.results.rowNotes)) assert.ok(data.pilotRows.some((row) => row.id === id), `note for unknown row ${id}`);
	}
});

test('the hero spells out Web Agentic Memory before it uses the abbreviation WAM', () => {
	for (const copy of [en, ko]) {
		// Everything a visitor reads above the lede (the head tags are not part of the page).
		const above = [];
		for (const [path, value] of leaves(copy)) {
			if (path === 'hero.lede') break;
			if (typeof value === 'string' && !path.startsWith('meta.')) above.push(value);
		}
		assert.ok(!above.some((value) => value.includes('WAM')), `${copy.lang} uses WAM above the hero lede`);
		// The name may use non-breaking spaces so it does not split across lines.
		const lede = copy.hero.lede.replace(/\u00a0/g, ' ');
		assert.ok(lede.includes('Web Agentic Memory'), `${copy.lang} hero.lede never spells out the product name`);
		assert.ok(lede.indexOf('Web Agentic Memory') < lede.indexOf('WAM'), `${copy.lang} hero.lede uses WAM before spelling it out`);
	}
});

test('the Korean page is actually translated', () => {
	// Words that are names, symbols or already shared are allowed to match the English copy.
	const same = new Set(['switcher.en', 'switcher.ko', 'results.rowLabels.medium', 'contribute.points.0.value', 'contribute.points.1.value']);
	const english = new Map(leaves(en));
	for (const [path, value] of leaves(ko)) {
		if (typeof value !== 'string' || ['lang', 'path', 'ogImage'].includes(path) || path.startsWith('videos.') || /^workflows\.items\.\d+\.(id|compareVideo)$/.test(path)) continue;
		if (value === english.get(path)) assert.ok(same.has(path), `ko ${path} is still English: ${value}`);
	}
	assert.match(ko.hero.lede, /[가-힣]/);
	assert.match(ko.cta.text, /[가-힣]/);
	assert.equal(ko.path, '/ko');
	for (const name of Object.values(ko.videos)) assert.ok(name.endsWith('-ko'), name);
});

test('the workflow cards line up in both languages and every clip they name exists', () => {
	const { recallLimits } = data;
	const media = new URL('../static/media/', import.meta.url);
	for (const copy of [en, ko]) {
		assert.deepEqual(copy.workflows.items.map((item) => item.id), ['slides', 'docs', 'blog', 'sheets', 'email', 'forms']);
		for (const item of copy.workflows.items) {
			assert.equal(item.hints.length, recallLimits.hints, `${copy.lang} ${item.id} shows one line per hint WAM can return`);
			assert.ok(item.ask && item.result && item.videoLabel, `${copy.lang} ${item.id} is incomplete`);
			assert.ok(copy.videos[item.id], `${copy.lang} ${item.id} has no clip`);
		}
		// Only the docs and blog cards have a side-by-side clip, and it must be a real clip with its own description.
		assert.deepEqual(copy.workflows.items.filter((item) => item.compareVideo).map((item) => [item.id, item.compareVideo]), [['docs', 'docsCompare'], ['blog', 'compare']]);
		for (const item of copy.workflows.items.filter((entry) => entry.compareVideo)) {
			assert.ok(copy.videos[item.compareVideo] && item.compareVideoLabel, `${copy.lang} ${item.id} side-by-side clip is incomplete`);
		}
		assert.ok(copy.workflows.compareLabel && copy.workflows.flowLabel, `${copy.lang} compare buttons have no labels`);
		for (const [key, name] of Object.entries(copy.videos)) {
			assert.ok(existsSync(new URL(`${name}.mp4`, media)), `${copy.lang} videos.${key}: ${name}.mp4 is missing from static/media`);
			assert.ok(existsSync(new URL(`${name}-poster.jpg`, media)), `${copy.lang} videos.${key}: ${name}-poster.jpg is missing from static/media`);
		}
	}
});

test('only the blog card quotes numbers; slides and documents say they have no speed claim', () => {
	for (const copy of [en, ko]) {
		for (const item of copy.workflows.items) {
			if (item.id === 'blog') {
				assert.equal(item.note, undefined, `${copy.lang} blog numbers come from the pilot chips, not from free text`);
				continue;
			}
			assert.match(item.note ?? '', /benchmark|벤치마크/, `${copy.lang} ${item.id} needs its no-benchmark note`);
			assert.doesNotMatch(`${item.title} ${item.result} ${item.hints.join(' ')}`, /%|\bfaster\b|\bfewer clicks\b|빠르/, `${copy.lang} ${item.id} must not claim speed`);
		}
	}
});

test('the token claim describes an estimate per hint instead of a whole lookup budget', async () => {
	assert.match(en.results.tileTokens.label, /estimated tokens per hint/i);
	assert.match(ko.results.tileTokens.label, /힌트당 추정 토큰/);
	for (const copy of [en, ko]) {
		const text = leaves(copy).filter(([, value]) => typeof value === 'string').map(([, value]) => value).join('\n');
		assert.doesNotMatch(text, /tokens in total|합쳐서.*토큰/);
	}
	const english = await readFile(new URL('../../README.md', import.meta.url), 'utf8');
	const korean = await readFile(new URL('../../README.ko.md', import.meta.url), 'utf8');
	assert.match(english.split('\n').find(line => line.includes('Approved per-site workflow catalog')), /each.*400 estimated tokens/);
	assert.match(korean.split('\n').find(line => line.includes('사이트별 승인된 워크플로 카탈로그')), /각.*400.*추정 토큰/);
});

test('privacy copy limits its promise to WAM instead of all host and model traffic', () => {
	for (const copy of [en, ko]) {
		const text = leaves(copy).filter(([, value]) => typeof value === 'string').map(([, value]) => value).join('\n');
		assert.doesNotMatch(text, /stay on your device|내 기기에 남/);
		assert.match(copy.contribute.text, /WAM/);
	}
});
