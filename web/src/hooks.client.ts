import { flushTelemetry, reportUnhandledError } from './lib/telemetry.js';

if (typeof window !== 'undefined') {
	window.addEventListener('error', (event) => reportUnhandledError('unhandled_error', event.error, window.location.pathname));
	window.addEventListener('unhandledrejection', (event) => reportUnhandledError('unhandled_rejection', event.reason, window.location.pathname));
	window.addEventListener('online', () => { void flushTelemetry(); });
}

export const handleError = ({ error }: { error: unknown }) => {
	const pagePath = typeof window === 'undefined' ? '' : window.location.pathname;
	const traceId = reportUnhandledError('unhandled_error', error, pagePath);
	return { message: traceId ? `Unexpected error. Reference: ${traceId}` : 'Unexpected error' };
};
