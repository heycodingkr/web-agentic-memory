import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/lib/local-search.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {
	compilerOptions: {
		target: ts.ScriptTarget.ES2022,
		module: ts.ModuleKind.ES2022
	}
}).outputText;
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`;
const { rankCatalogMemories } = await import(moduleUrl);

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
