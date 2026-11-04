import type { HandleClientError } from '@sveltejs/kit/hooks';
import { flushTelemetry, reportUnhandledError } from './lib/telemetry.js';

if (typeof window !== 'undefined') {
	window.addEventListener('error', (event) => reportUnhandledError('unhandled_error', event.error, window.location.pathname));
	window.addEventListener('unhandledrejection', (event) => reportUnhandledError('unhandled_rejection', event.reason, window.location.pathname));
	window.addEventListener('online', () => { void flushTelemetry(); });
}

export const handleError: HandleClientError = ({ kind, error, event }) => {
	const expected = kind !== 'unknown' && error.status >= 400 && error.status < 500;
	const traceId = reportUnhandledError(
		expected ? 'http_error' : 'unhandled_error', error, event.route.id ?? event.url.pathname
	);
	// Preserve SvelteKit's safe status/message for expected navigation failures.
	if (expected) return;
	return { message: traceId ? `Unexpected error. Reference: ${traceId}` : 'Unexpected error' };
};
