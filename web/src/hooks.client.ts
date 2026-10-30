import { flushTelemetry, reportUnhandledError } from './lib/telemetry.js';

if (typeof window !== 'undefined') {
	window.addEventListener('error', () => reportUnhandledError('unhandled_error'));
	window.addEventListener('unhandledrejection', () => reportUnhandledError('unhandled_rejection'));
	window.addEventListener('online', () => { void flushTelemetry(); });
}

export const handleError = ({ error }: { error: unknown }) => {
	const traceId = reportUnhandledError('unhandled_error');
	return { message: traceId ? `Unexpected error. Reference: ${traceId}` : 'Unexpected error' };
};
