import { createTraceId, flushTelemetry, operationForPath, reportTelemetry, resolveTraceId } from './telemetry.js';

export type BioInline =
	| { type: 'text'; text: string }
	| { type: 'strong'; text: string }
	| { type: 'emphasis'; text: string }
	| { type: 'link'; text: string; url: string };

export type BioBlock =
	| { type: 'paragraph'; content: BioInline[] }
	| { type: 'heading'; level: 2 | 3; content: BioInline[] }
	| { type: 'bullet_list'; items: BioInline[][] }
	| { type: 'ordered_list'; items: BioInline[][] };

export interface ServiceLink {
	label: string;
	url: string;
}

export interface Profile {
	schema_version: number;
	handle: string;
	bio: BioBlock[];
	service_links: ServiceLink[];
	public_opt_in: boolean;
}

export interface Leaderboard {
	schema_version: number;
	entries: Array<{ rank: number; handle: string; total_points: number }>;
}

export interface Dashboard {
	schema_version: number;
	total_points: number;
	overall_rank: number | null;
	public_rank: number | null;
	published_memories?: number;
	pending_contributions?: number;
	action_required_contributions?: number;
}

export interface OwnerUsage {
	schema_version: number;
	period_days: number;
	limit_per_minute: number;
	burst_capacity: number;
	remaining_units: number;
	resets_in_seconds: number;
	daily: Array<{
		utc_date: string;
		requests: number;
		units: number;
		recall_requests: number;
		detail_requests: number;
		other_requests: number;
		denied_requests: number;
	}>;
}

export interface MemoryAnalyticsPoint {
	period_start: string;
	anonymous_recall_hits: number;
	other_account_recall_hits: number;
	anonymous_detail_views: number;
	other_account_detail_views: number;
	success_outcomes: number;
	partial_outcomes: number;
	failed_outcomes: number;
}

export interface AuthoredMemoryAnalytics {
	memory_id: string;
	version_id: string;
	title: string;
	site: string;
	points: MemoryAnalyticsPoint[];
}

export interface MemoryAnalytics {
	schema_version: number;
	from_date: string;
	to_date: string;
	bucket: 'day' | 'week';
	memories: AuthoredMemoryAnalytics[];
}

export interface ScoreEvent {
    id: string;
    event_type: string;
    points_delta: number;
    memory_id: string;
    memory_version_id: string;
    created_at: number;
}

export interface ScoreEventList {
    schema_version: number;
    events: ScoreEvent[];
    next_cursor: string | null;
}

export interface CatalogMemory {
	memory_id: string;
	version_id: string;
	title: string;
	summary: string;
	site: string;
	domain: string;
	task_category: string;
	kind: string;
	status: string;
	verified_date: string;
	canonical_url_patterns: string[];
	author_handle?: string | null;
}

export interface MemoryCatalog {
	schema_version: number;
	memories: CatalogMemory[];
}

export interface MemoryDetail {
	schema_version: number;
	memory_id: string;
	version_id: string;
	level: number;
	content: string;
}

export type ContributionAction = 'navigate' | 'edit' | 'save';

export interface Contribution {
	id: string;
	url_pattern: string;
	task_category: string;
	actions: ContributionAction[];
	status: 'queued' | 'in_review' | 'approved' | 'rejected' | 'withdrawn' | 'expired' | 'consent_required';
	submitted_at: number;
	title?: string;
	progress?: { stage: string; updated_at?: number; reason?: string; next_attempt_at?: number };
	reflection?: { kind: 'new' | 'updated' | 'supported'; memory_id: string; version_id?: string; reflected_at?: number; availability?: 'current' | 'superseded' | 'retired' | 'not_current' };
}

export interface ContributionList {
	schema_version: number;
	contributions: Contribution[];
	next_cursor?: string | null;
	total_count?: number;
}

export interface ReviewQueueItem {
	record: Contribution;
	review_generation: number;
}

export interface ReviewQueue {
	schema_version: number;
	counts: {
		queued: number;
		in_review: number;
		approved: number;
		rejected: number;
	};
	queue_health: {
		active_jobs: number;
		oldest_age_secs: number;
		alert: boolean;
	};
	contributions: ReviewQueueItem[];
	decision_history: Array<{
		contribution_id: string;
		decision: 'approved' | 'rejected' | 'verification_requested';
		review_generation: number;
		decided_at: number;
		reviewer: string;
	}>;
}

export interface PublicUsageMetrics {
	schema_version: number;
	recall_requests: number;
	authenticated_recall_requests: number;
	anonymous_recall_requests: number;
	matched_memory_hits: number;
	detail_requests: number;
	success_outcomes: number;
	partial_outcomes: number;
	failed_outcomes: number;
}

export interface ContributionConsent {
	enabled: boolean;
	consent_version: string;
	retention_policy: string;
	retention_days: number;
}

export interface AuthConfig {
	github_oauth_enabled: boolean;
	email_auth_enabled?: boolean;
	mcp_oauth_enabled?: boolean;
	mcp_resource?: string | null;
}

export interface AuthSession {
	account_id: string;
	csrf_token: string;
	expires_at: number;
}

export interface DeviceStart {
	device_code: string;
	user_code: string;
	verification_uri: string;
	expires_in: number;
	interval: number;
}

export interface DeviceApprovalPreview {
	name: string;
	scopes: string[];
	expires_at: number;
}

export interface ConnectedDevice {
	id: string;
	name: string;
	scopes: string[];
	created_at: number;
	expires_at: number;
	last_seen_at: number | null;
	revoked: boolean;
	oauth_managed?: boolean;
}

