import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import ts from 'typescript';

const sourceRoot = new URL('../../src/lib/', import.meta.url);

/** Compile selected library modules to a temporary ESM tree for Node's built-in test runner. */
export async function loadTypeScriptModules(names) {
	const directory = await mkdtemp(join(tmpdir(), 'wam-typescript-test-'));
	await writeFile(join(directory, 'package.json'), JSON.stringify({ type: 'module' }));

	await Promise.all(names.map(async (name) => {
		const source = await readFile(new URL(`${name}.ts`, sourceRoot), 'utf8');
		const compiled = ts.transpileModule(source, {
			compilerOptions: {
				target: ts.ScriptTarget.ES2022,
				module: ts.ModuleKind.ES2022
			}
		}).outputText;
		const output = join(directory, `${name}.js`);
		await mkdir(dirname(output), { recursive: true });
		await writeFile(output, compiled);
	}));

	return {
		load: (name) => import(pathToFileURL(join(directory, `${name}.js`)).href),
		cleanup: () => rm(directory, { recursive: true, force: true })
	};
}
