import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadTypeScriptModules } from './helpers/load-typescript.mjs';

const modules = await loadTypeScriptModules(['local-search', 'search/bm25']);
const { rankCatalogMemories } = await modules.load('local-search');
await modules.cleanup();

function memory(memoryId, { title, summary, taskCategory = 'compose-draft' }) {
	return {
		memory_id: memoryId,
		version_id: 'v1',
		title,
		summary,
		site: 'Example',
		domain: 'example.com',
		task_category: taskCategory,
		kind: 'workflow',
		status: 'active',
		verified_date: '2026-10-01',
		canonical_url_patterns: ['https://example.com/workflows/:id']
	};
}

const catalog = [
	memory('title-match', { title: 'Draft editor workflow', summary: 'Save without publishing.' }),
	memory('summary-match', { title: 'Writing guide', summary: 'Draft and review the page in the editor.' }),
	memory('other-task', {
		title: 'Browse the site',
		summary: 'Use the navigation menu.',
		taskCategory: 'navigate'
	})
];

test('catalog search ranks title matches and excludes documents with no lexical match', () => {
	const results = rankCatalogMemories(catalog, 'draft editor', 'all');
	assert.equal(results[0].memory_id, 'title-match');
	assert.equal(results[1].memory_id, 'summary-match');
	assert.deepEqual(rankCatalogMemories(catalog, 'unmatched phrase', 'all'), []);
});

test('task filters apply before ranking and empty queries preserve catalog order', () => {
	assert.deepEqual(
		rankCatalogMemories(catalog, 'draft editor', 'compose-draft').map((item) => item.memory_id),
		['title-match', 'summary-match']
	);
	assert.deepEqual(
		rankCatalogMemories(catalog, '', 'navigate').map((item) => item.memory_id),
		['other-task']
	);
});

test('search normalizes Unicode, ignores repeated query terms and preserves stable ties', () => {
	const repeated = memory('repeated', { title: 'alpha alpha alpha', summary: '' });
	const single = memory('single', { title: 'alpha', summary: '' });
	const tied = memory('tied', { title: 'alpha', summary: '' });
	const rows = [repeated, single, tied];

	for (const query of ['ＡＬＰＨＡ', 'alpha', 'alpha alpha']) {
		assert.deepEqual(
			rankCatalogMemories(rows, query, 'all').map((item) => item.memory_id),
			['repeated', 'single', 'tied']
		);
	}
	assert.deepEqual(
		rankCatalogMemories([single, tied], 'alpha', 'all').map((item) => item.memory_id),
		['single', 'tied']
	);
});

test('empty catalogs and empty fields return no matches without invalid scores', () => {
	assert.deepEqual(rankCatalogMemories([], 'anything', 'all'), []);
	const empty = memory('empty-fields', { title: '', summary: '' });
	empty.site = '';
	empty.domain = '';
	empty.canonical_url_patterns = [];
	assert.deepEqual(rankCatalogMemories([empty], 'anything', 'all'), []);
});
