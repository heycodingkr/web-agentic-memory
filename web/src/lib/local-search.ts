import type { CatalogMemory } from './api';

const RRF_K = 60;
const K1 = 1.2;
const B = 0.75;

type Field = { value: string; boost: number };

/** Searches the already-loaded public catalog; the query never leaves the browser. */
export function rankCatalogMemories(
	memories: CatalogMemory[],
	query: string,
	taskCategory: string
): CatalogMemory[] {
	const scoped = memories.filter((memory) => taskCategory === 'all' || memory.task_category === taskCategory);
	const terms = [...new Set(tokenize(query))];
	if (!terms.length) return scoped;

	const contentFields = scoped.map((memory) => [
		{ value: memory.title, boost: 4 },
		{ value: memory.task_category, boost: 2 },
		{ value: memory.summary, boost: 1 }
	]);
	const scopeFields = scoped.map((memory) => [
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

function bm25(fields: Field[][], terms: string[]): number[] {
	const scores = fields.map(() => 0);
	for (let fieldIndex = 0; fieldIndex < (fields[0]?.length ?? 0); fieldIndex += 1) {
		const documents = fields.map((document) => tokenize(document[fieldIndex].value));
		const lengths = documents.map((document) => document.length);
		const averageLength = lengths.reduce((sum, length) => sum + length, 0) / (documents.length || 1);
		if (!averageLength) continue;
		const boost = fields[0][fieldIndex].boost;

		for (const term of terms) {
			const documentFrequency = documents.filter((document) => document.includes(term)).length;
			if (!documentFrequency) continue;
			const idf = Math.log(1 + (documents.length - documentFrequency + 0.5) / (documentFrequency + 0.5));
			for (let index = 0; index < documents.length; index += 1) {
				const termFrequency = documents[index].filter((value) => value === term).length;
				if (!termFrequency) continue;
				const normalization = K1 * (1 - B + B * lengths[index] / averageLength);
				scores[index] += boost * idf * (termFrequency * (K1 + 1)) / (termFrequency + normalization);
			}
		}
	}
	return scores;
}

function tokenize(value: string): string[] {
	return value.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];
}
