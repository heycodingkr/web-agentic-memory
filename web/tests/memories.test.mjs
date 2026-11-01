import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';
import ts from 'typescript';

// Load the memories helpers and the real search straight from the TypeScript sources (type imports are erased).
const root = new URL('../src/lib/', import.meta.url);
const dir = await mkdtemp(join(tmpdir(), 'wam-memories-test-'));
for (const [name, path] of Object.entries({ memories: 'memories.ts', search: 'local-search.ts' })) {
	const source = await readFile(new URL(path, root), 'utf8');
	const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
	await writeFile(join(dir, `${name}.mjs`), js);
}
const load = (name) => import(pathToFileURL(join(dir, `${name}.mjs`)).href);
const m = await load('memories');
const { rankCatalogMemories } = await load('search');

// The bundled examples, turned into what GET /v1/memories returns for them.
const seed = JSON.parse(await readFile(new URL('../../crates/wam-server/seed/example_memories.json', import.meta.url), 'utf8'));
const catalog = seed.map(({ id, payload: { l0, l1 } }) => ({
	memory_id: id,
	version_id: `ver-${id}`,
	title: l1.title,
	summary: l1.summary,
	site: l0.site_host,
	domain: l0.site_host,
	task_category: l0.task_category,
	kind: l0.kind,
	status: 'active',
	verified_date: '1793318400',
	canonical_url_patterns: l0.url_patterns.map((route) => `https://${l0.site_host}${route.replace(/\/\*$/, '/**')}`)
}));
const find = (query, filters = {}) =>
	rankCatalogMemories(m.applyFilters(catalog, { query, product: 'all', task: 'all', source: 'all', ...filters }), m.expandQuery(query), 'all').map((memory) => memory.memory_id);

test('featured document templates exist in the real catalog and English or Korean searches find them', () => {
    assert.equal(m.DOCUMENT_TEMPLATES.length, 3);
    for (const template of m.DOCUMENT_TEMPLATES) {
        const memory = catalog.find(row => row.memory_id === template.id);
        assert.ok(memory, template.id);
        assert.equal(m.productOf(memory).key, 'docs');
        assert.ok(template.prompt.includes('Google Docs'));
    }
    for (const [query, id] of [
        ['request form', 'web:example-docs-request-form'],
        ['신청서', 'web:example-docs-request-form'],
        ['구매 양식', 'web:example-docs-request-form'],
        ['project proposal', 'web:example-docs-template-gallery'],
        ['제안서', 'web:example-docs-template-gallery'],
        ['회의록', 'web:example-docs-meeting-notes']
    ]) {
        assert.equal(find(query, { product: 'docs' })[0], id, query);
    }
});

test('a product is told apart by host and path, so Docs, Sheets, Slides and Forms are not mixed up', () => {
	const of = (domain, pattern) => m.productOf({ domain, canonical_url_patterns: [pattern], task_category: 'compose-draft' }).key;
	assert.equal(of('docs.google.com', 'https://docs.google.com/document/**'), 'docs');
	assert.equal(of('docs.google.com', 'https://docs.google.com/spreadsheets/**'), 'sheets');
	assert.equal(of('docs.google.com', 'https://docs.google.com/presentation/**'), 'slides');
	assert.equal(of('docs.google.com', 'https://docs.google.com/forms/**'), 'forms');
	assert.equal(of('mail.google.com', 'https://mail.google.com/mail/**'), 'gmail');
	assert.equal(of('en.wikipedia.org', 'https://en.wikipedia.org/wiki/**'), 'wikipedia');
	assert.equal(of('www.notion.so', 'https://www.notion.so/**'), 'notion');
	const unknown = m.productOf({ domain: 'shop.example.org', canonical_url_patterns: ['https://shop.example.org/cart/**'], task_category: 'navigate' });
	assert.deepEqual([unknown.label, unknown.glyph], ['shop.example.org', 'route']);
});

test('dates, example labels and recall commands are shown the way people read them', () => {
	assert.equal(m.formatDate('1793318400'), '2026-10-30');
	assert.equal(m.formatDate('Local demo seed'), 'Local demo seed');
	assert.equal(m.isExample({ title: 'Example: Google Docs meeting notes' }), true);
	assert.equal(m.isExample({ title: 'Save a clean draft' }), false);
	assert.equal(m.displayTitle({ title: 'Example: Google Docs meeting notes' }), 'Google Docs meeting notes');
	assert.equal(m.recallCommand({ canonical_url_patterns: ['https://docs.google.com/document/**'], task_category: 'compose-draft' }), 'wam recall https://docs.google.com/document/ --task compose-draft --json');
	assert.equal(m.taskLabel('compose-draft'), 'Create & write');
});

