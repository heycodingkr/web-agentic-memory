import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { loadTypeScriptModules } from './helpers/load-typescript.mjs';

// Trip.com flight search example (issue #270). Its only measured evidence is a preliminary individual-run
// observation (docs/test/benchmarks/trip-flights.md), so it is browsed as a built-in example like the others.
const modules = await loadTypeScriptModules(['memories', 'local-search', 'search/bm25']);
const m = await modules.load('memories');
const { rankCatalogMemories } = await modules.load('local-search');
await modules.cleanup();

const seed = JSON.parse(await readFile(new URL('../../crates/wam-server/seed/travel_memories.json', import.meta.url), 'utf8'));
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
	verified_date: '1793491200',
	canonical_url_patterns: l0.url_patterns.map((route) => `https://${l0.site_host}${route.replace(/\/\*$/, '/**')}`)
}));
const find = (query) =>
	rankCatalogMemories(m.applyFilters(catalog, { query, product: 'all', task: 'all', source: 'all' }), m.expandQuery(query), 'all').map((memory) => memory.memory_id);

test('the travel seed holds the one flight search example', () => {
	assert.deepEqual(catalog.map((memory) => memory.memory_id), ['web:example-trip-flight-search']);
});

test('it is shown under the Trip.com product name and browsed with research', () => {
	const [memory] = catalog;
	assert.equal(m.productOf(memory).label, 'Trip.com');
	assert.equal(m.productOf(memory).key, 'trip');
	assert.equal(m.categoryOf(memory), 'research');
});

test('both Trip.com host spellings get the name and other hosts do not', () => {
	const scope = (host) => ({ domain: host, canonical_url_patterns: [`https://${host}/flights/**`], task_category: 'research' });
	assert.equal(m.productOf(scope('www.trip.com')).key, 'trip');
	assert.equal(m.productOf(scope('trip.com')).key, 'trip');
	assert.equal(m.productOf(scope('www.nottrip.com')).label, 'nottrip.com');
	assert.notEqual(m.productOf(scope('www.nottrip.com')).key, 'trip');
});

test('natural queries find it', () => {
	assert.equal(find('trip.com flights')[0], 'web:example-trip-flight-search');
	assert.equal(find('cheapest nonstop flight')[0], 'web:example-trip-flight-search');
	assert.equal(find('one-way airfare search')[0], 'web:example-trip-flight-search');
});

test('it is browsable as an example and does not claim to be a verified entry', () => {
	const [memory] = catalog;
	assert.equal(m.isExample(memory), true);
	assert.ok(m.displayTitle(memory).length > 10);
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'contributed' }).length, 0);
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'examples' }).length, 1);
});
