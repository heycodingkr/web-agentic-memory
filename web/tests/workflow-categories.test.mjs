import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const source = await readFile(new URL('../src/lib/memories.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, {
	compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
});
const m = await import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
const row = (host, path, title, task = 'compose-draft') => ({
	memory_id: host + path, version_id: 'v1', title, summary: title,
	domain: host, site: host, task_category: task, kind: 'workflow',
	status: 'active', verified_date: '1793491200', canonical_url_patterns: ['https://' + host + path]
});
const catalog = [
	row('docs.google.com', '/document/**', 'Example: Google Docs meeting notes'),
	row('docs.google.com', '/spreadsheets/**', 'Example: Google Sheets work log'),
	row('docs.google.com', '/presentation/**', 'Example: Google Slides presentation'),
	row('www.figma.com', '/design/**', 'Example: Figma responsive card'),
	row('stitch.withgoogle.com', '/projects/**', 'Example: Google Stitch mobile app'),
	row('mail.google.com', '/mail/**', 'Example: Gmail compose'),
	row('app.notion.com', '/**', 'Example: Notion task database'),
	row('www.notion.so', '/**', 'Example: Notion meeting notes'),
	row('en.wikipedia.org', '/wiki/**', 'Example: Wikipedia facts', 'research'),
	row('unknown.example.com', '/**', 'Example: Unclassified workflow')
];

test('large workflow categories group related tasks across products without treating every draft as a document', () => {
	assert.deepEqual(catalog.map(memory => m.categoryOf(memory)), [
		'documents', 'productivity', 'design', 'design', 'design',
		'communication', 'productivity', 'documents', 'research', 'other'
	]);
	const counts = m.categoryCounts(catalog);
	assert.equal(counts.reduce((sum, category) => sum + category.count, 0), catalog.length);
	assert.equal(counts.find(category => category.key === 'design').count, 3);
	assert.equal(counts.find(category => category.key === 'documents').count, 2);
	assert.deepEqual(counts.map(({ key, count }) => [key, count]), [
		['documents', 2], ['design', 3], ['productivity', 2],
		['communication', 1], ['research', 1], ['other', 1]
	]);
});

test('category, URL, product and source filters intersect without losing the existing all behavior', () => {
	const filters = { query: '', product: 'all', task: 'all', source: 'all' };
	assert.equal(m.applyFilters(catalog, filters).length, catalog.length);
	assert.deepEqual(m.applyFilters(catalog, { ...filters, category: 'design' }).map(memory => m.productOf(memory).key), ['slides', 'figma', 'stitch']);
	assert.deepEqual(m.applyFilters(catalog, { ...filters, category: 'design', query: 'https://www.figma.com/design/test-file' }).map(memory => memory.title), ['Example: Figma responsive card']);
	assert.equal(m.applyFilters(catalog, { ...filters, category: 'documents', product: 'figma' }).length, 0);
	assert.equal(m.applyFilters(catalog, { ...filters, category: 'design', source: 'contributed' }).length, 0);
});

test('design product detection uses exact domains and never classifies lookalikes as Figma or Stitch', () => {
	assert.equal(m.productOf(catalog[3]).key, 'figma');
	assert.equal(m.productOf(catalog[4]).key, 'stitch');
	for (const host of ['www.figma.com.evil.example', 'stitch.withgoogle.com.evil.example']) {
		assert.equal(m.categoryOf(row(host, '/design/**', 'Unrelated draft')), 'other');
	}
});
