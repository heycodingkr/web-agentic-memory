import type { Page, Route } from '@playwright/test';

export const impact = (title = 'Private authored workflow') => ({
	schema_version: 1, from_date: '2026-10-01', to_date: '2026-10-29', bucket: 'day',
	memories: [{ memory_id: 'public-memory', version_id: 'v1', title, site: 'example.org', points: [{
		period_start: '2026-10-29', anonymous_recall_hits: 2, other_account_recall_hits: 3,
		anonymous_detail_views: 1, other_account_detail_views: 1, success_outcomes: 1,
		partial_outcomes: 0, failed_outcomes: 0
	}] }]
});

export async function mockDashboard(page: Page, override?: (path: string, route: Route) => Promise<boolean>) {
	const calls: string[] = [];
	await page.route('**/v1/**', async route => {
		const path = new URL(route.request().url()).pathname;
		calls.push(path);
		if (await override?.(path, route)) return;
		const body: Record<string, unknown> = {
			'/v1/auth/config': { github_oauth_enabled: true },
			'/v1/auth/session': { csrf_token: 'synthetic-csrf' },
			'/v1/me/dashboard': { schema_version: 1, total_points: 342, overall_rank: 2, public_rank: 1 },
			'/v1/me/usage': { schema_version: 1, period_days: 30, limit_per_minute: 100,
				burst_capacity: 100, remaining_units: 5, resets_in_seconds: 12, daily: [] },
			'/v1/me/contributions': { schema_version: 1, contributions: [] },
			'/v1/me/score-events': { schema_version: 1, events: [{ id: 'score-1', event_type: 'verified_publication',
				points_delta: 20, memory_id: 'public-memory', memory_version_id: 'v1', created_at: 1_798_000_000 }], next_cursor: null },
			'/v1/me/memories/analytics': impact()
		};
		await route.fulfill({ json: body[path] ?? {} });
	});
	return calls;
}
