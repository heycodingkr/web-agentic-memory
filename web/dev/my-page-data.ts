// Synthetic UI fixtures. Imported by the opt-in Vite dev middleware, never by application code.
export function previewData() {
  const now = Math.floor(Date.now() / 1000);
  const ago = (seconds: number) => now - seconds;
  const contributions = [
    { id: 'sample-docs-01', title: 'Format a long report in Google Docs', url_pattern: 'https://docs.google.com/document/*', task_category: 'compose-draft', actions: ['edit', 'save'], status: 'approved', submitted_at: ago(86400), progress: { stage: 'published', updated_at: ago(7200) }, reflection: { kind: 'new', memory_id: 'sample-docs-report', version_id: 'v3', reflected_at: ago(7200), availability: 'current' } },
    { id: 'sample-sheets-02', title: 'Clean duplicate rows in a spreadsheet', url_pattern: 'https://docs.google.com/spreadsheets/*', task_category: 'research', actions: ['edit', 'save'], status: 'queued', submitted_at: ago(210), progress: { stage: 'processing', updated_at: ago(120) } },
    { id: 'sample-notion-03', title: 'Organize a Notion project database', url_pattern: 'https://www.notion.so/*', task_category: 'navigate', actions: ['navigate', 'save'], status: 'queued', submitted_at: ago(3000), progress: { stage: 'retry_scheduled', updated_at: ago(420), reason: 'model_budget', next_attempt_at: now + 180 } },
    { id: 'sample-calendar-04', title: 'Create a recurring calendar event', url_pattern: 'https://calendar.google.com/*', task_category: 'navigate', actions: ['navigate', 'save'], status: 'consent_required', submitted_at: ago(6000), progress: { stage: 'consent_required', updated_at: ago(900) } },
    { id: 'sample-gmail-05', title: 'Schedule a follow-up email', url_pattern: 'https://mail.google.com/*', task_category: 'compose-draft', actions: ['edit', 'save'], status: 'in_review', submitted_at: ago(10000), progress: { stage: 'needs_review', updated_at: ago(1800), reason: 'additional_review' } },
    { id: 'sample-docs-06', title: 'Keep document headings consistent', url_pattern: 'https://docs.google.com/document/*', task_category: 'compose-draft', actions: ['edit'], status: 'approved', submitted_at: ago(18000), progress: { stage: 'supported', updated_at: ago(3600) }, reflection: { kind: 'supported', memory_id: 'sample-docs-headings', version_id: 'v2', reflected_at: ago(3600), availability: 'current' } },
    { id: 'sample-sheets-07', title: 'Build a monthly expense summary', url_pattern: 'https://docs.google.com/spreadsheets/*', task_category: 'research', actions: ['edit', 'save'], status: 'approved', submitted_at: ago(172800), progress: { stage: 'published', updated_at: ago(86400) }, reflection: { kind: 'updated', memory_id: 'sample-sheets-summary', version_id: 'v2', reflected_at: ago(86400), availability: 'current' } },
    { id: 'sample-notion-08', title: 'Share a project status page', url_pattern: 'https://www.notion.so/*', task_category: 'navigate', actions: ['navigate'], status: 'rejected', submitted_at: ago(20000), progress: { stage: 'rejected', updated_at: ago(3600), reason: 'not_accepted' } },
    { id: 'sample-mail-09', title: 'Group newsletters with a mail filter', url_pattern: 'https://mail.google.com/*', task_category: 'navigate', actions: ['navigate', 'save'], status: 'queued', submitted_at: ago(60), progress: { stage: 'queued', updated_at: ago(60) } }
  ];
  const today = new Date(); today.setUTCHours(0, 0, 0, 0);
  const dates = Array.from({ length: 30 }, (_, index) => new Date(today.getTime() - (29 - index) * 86400000).toISOString().slice(0, 10));
  const memories = [
    { memory_id: 'sample-docs-report', version_id: 'v3', title: 'Format a long report in Google Docs', site: 'docs.google.com' },
    { memory_id: 'sample-sheets-summary', version_id: 'v2', title: 'Build a monthly expense summary', site: 'docs.google.com' },
    { memory_id: 'sample-notion-database', version_id: 'v1', title: 'Organize a Notion project database', site: 'notion.so' }
  ].map((memory, m) => ({ ...memory, points: dates.map((date, index) => ({ period_start: date, other_account_recall_hits: index < 4 ? 0 : (index * 7 + m * 3) % 12 + 1, anonymous_recall_hits: (index + m) % 5, other_account_detail_views: (index * 3 + m) % 4, anonymous_detail_views: index % 3, success_outcomes: index % 6 === m ? 1 : 0, partial_outcomes: index === 17 ? 1 : 0, failed_outcomes: 0 })) }));
  return {
    '/v1/auth/config': { email_auth_enabled: true, github_oauth_enabled: false, mcp_oauth_enabled: true },
    '/v1/auth/session': { account_id: 'email_sample_creator', csrf_token: 'sample-csrf-only', expires_at: now + 43200 },
    '/v1/me/profile': { schema_version: 1, handle: 'alex_park', bio: [], service_links: [], public_opt_in: false },
    '/v1/me/dashboard': { schema_version: 1, total_points: 246, overall_rank: 18, public_rank: null, published_memories: 3, pending_contributions: 4, action_required_contributions: 1 },
    '/v1/me/contributions': { schema_version: 1, contributions },
    '/v1/me/memories/analytics': { schema_version: 1, from_date: dates[0], to_date: dates.at(-1), bucket: 'day', memories },
    '/v1/me/score-events': { schema_version: 1, events: [
      { id: 'sample-credit-1', event_type: 'verified_publication', points_delta: 20, memory_id: 'sample-docs-report', memory_version_id: 'v3', created_at: ago(7200) },
      { id: 'sample-credit-2', event_type: 'successful_use', points_delta: 2, memory_id: 'sample-sheets-summary', memory_version_id: 'v2', created_at: ago(12000) },
      { id: 'sample-credit-3', event_type: 'verified_publication', points_delta: 20, memory_id: 'sample-sheets-summary', memory_version_id: 'v2', created_at: ago(86400) },
      { id: 'sample-credit-4', event_type: 'reversal', points_delta: -2, memory_id: 'sample-notion-database', memory_version_id: 'v1', created_at: ago(172800) }
    ], next_cursor: 'sample-older' },
    '/v1/me/usage': { schema_version: 1, period_days: 30, limit_per_minute: 300, burst_capacity: 60, remaining_units: 48, resets_in_seconds: 3, daily: dates.map((date, index) => ({ utc_date: date, requests: index % 11 + 3, units: index % 17 + 4, recall_requests: index % 5, detail_requests: index % 3, other_requests: 2, denied_requests: 0 })) }
  } as Record<string, any>;
}