export interface DeviceList {
	devices: ConnectedDevice[];
}

export interface RotatedDevice {
	access_token: string;
	device: ConnectedDevice;
}

export type DevicePoll =
	| { status: 'pending'; interval: number }
	| { status: 'authorized'; device_id: string; access_token: string; scopes: string[]; expires_at: number }
	| { status: 'denied' | 'expired' | 'already_used' };

const apiBase = (import.meta.env.VITE_WAM_API_URL || '').replace(/\/$/, '');

let csrfToken: string | undefined;

export function apiUrl(path: string): string {
	return apiBase + path;
}

export function setCsrfToken(value?: string): void {
	csrfToken = value || undefined;
}

async function refreshCsrfToken(): Promise<string | undefined> {
	if (csrfToken) return csrfToken;
	const traceId = createTraceId();
	try {
		const response = await fetch(apiUrl('/v1/auth/session'), {
			credentials: 'include',
			cache: 'no-store',
			headers: { 'x-wam-trace-id': traceId, 'x-wam-client': 'web' }
		});
		if (!response.ok) {
			csrfToken = undefined;
			return undefined;
		}
		void flushTelemetry();
		const session = (await response.json()) as AuthSession;
		csrfToken = session.csrf_token;
		return csrfToken;
	} catch {
		return undefined;
	}
}

export class ApiError extends Error {
	constructor(
		message: string,
		public readonly status: number,
		public readonly retryAfterSeconds?: number,
		public readonly traceId?: string
	) {
		super(message);
	}
}

export async function api<T>(path: string, init: RequestInit = {}, token?: string): Promise<T> {
	const headers = new Headers(init.headers);
	const traceId = createTraceId();
	const startedAt = Date.now();
	headers.set('x-wam-trace-id', traceId);
	headers.set('x-wam-client', 'web');
	if (init.body) headers.set('content-type', 'application/json');
	if (token) headers.set('authorization', `Bearer ${token}`);
	const method = (init.method || (init.body ? 'POST' : 'GET')).toUpperCase();
	if (!token && !headers.has('authorization') && ['POST', 'PUT', 'PATCH', 'DELETE'].includes(method)) {
		const csrf = csrfToken || (await refreshCsrfToken());
		if (csrf) headers.set('x-csrf-token', csrf);
	}
	let response: Response;
	try {
		response = await fetch(apiUrl(path), {
			...init,
			headers,
			credentials: init.credentials ?? 'include',
			cache: 'no-store'
		});
	} catch (error) {
		if (!path.startsWith('/v1/telemetry/')) reportTelemetry({ operation: operationForPath(path), error_code: 'network_error', duration_ms: Date.now() - startedAt, trace_id: traceId });
		throw error;
	}
	if (!response.ok) {
		if (response.status === 401 && !token) csrfToken = undefined;
		let retryAfterSeconds: number | undefined;
		if (response.status === 429) {
			const headerValue = Number(response.headers.get('retry-after'));
			if (Number.isFinite(headerValue) && headerValue > 0) retryAfterSeconds = Math.ceil(headerValue);
			if (retryAfterSeconds === undefined) {
				try {
					const body = await response.clone().json() as { retry_after_seconds?: unknown };
					if (typeof body.retry_after_seconds === 'number' && Number.isFinite(body.retry_after_seconds)) {
						retryAfterSeconds = Math.max(1, Math.ceil(body.retry_after_seconds));
					}
				} catch { /* Keep a useful generic message for non-JSON gateways. */ }
			}
		}
		const message = response.status === 401
			? import.meta.env.DEV ? 'Sign in or connect a valid device. For local development, use WAM_DEMO_TOKEN.' : 'Sign in or connect a valid device.'
			:
				response.status === 403 ? 'You do not have the required permission, or the confirmation token expired.' :
				response.status === 404 ? 'The requested item could not be found.' :
						response.status === 409 ? 'The state changed or a value conflicted. Refresh the list and try again.' :
						response.status === 429 ? retryAfterSeconds ? `Rate limit reached. Try again in ${retryAfterSeconds} seconds.` : 'Rate limit reached. Wait a moment and try again.' :
							response.status === 503 ? 'The service is temporarily unavailable. Try again later.' :
								`Server request failed (${response.status})`;
		const headerTraceId = response.headers.get('x-wam-trace-id');
		const responseTraceId = resolveTraceId(headerTraceId, traceId);
		if (!path.startsWith('/v1/telemetry/')) reportTelemetry({ operation: operationForPath(path), error_code: 'http_error', status_code: response.status, duration_ms: Date.now() - startedAt, trace_id: responseTraceId });
		throw new ApiError(`${message} Reference: ${responseTraceId}`, response.status, retryAfterSeconds, responseTraceId);
	}
	void flushTelemetry();
	if (response.status === 204) return undefined as T;
	try {
		return await response.json() as T;
	} catch (error) {
		const headerTraceId = response.headers.get('x-wam-trace-id');
		if (!path.startsWith('/v1/telemetry/')) reportTelemetry({ operation: operationForPath(path), error_code: 'invalid_response', status_code: response.status, duration_ms: Date.now() - startedAt, trace_id: resolveTraceId(headerTraceId, traceId) });
		throw error;
	}
}

export function safeExternalUrl(value: string): boolean {
	try {
		const url = new URL(value);
		const host = url.hostname.toLowerCase();
		return url.protocol === 'https:' && !url.username && !url.password &&
			host.includes('.') && !['localhost', 'local', 'internal', 'test', 'invalid', 'example'].some(
				(suffix) => host === suffix || host.endsWith(`.${suffix}`)
			);
	} catch {
		return false;
	}
}