test('level 2 text is read as steps, lines and notes, from JSON, newline text or the older single line', () => {
	const json = JSON.stringify({ steps: [{ action: 'navigate', note_code: 'new-document' }, { action: 'click', when: 'dialog-open', note_code: 'ok' }], preconditions: ['signed-in'], recovery: ['Press Escape.'], detail: '1. Open it.\n2. Type.\n\nNotes: Observed once.' });
	const parsed = m.parseLevel2(json);
	assert.deepEqual(parsed.steps.map((step) => step.action), ['navigate', 'click']);
	assert.deepEqual(parsed.lines, ['Open it.', 'Type.']);
	assert.equal(parsed.notes, 'Observed once.');
	assert.deepEqual([parsed.preconditions, parsed.recovery], [['signed-in'], ['Press Escape.']]);
	const old = m.parseLevel2(JSON.stringify({ steps: [{ action: 'navigate' }], detail: '1. Open the editor. 2. Add a title. 3. Save.' }));
	assert.deepEqual(old.lines, ['Open the editor.', 'Add a title.', 'Save.']);
	assert.deepEqual(m.parseLevel2('1. Plain. 2. Text.').lines, ['Plain.', 'Text.']);
	assert.deepEqual(m.parseLevel2('{not json').steps, []);
	assert.equal(m.actionLabel('key'), 'Press');
	assert.equal(m.humanize('saved-indicator'), 'Saved indicator');
});

test('a level 1 hint is split into its title, summary, key points and pitfalls', () => {
	const hint = m.parseHint('Example: X\nDo the thing.\n- first point\n- second point\npitfalls: untitled-document, stray-row');
	assert.deepEqual(hint, { title: 'Example: X', summary: 'Do the thing.', points: ['first point', 'second point'], pitfalls: ['untitled-document', 'stray-row'] });
});

test('filters narrow by product, task and source', () => {
	assert.ok(m.applyFilters(catalog, { query: '', product: 'sheets', task: 'all', source: 'all' }).every((memory) => m.productOf(memory).key === 'sheets'));
	assert.deepEqual(m.applyFilters(catalog, { query: '', product: 'all', task: 'research', source: 'all' }).map((memory) => memory.memory_id), ['web:example-wikipedia-facts']);
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'contributed' }).length, 0);
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'examples' }).length, catalog.length);
	const counts = m.productCounts(catalog);
	assert.equal(counts.reduce((sum, row) => sum + row.count, 0), catalog.length);
	assert.deepEqual(counts.slice(0, 3).map((row) => row.key), ['docs', 'sheets', 'gmail'], 'everyday tools come first');
	assert.deepEqual(m.libraryOrder(catalog).slice(0, 3).map((memory) => m.productOf(memory).key), ['docs', 'docs', 'docs']);
});

test('pasted URLs scope workflows by host and route instead of matching common Google words', () => {
	const expected = (product) => catalog.filter(memory => m.productOf(memory).key === product).map(memory => memory.memory_id).sort();
	for (const query of ['https://docs.google.com/spreadsheets/d/private-sheet/edit?token=private#gid=12', 'docs.google.com/spreadsheets/']) {
		assert.deepEqual(find(query).sort(), expected('sheets'), query);
	}
	assert.deepEqual(find('https://docs.google.com/document/d/private-document/edit').sort(), expected('docs'));
	assert.deepEqual(find('docs.google.com').sort(), catalog.filter(memory => memory.domain === 'docs.google.com').map(memory => memory.memory_id).sort());
	assert.deepEqual(find('https://unknown.example.com/document/edit?google=docs'), []);
	assert.deepEqual(find('https://docs.google.com.evil.example/spreadsheets/'), []);
	assert.deepEqual(find('notion.so').sort(), expected('notion'));
	assert.deepEqual(find('https://docs.google.com/spreadsheets/../document/d/private/edit').sort(), expected('docs'));
});

test('people find the bundled examples with the words they would use', () => {
	assert.equal(find('work log')[0], 'web:example-sheets-work-log');
	assert.equal(find('timesheet')[0], 'web:example-sheets-work-log');
	assert.equal(find('meeting notes')[0], 'web:example-docs-meeting-notes');
	assert.equal(find('email')[0], 'web:example-gmail-compose');
	assert.equal(find('mail')[0], 'web:example-gmail-compose');
	assert.equal(find('send an email')[0], 'web:example-gmail-compose');
	assert.equal(find('calendar event')[0], 'web:example-calendar-event');
	assert.equal(find('expense tracker')[0], 'web:example-sheets-expense-tracker');
	assert.ok(find('spreadsheet formulas').slice(0, 2).every((id) => id.startsWith('web:example-sheets-')));
	assert.ok(find('google docs template').slice(0, 3).every((id) => id.startsWith('web:example-docs-') || id.includes('notion')));
	assert.deepEqual(find('zzzz nothing matches this'), []);
	for (const suggestion of m.SUGGESTIONS) assert.ok(find(suggestion).length > 0, `the suggestion "${suggestion}" must find something`);
});
