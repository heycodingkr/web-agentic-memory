import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';
import ts from 'typescript';

// Load the landing data and both language files straight from the TypeScript sources: transpile each file,
// flatten them into one temp folder and point their relative imports at the flattened copies.
const root = new URL('../src/lib/', import.meta.url);
const sources = { landing: 'landing.ts', demo: 'demo.ts', en: 'landing/en.ts', ko: 'landing/ko.ts' };
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
	const { pilotRows, change, percent, averageTimeChange, averageCallChange, timeRange, callRange, hoursSavedPer1000Posts, recallLimits, iframeErrors } = data;
	assert.deepEqual(pilotRows.map((row) => percent(change(row.secondsBefore, row.secondsAfter))), ['−48%', '−30%', '−58%', '−33%']);
	assert.deepEqual(pilotRows.map((row) => percent(change(row.callsBefore, row.callsAfter))), ['−30%', '−25%', '−31%', '−39%']);
	assert.equal(percent(averageTimeChange), '−42%');
	assert.equal(percent(averageCallChange), '−31%');
	assert.deepEqual([...timeRange], ['−30%', '−58%']);
	assert.deepEqual([...callRange], ['−25%', '−39%']);
	assert.equal(Math.round(hoursSavedPer1000Posts), 11);
	assert.deepEqual({ ...recallLimits }, { hints: 3, tokens: 400 });
	assert.deepEqual({ ...iframeErrors }, { before: 3, after: 0 });
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
		assert.ok(copy.demo.blog.chipTime('−48%').includes('−48%'));
		assert.ok(copy.demo.blog.chipCalls('−30%').includes('−30%'));
		assert.ok(copy.results.range('−30%', '−58%').includes('−30%') && copy.results.range('−30%', '−58%').includes('−58%'));
		assert.ok(copy.results.seconds(85.5).includes('85.5'));
		assert.ok(copy.results.scaleBody(11).includes('11'));
		assert.ok(copy.results.tileTokens.small(3).includes('3'));
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

test('the Korean page is actually translated', () => {
	// Words that are names, symbols or already shared are allowed to match the English copy.
	const same = new Set(['switcher.en', 'switcher.ko', 'examples.cards.0.site', 'examples.cards.2.site', 'results.rowLabels.medium', 'contribute.points.0.value', 'contribute.points.1.value']);
	const english = new Map(leaves(en));
	for (const [path, value] of leaves(ko)) {
		if (typeof value !== 'string' || ['lang', 'path', 'ogImage'].includes(path) || path.startsWith('videos.')) continue;
		if (value === english.get(path)) assert.ok(same.has(path), `ko ${path} is still English: ${value}`);
	}
	assert.match(ko.hero.lede, /[가-힣]/);
	assert.match(ko.cta.text, /[가-힣]/);
	assert.equal(ko.path, '/ko');
	for (const name of Object.values(ko.videos)) assert.ok(name.endsWith('-ko'), name);
});
