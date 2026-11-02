const K1 = 1.2;
const B = 0.75;

export type WeightedField = { value: string; boost: number };

type DocumentTerms = {
	length: number;
	frequencies: Map<string, number>;
};

/** Scores documents with BM25 after indexing each field's token frequencies once. */
export function bm25(fields: WeightedField[][], terms: string[]): number[] {
	const scores = Array<number>(fields.length).fill(0);
	const fieldCount = fields[0]?.length ?? 0;

	for (let fieldIndex = 0; fieldIndex < fieldCount; fieldIndex += 1) {
		const documents: DocumentTerms[] = [];
		const documentFrequencies = new Map<string, number>();
		let totalLength = 0;

		for (const fieldsForDocument of fields) {
			const frequencies = new Map<string, number>();
			const tokens = tokenize(fieldsForDocument[fieldIndex].value);
			for (const token of tokens) frequencies.set(token, (frequencies.get(token) ?? 0) + 1);

			const document: DocumentTerms = { length: tokens.length, frequencies };
			documents.push(document);
			totalLength += document.length;
			for (const token of frequencies.keys()) {
				documentFrequencies.set(token, (documentFrequencies.get(token) ?? 0) + 1);
			}
		}

		const averageLength = totalLength / (documents.length || 1);
		if (!averageLength) continue;
		const boost = fields[0][fieldIndex].boost;

		for (const term of terms) {
			const documentFrequency = documentFrequencies.get(term) ?? 0;
			if (!documentFrequency) continue;
			const idf = Math.log(1 + (documents.length - documentFrequency + 0.5) / (documentFrequency + 0.5));

			for (let index = 0; index < documents.length; index += 1) {
				const document = documents[index];
				const termFrequency = document.frequencies.get(term) ?? 0;
				if (!termFrequency) continue;
				const normalization = K1 * (1 - B + B * document.length / averageLength);
				scores[index] += boost * idf * (termFrequency * (K1 + 1)) / (termFrequency + normalization);
			}
		}
	}
	return scores;
}

export function tokenize(value: string): string[] {
	return value.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];
}
