import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'node:test';
import ts from 'typescript';

const dir = await mkdtemp(join(tmpdir(), 'wam-my-page-test-'));
const source = await readFile(new URL('../src/lib/my-page.ts', import.meta.url), 'utf8');
const js = ts.transpileModule(source, {
	compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
}).outputText;
const modulePath = join(dir, 'my-page.mjs');
await writeFile(modulePath, js);
const myPage = await import(pathToFileURL(modulePath).href);

test('worker-backed contribution results take precedence over legacy status strings', () => {
	const supported = { id: 'c1', url_pattern: 'https://example.org/a', task_category: 'navigate', actions: [], status: 'approved', submitted_at: 1, reflection: { kind: 'supported', memory_id: 'm1', version_id: 'v1' } };
	const unknown = { ...supported, id: 'c2', reflection: undefined };
	assert.equal(myPage.contributionStatus(supported).key, 'supported');
	assert.equal(myPage.contributionStatus(unknown).key, 'unknown');
});

test('owner wait reasons use stable safe copy and leave unknown reasons generic', () => {
	const item = { id: 'c1', url_pattern: 'https://example.org/a', task_category: 'navigate', actions: [], status: 'queued', submitted_at: 1, progress: { stage: 'retry_scheduled', reason: 'model_budget' } };
	assert.match(myPage.contributionDescription(item), /model budget/i);
	assert.doesNotMatch(myPage.contributionDescription({ ...item, progress: { ...item.progress, reason: 'private-provider-error' } }), /private-provider-error/);
	const withoutProgress = { ...item, progress: undefined };
	assert.equal(myPage.contributionDescription(withoutProgress), myPage.contributionStatus(withoutProgress).description);
});

test.after(async () => rm(dir, { recursive: true, force: true }));
