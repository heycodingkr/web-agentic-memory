import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';
import ts from 'typescript';

// Site registration workflows (issue #217): Search Console, Bing Webmaster Tools and Cloudflare DNS.
// Load the memories helpers and the real search straight from the TypeScript sources (type imports are erased).
const root = new URL('../src/lib/', import.meta.url);
const dir = await mkdtemp(join(tmpdir(), 'wam-seo-memories-test-'));
for (const [name, path] of Object.entries({ memories: 'memories.ts', search: 'local-search.ts' })) {
	const source = await readFile(new URL(path, root), 'utf8');
	const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
	await writeFile(join(dir, `${name}.mjs`), js);
}
const load = (name) => import(pathToFileURL(join(dir, `${name}.mjs`)).href);
const m = await load('memories');
const { rankCatalogMemories } = await load('search');
await rm(dir, { recursive: true, force: true });

const seed = JSON.parse(await readFile(new URL('../../crates/wam-server/seed/seo_memories.json', import.meta.url), 'utf8'));
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
const byId = (id) => catalog.find((memory) => memory.memory_id === id);
const find = (query) =>
	rankCatalogMemories(m.applyFilters(catalog, { query, product: 'all', task: 'all', source: 'all' }), m.expandQuery(query), 'all').map((memory) => memory.memory_id);

test('each registration workflow is shown under its own product name', () => {
	assert.equal(m.productOf(byId('web:example-search-console-domain-verify')).label, 'Google Search Console');
	assert.equal(m.productOf(byId('web:example-bing-webmaster-add-site')).label, 'Bing Webmaster Tools');
	assert.equal(m.productOf(byId('web:example-cloudflare-dns-record-gotcha')).label, 'Cloudflare');
});

test('only the Search Console and Bing Webmaster sections get those names, not the whole search host', () => {
	const scope = (host, path) => ({ domain: host, canonical_url_patterns: [`https://${host}${path}`], task_category: 'navigate' });
	assert.equal(m.productOf(scope('search.google.com', '/search/**')).label, 'search.google.com');
	assert.equal(m.productOf(scope('www.bing.com', '/search/**')).label, 'bing.com');
	assert.equal(m.productOf(scope('search.google.com', '/search-console/**')).key, 'search-console');
	assert.equal(m.productOf(scope('bing.com', '/webmasters/**')).key, 'bing-webmaster');
});

test('natural queries find the right workflow first', () => {
	assert.equal(find('search console sitemap')[0], 'web:example-search-console-domain-verify');
	assert.equal(find('add my domain to google seo')[0], 'web:example-search-console-domain-verify');
	assert.equal(find('bing webmaster verify cname')[0], 'web:example-bing-webmaster-add-site');
	assert.equal(find('cloudflare proxy dns only')[0], 'web:example-cloudflare-dns-record-gotcha');
});

test('they are browsable as examples and none of them claims to be a verified entry', () => {
	for (const memory of catalog) {
		assert.equal(m.isExample(memory), true, memory.memory_id);
		assert.ok(m.displayTitle(memory).length > 10, memory.memory_id);
	}
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'contributed' }).length, 0);
	assert.equal(m.applyFilters(catalog, { query: '', product: 'all', task: 'all', source: 'examples' }).length, catalog.length);
});
