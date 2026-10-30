import test from 'node:test';
import assert from 'node:assert/strict';
import { createTraceId, flushTelemetry, isTraceId, operationForPath, reportTelemetry, resetTelemetryForTests, resolveTraceId, telemetryEnabled, telemetryEvent } from '../src/lib/telemetry.js';

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

test('events use the closed schema and exclude arbitrary diagnostic data', () => {
	const event = telemetryEvent({
		operation: 'recall?query=PRIVATE_USER_INPUT', error_code: 'http_error', status_code: 503,
		duration_ms: 9999999, trace_id: 'bad-trace-id', message: 'PRIVATE_MESSAGE',
		stack: 'PRIVATE_STACK', url: '/PRIVATE_PAGE?account=PRIVATE_ACCOUNT', headers: 'PRIVATE_COOKIE'
	});
	assert.deepEqual(Object.keys(event).sort(), ['duration_ms', 'error_code', 'event_id', 'operation', 'schema_version', 'source', 'status_code', 'trace_id'].sort());
	assert.match(event.event_id, /^[a-f0-9]{32}$/);
	assert.match(event.trace_id, /^[a-f0-9]{32}$/);
	assert.equal(event.operation, 'unknown');
	assert.equal(event.duration_ms, 600000);
	assert.equal(JSON.stringify(event).includes('PRIVATE_'), false);
	assert.equal(createTraceId().length, 32);
});

test('fixed route templates classify memory detail and review before contribution', () => {
	assert.equal(operationForPath('/v1/memories?site=example.test'), 'catalog');
	assert.equal(operationForPath('/v1/memories/vocab'), 'vocab');
	assert.equal(operationForPath('/v1/memories/mem-123?level=2'), 'expand');
	assert.equal(operationForPath('/v1/review/contributions/c-123/decision'), 'review');
	assert.equal(operationForPath('/v1/me/contributions/c-123'), 'contribution');
	assert.equal(operationForPath('/v1/telemetry/events'), 'telemetry');
	assert.equal(operationForPath('/v1/review/not-a-route'), 'unknown');
	assert.equal(operationForPath('/v1/some-contributionish-path'), 'unknown');
	assert.equal(isTraceId('a'.repeat(32)), true);
	assert.equal(isTraceId('A'.repeat(32)), false);
	assert.equal(isTraceId('a'.repeat(31)), false);
	assert.equal(resolveTraceId('INVALID', 'b'.repeat(32)), 'b'.repeat(32));
	assert.equal(resolveTraceId('c'.repeat(32), 'b'.repeat(32)), 'c'.repeat(32));
});

test('explicit local opt-out prevents capture and delivery', async () => {
	resetTelemetryForTests();
	const oldStorage = globalThis.localStorage;
	const oldFetch = globalThis.fetch;
	let sent = 0;
	globalThis.localStorage = { getItem: () => 'false' };
	globalThis.fetch = async () => { sent++; };
	try {
		assert.equal(telemetryEnabled(), false);
		reportTelemetry({ operation: 'catalog', error_code: 'network_error' });
		await tick();
		assert.equal(sent, 0);
	} finally {
		globalThis.localStorage = oldStorage;
		globalThis.fetch = oldFetch;
	}
});

test('telemetry delivery failure is swallowed and does not recursively report', async () => {
	resetTelemetryForTests();
	const oldFetch = globalThis.fetch;
	let calls = 0;
	globalThis.fetch = async (url) => {
		calls++;
		assert.equal(url, '/v1/telemetry/events');
		throw new Error('PRIVATE_TRANSPORT_DETAIL');
	};
	try {
		assert.match(reportTelemetry({ operation: 'auth', error_code: 'http_error', status_code: 500 }), /^[a-f0-9]{32}$/);
		await tick();
		assert.equal(calls, 1);
	} finally {
		globalThis.fetch = oldFetch;
		resetTelemetryForTests();
	}
});

test('offline rejection keeps the same event for an online retry after cooldown', async () => {
	resetTelemetryForTests();
	const oldFetch = globalThis.fetch;
	const oldNow = Date.now;
	let now = 100000;
	let calls = 0;
	const bodies = [];
	Date.now = () => now;
	globalThis.fetch = async (_url, init) => {
		calls++;
		bodies.push(JSON.parse(init.body));
		if (calls === 1) throw new Error('offline');
		return { ok: true };
	};
	try {
		reportTelemetry({ operation: 'recall', error_code: 'network_error' });
		await tick();
		assert.equal(calls, 1);
		await new Promise((resolve) => setTimeout(resolve, 20));
		assert.equal(calls, 1, 'failure must not start a busy retry loop');
		await flushTelemetry();
		assert.equal(calls, 1, 'cooldown blocks immediate retry');
		now += 5000;
		await flushTelemetry();
		assert.equal(calls, 2);
		assert.equal(bodies[1].event_id, bodies[0].event_id);
		assert.equal(bodies[1].trace_id, bodies[0].trace_id);
	} finally {
		Date.now = oldNow;
		globalThis.fetch = oldFetch;
		resetTelemetryForTests();
	}
});

test('opt-out clears queued events and stops offline retries', async () => {
	resetTelemetryForTests();
	const oldStorage = globalThis.localStorage;
	const oldFetch = globalThis.fetch;
	const oldNow = Date.now;
	let now = 200000;
	let enabled = true;
	let calls = 0;
	Date.now = () => now;
	globalThis.localStorage = { getItem: () => enabled ? 'true' : 'false' };
	globalThis.fetch = async () => { calls++; throw new Error('offline'); };
	try {
		reportTelemetry({ operation: 'dashboard', error_code: 'network_error' });
		await tick();
		assert.equal(calls, 1);
		enabled = false;
		await flushTelemetry();
		enabled = true;
		now += 6000;
		await flushTelemetry();
		assert.equal(calls, 1, 'opt-out drops pending events and prevents a later retry');
	} finally {
		Date.now = oldNow;
		globalThis.localStorage = oldStorage;
		globalThis.fetch = oldFetch;
		resetTelemetryForTests();
	}
});
