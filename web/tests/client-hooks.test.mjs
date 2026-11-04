import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { attachTelemetryContext, resetTelemetryForTests } from '../src/lib/telemetry.js';

const source = await readFile(new URL('../src/hooks.client.ts', import.meta.url), 'utf8');
const js = ts.transpileModule(source, {
	compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 }
}).outputText.replace('./lib/telemetry.js', new URL('../src/lib/telemetry.js', import.meta.url).href);
const { handleError } = await import('data:text/javascript;base64,' + Buffer.from(js).toString('base64'));

async function capture(run) {
	resetTelemetryForTests();
	const oldFetch = globalThis.fetch;
	const oldWindow = globalThis.window;
	const events = [];
	globalThis.window = { location: { pathname: '/dashboard' } };
	globalThis.fetch = async (_url, init) => {
		events.push(JSON.parse(init.body));
		return { ok: true };
	};
	try {
		await run(events);
		await new Promise((resolve) => setTimeout(resolve, 0));
	} finally {
		globalThis.fetch = oldFetch;
		globalThis.window = oldWindow;
		resetTelemetryForTests();
	}
}

function navigation(path, id) {
	return { url: new URL(path, 'https://example.test'), params: {}, route: { id } };
}

for (const [kind, status] of [['framework', 404], ['app', 403]]) {
	test(`${kind} ${status} keeps the framework response and records an ordinary HTTP error`, async () => {
		await capture(async (events) => {
			const result = handleError({
				kind, error: { status, message: 'PRIVATE_MESSAGE' },
				event: navigation('/memories/PRIVATE_ID?token=PRIVATE_TOKEN', '/memories/[slug]')
			});
			assert.equal(events.length, 1);
			assert.equal(events[0].error_code, 'http_error');
			assert.equal(events[0].status_code, status);
			assert.equal(events[0].operation, 'expand');
			assert.equal(JSON.stringify(events).includes('PRIVATE_'), false);
			assert.equal(result, undefined, 'SvelteKit must retain its safe status and message');
		});
	});
}

test('framework 5xx remains actionable and uses the destination route', async () => {
	await capture(async (events) => {
		const result = handleError({
			kind: 'framework', error: { status: 503, message: 'PRIVATE_MESSAGE' },
			event: navigation('/memories', '/memories')
		});
		assert.equal(events[0].error_code, 'unhandled_error');
		assert.equal(events[0].status_code, 503);
		assert.equal(events[0].operation, 'catalog');
		assert.equal(result.message, `Unexpected error. Reference: ${events[0].trace_id}`);
		assert.equal(JSON.stringify(events).includes('PRIVATE_'), false);
	});
});

test('unknown runtime errors with a 4xx property are still actionable', async () => {
	await capture(async (events) => {
		handleError({
			kind: 'unknown', error: Object.assign(new Error('PRIVATE_MESSAGE'), { status: 404 }),
			event: navigation('/settings/devices', '/settings/devices')
		});
		assert.equal(events[0].error_code, 'unhandled_error');
		assert.equal(events[0].operation, 'device');
		assert.equal(events[0].status_code, 404);
		assert.equal(JSON.stringify(events).includes('PRIVATE_'), false);
	});
});

test('an API failure keeps its originating trace and elapsed time across the navigation hook', async () => {
	await capture(async (events) => {
		const error = new Error('PRIVATE_MESSAGE');
		attachTelemetryContext(error, {
			operation: 'auth', trace_id: 'e'.repeat(32), status_code: 500, duration_ms: 27
		});
		const result = handleError({
			kind: 'unknown', error,
			event: navigation('/memories', '/memories')
		});
		assert.equal(events[0].trace_id, 'e'.repeat(32));
		assert.equal(events[0].operation, 'auth');
		assert.equal(events[0].duration_ms, 27);
		assert.equal(result.message, `Unexpected error. Reference: ${'e'.repeat(32)}`);
	});
});
