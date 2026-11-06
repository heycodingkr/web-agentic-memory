const OPERATIONS = new Set([
	'recall', 'expand', 'vocab', 'catalog', 'profile', 'dashboard', 'contribution', 'outcome',
	'auth', 'device', 'review', 'usage', 'telemetry', 'unknown'
]);
const ERROR_CODES = new Set([
	'network_error', 'http_error', 'invalid_response', 'invalid_input', 'unhandled_error',
	'unhandled_rejection', 'panic', 'internal_error'
]);
const MAX_QUEUE = 20;
const TIMEOUT_MS = 1000;
const RETRY_COOLDOWN_MS = 5000;
const API_PATH = '/v1/telemetry/events';
const API_BASE = (import.meta.env?.VITE_WAM_API_URL || '').replace(/\/$/, '');

/** @typedef {{operation?: string, trace_id?: string, status_code?: number, duration_ms?: number}} TelemetryContext */

/** @type {ReturnType<typeof telemetryEvent>[]} */
let queue = [];
let flushing = false;
let retryNotBefore = 0;
/** @type {AbortController | undefined} */
let activeController;
/** @type {WeakMap<object, TelemetryContext>} */
const errorContexts = new WeakMap();

export function createTraceId() {
	const bytes = new Uint8Array(16);
	if (globalThis.crypto?.getRandomValues) globalThis.crypto.getRandomValues(bytes);
	else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function telemetryEnabled() {
	let enabled = true;
	try { enabled = globalThis.localStorage?.getItem('wam.telemetry.enabled') !== 'false'; }
	catch { return true; }
	if (!enabled) {
		queue = [];
		activeController?.abort();
	}
	return enabled;
}

/** @param {unknown} value @returns {value is string} */
export function isTraceId(value) {
	return typeof value === 'string' && /^[a-f0-9]{32}$/.test(value);
}

/** @param {unknown} responseId @param {string} requestId */
export function resolveTraceId(responseId, requestId) {
	return isTraceId(responseId) ? responseId : safeId(requestId);
}

/** @param {string} path */
export function operationForPath(path) {
	const route = path.split('?')[0];
	if (route === '/v1/telemetry/events') return 'telemetry';
	if (route === '/v1/memories/recall') return 'recall';
	if (route === '/v1/memories/vocab') return 'vocab';
	if (route === '/v1/memories') return 'catalog';
	if (/^\/v1\/memories\/[^/]+$/.test(route)) return 'expand';
	if (/^\/v1\/review\/contributions(?:\/[^/]+\/(?:claim|decision))?$/.test(route)) return 'review';
	if (/^\/v1\/me\/contributions(?:\/[^/]+)?$/.test(route)) return 'contribution';
	if (route === '/v1/me/dashboard' || route === '/v1/me/memories/analytics') return 'dashboard';
	if (route === '/v1/me/profile' || /^\/v1\/profiles\/[^/]+$/.test(route)) return 'profile';
	if (route === '/v1/me/usage' || route === '/v1/metrics') return 'usage';
	if (/^\/v1\/auth\/(?:config|session|logout|github\/(?:start|callback)|email\/(?:start|verify))$/.test(route) || route === '/v1/me/account' || route === '/v1/me/consent') return 'auth';
	if (/^\/v1\/(?:link\/(?:start|poll|preview|approve|deny)|me\/devices(?:\/[^/]+\/(?:revoke|rotate))?)$/.test(route)) return 'device';
	if (/^\/v1\/receipts\/[^/]+\/outcome$/.test(route)) return 'outcome';
	return 'unknown';
}

/** @param {string} path */
export function operationForPagePath(path) {
	const route = path.split(/[?#]/)[0];
	if (route === '/dashboard') return 'dashboard';
	if (route === '/login' || route === '/settings/profile' || route === '/oauth/approve') return 'auth';
	if (route === '/contribute') return 'contribution';
	if (route === '/link' || route === '/settings/devices') return 'device';
	if (route === '/review') return 'review';
	if (route === '/memories') return 'catalog';
	if (/^\/memories\/[^/]+$/.test(route)) return 'expand';
	if (route === '/leaderboard' || route === '/metrics') return 'usage';
	if (/^\/u\/[^/]+$/.test(route)) return 'profile';
	return 'unknown';
}

/** @param {unknown} error @param {{operation?: string, trace_id?: string, status_code?: number, duration_ms?: number}} details */
export function attachTelemetryContext(error, details) {
	if ((typeof error === 'object' && error !== null) || typeof error === 'function') {
		errorContexts.set(error, details);
	}
}

/** @param {unknown} value */
function safeId(value) {
	return isTraceId(value) ? value : createTraceId();
}

/** @param {unknown} status @returns {number | undefined} */
function validStatus(status) {
	return typeof status === 'number' && Number.isInteger(status) && status >= 100 && status <= 599 ? status : undefined;
}

/** @param {{operation?: string, error_code?: string, status_code?: number, duration_ms?: number, trace_id?: string}} details */
export function telemetryEvent(details = /** @type {{operation?: string, error_code?: string, status_code?: number, duration_ms?: number, trace_id?: string}} */ ({})) {
	const { operation, error_code, status_code, duration_ms, trace_id } = details;
	/** @type {{schema_version: number, event_id: string, trace_id: string, source: string, operation: string, error_code: string, duration_ms: number, status_code?: number}} */
	const event = {
		schema_version: 1,
		event_id: createTraceId(),
		trace_id: safeId(trace_id),
		source: 'web',
		operation: operation && OPERATIONS.has(operation) ? operation : 'unknown',
		error_code: error_code && ERROR_CODES.has(error_code) ? error_code : 'internal_error',
		duration_ms: typeof duration_ms === 'number' && Number.isFinite(duration_ms) ? Math.max(0, Math.min(600000, Math.round(duration_ms))) : 0
	};
	const status = validStatus(status_code);
	if (status !== undefined) event.status_code = status;
	return event;
}

export async function flushTelemetry() {
	if (!telemetryEnabled()) {
		queue = [];
		activeController?.abort();
		return;
	}
	if (flushing || !queue.length || Date.now() < retryNotBefore) return;
	flushing = true;
	const item = queue[0];
	if (!item) { flushing = false; return; }
	activeController = typeof AbortController === 'undefined' ? undefined : new AbortController();
	const controller = activeController;
	const timeout = setTimeout(() => controller?.abort(), TIMEOUT_MS);
	let delivered = false;
	try {
		const response = await fetch(API_BASE + API_PATH, {
			method: 'POST',
			headers: { 'content-type': 'application/json', 'x-wam-client': 'web', 'x-wam-trace-id': item.trace_id },
			body: JSON.stringify(item),
			credentials: 'include',
			cache: 'no-store',
			signal: controller?.signal
		});
		if (!response.ok) throw new Error('Telemetry endpoint rejected event');
		delivered = true;
	} catch { /* Diagnostics must never affect the original request or user flow. */ }
	finally {
		clearTimeout(timeout);
		activeController = undefined;
		flushing = false;
		if (delivered) {
			if (queue[0] === item) queue.shift();
			retryNotBefore = 0;
			if (queue.length) void flushTelemetry();
		} else if (queue.includes(item)) {
			retryNotBefore = Date.now() + RETRY_COOLDOWN_MS;
		}
	}
}

/** @param {{operation?: string, error_code?: string, status_code?: number, duration_ms?: number, trace_id?: string}} details */
export function reportTelemetry(details) {
	if (!telemetryEnabled()) return undefined;
	if (queue.length >= MAX_QUEUE) queue.shift();
	const event = telemetryEvent(details);
	queue.push(event);
	void flushTelemetry();
	return event.trace_id;
}

/** @param {string} error_code @param {unknown} reason @param {string} pagePath */
export function reportUnhandledError(error_code = 'unhandled_error', reason, pagePath = '') {
	/** @type {TelemetryContext} */
	let context = {};
	if ((typeof reason === 'object' && reason !== null) || typeof reason === 'function') {
		context = errorContexts.get(reason) ?? {};
		if (!Object.keys(context).length) {
			try {
				const value = /** @type {{operation?: string, traceId?: string, status?: number, duration_ms?: number}} */ (reason);
				context = {
					operation: value.operation,
					trace_id: value.traceId,
					status_code: value.status,
					duration_ms: value.duration_ms
				};
			} catch { /* The rejection value may expose hostile property getters. */ }
		}
	}
	const operation = context.operation && OPERATIONS.has(context.operation)
		? context.operation
		: operationForPagePath(pagePath);
	return reportTelemetry({
		operation,
		error_code,
		status_code: context.status_code,
		duration_ms: context.duration_ms,
		trace_id: context.trace_id
	});
}

// Exposed only to keep tests isolated from browser globals and pending sends.
export function resetTelemetryForTests() {
	activeController?.abort();
	queue = [];
	flushing = false;
	retryNotBefore = 0;
}
