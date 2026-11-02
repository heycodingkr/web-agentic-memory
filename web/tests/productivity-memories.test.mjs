import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { loadTypeScriptModules } from './helpers/load-typescript.mjs';

const modules = await loadTypeScriptModules(['memories', 'local-search', 'search/bm25']);
const m = await modules.load('memories');
const { rankCatalogMemories } = await modules.load('local-search');
await modules.cleanup();
const seed = JSON.parse(await readFile(
    new URL('../../crates/wam-server/seed/productivity_memories.json', import.meta.url), 'utf8'
));
const catalog = seed.map(({ id, payload: { l0, l1 } }) => ({
    memory_id: id, version_id: 'v1', title: l1.title, summary: l1.summary,
    site: l0.site_host, domain: l0.site_host, task_category: l0.task_category,
    kind: l0.kind, status: 'active', verified_date: '1793318400',
    canonical_url_patterns: l0.url_patterns.map(route => `https://${l0.site_host}${route}`)
}));

test('Canva and current/legacy Notion app domains have useful product filters', () => {
    for (const [domain, key, label] of [
        ['www.canva.com', 'canva', 'Canva'],
        ['app.notion.com', 'notion', 'Notion'],
        ['www.notion.so', 'notion', 'Notion']
    ]) {
        const result = m.productOf({ domain, canonical_url_patterns: [`https://${domain}/**`], task_category: 'compose-draft' });
        assert.deepEqual([result.key, result.label], [key, label]);
    }
    const marketing = m.productOf({
        domain: 'www.notion.com',
        canonical_url_patterns: ['https://www.notion.com/help/**'],
        task_category: 'research'
    });
    assert.notEqual(marketing.key, 'notion');
});

test('requested workflows are searchable within the product selected in the library', () => {
    for (const [product, query, expected] of [
        ['canva', 'Canva template', 'web:example-canva-template'],
        ['canva', 'Canva replace text image', 'web:example-canva-replace-content'],
        ['canva', 'Canva PNG PDF download', 'web:example-canva-export-pages'],
        ['calendar', 'Google Calendar repeating', 'web:example-calendar-recurring'],
        ['calendar', 'Google Calendar time zone', 'web:example-calendar-timezone'],
        ['notion', 'Notion task database', 'web:example-notion-task-database'],
        ['notion', 'Notion database item properties', 'web:example-notion-database-entry'],
        ['notion', 'Notion database filter', 'web:example-notion-database-views']
    ]) {
        const filtered = m.applyFilters(catalog, { query, product, task: 'all', source: 'examples' });
        const results = rankCatalogMemories(filtered, m.expandQuery(query), 'all');
        assert.ok(results.slice(0, 3).some(row => row.memory_id === expected), `${query}: ${results.map(row => row.memory_id)}`);
        assert.ok(results.every(row => m.productOf(row).key === product));
    }
});
