import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import ts from 'typescript';

const source = await readFile(new URL('../src/lib/admin-api.ts', import.meta.url), 'utf8');
const js = ts.transpileModule(source, {
	compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
}).outputText;

test('an unexpected admin API failure has a readable error and retains its status', async () => {
	const { AdminError } = await import('data:text/javascript;base64,' + Buffer.from(js).toString('base64'));
	const error = new AdminError(503);
	assert.ok(error instanceof Error);
	assert.equal(error.status, 503);
	assert.equal(error.message, "We couldn't complete the request. Try again shortly.");
});
