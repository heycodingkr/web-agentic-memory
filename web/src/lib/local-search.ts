import type { CatalogMemory } from './api';
import { bm25, tokenize, type WeightedField } from './search/bm25.js';

const RRF_K = 60;

/** Searches the already-loaded public catalog; the query never leaves the browser. */
export function rankCatalogMemories(
	memories: CatalogMemory[],
	query: string,
	taskCategory: string
): CatalogMemory[] {
	const scoped = memories.filter((memory) => taskCategory === 'all' || memory.task_category === taskCategory);
	const terms = [...new Set(tokenize(query))];
	if (!terms.length) return scoped;

	const contentFields: WeightedField[][] = scoped.map((memory) => [
		{ value: memory.title, boost: 4 },
		{ value: memory.task_category, boost: 2 },
		{ value: memory.summary, boost: 1 }
	]);
	const scopeFields: WeightedField[][] = scoped.map((memory) => [
		{ value: memory.domain, boost: 3 },
		{ value: memory.site, boost: 2 },
		{ value: memory.canonical_url_patterns.join(' '), boost: 1 }
	]);
	const contentScores = bm25(contentFields, terms);
	const scopeScores = bm25(scopeFields, terms);
	const fused = scoped.map((memory, index) => ({ memory, index, score: 0 }));

	for (const scores of [contentScores, scopeScores]) {
		const ranked = scores
			.map((score, index) => ({ score, index }))
			.filter((item) => item.score > 0)
			.sort((left, right) => right.score - left.score || left.index - right.index);
		ranked.forEach((item, rank) => {
			fused[item.index].score += 1 / (RRF_K + rank + 1);
		});
	}

	return fused
		.filter((item) => item.score > 0)
		.sort((left, right) => right.score - left.score || left.index - right.index)
		.map((item) => item.memory);
}
