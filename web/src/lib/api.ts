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
	status: 'queued' | 'in_review' | 'approved' | 'rejected';
	submitted_at: number;
}

export interface ContributionList {
	schema_version: number;
	contributions: Contribution[];
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
		decision: 'approved' | 'rejected';
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

export interface AuthConfig {
	github_oauth_enabled: boolean;
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
	try {
		const response = await fetch(apiUrl('/v1/auth/session'), {
			credentials: 'include',
			cache: 'no-store'
		});
		if (!response.ok) {
			csrfToken = undefined;
			return undefined;
		}
		const session = (await response.json()) as AuthSession;
		csrfToken = session.csrf_token;
		return csrfToken;
	} catch {
		return undefined;
	}
}

export class ApiError extends Error {
	constructor(message: string, public readonly status: number) {
		super(message);
	}
}

export async function api<T>(path: string, init: RequestInit = {}, token?: string): Promise<T> {
	const headers = new Headers(init.headers);
	if (init.body) headers.set('content-type', 'application/json');
	if (token) headers.set('authorization', `Bearer ${token}`);
	const method = (init.method || (init.body ? 'POST' : 'GET')).toUpperCase();
	if (!token && !headers.has('authorization') && ['POST', 'PUT', 'PATCH', 'DELETE'].includes(method)) {
		const csrf = csrfToken || (await refreshCsrfToken());
		if (csrf) headers.set('x-csrf-token', csrf);
	}
	const response = await fetch(apiUrl(path), {
		...init,
		headers,
		credentials: init.credentials ?? 'include'
	});
	if (!response.ok) {
		if (response.status === 401 && !token) csrfToken = undefined;
		throw new ApiError(
			response.status === 401 ? 'Sign in or connect a valid device. For local development, use WAM_DEMO_TOKEN.' :
				response.status === 403 ? 'You do not have the required permission, or the confirmation token expired.' :
				response.status === 404 ? 'The requested item could not be found.' :
						response.status === 409 ? 'The state changed or a value conflicted. Refresh the list and try again.' :
						response.status === 429 ? 'Too many requests. Wait a moment and try again.' :
							response.status === 503 ? 'The authentication provider or local API is not configured yet.' :
								`Server request failed (${response.status})`,
			response.status
		);
	}
	if (response.status === 204) return undefined as T;
	return response.json() as Promise<T>;
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
