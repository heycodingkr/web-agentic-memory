import type { Contribution, MemoryAnalytics, MemoryAnalyticsPoint, ScoreEvent } from './api';

export type ContributionFilter = 'all' | 'pending' | 'published' | 'supported' | 'action';
export type StatusView = { key: string; label: string; tone: string; description: string; pending: boolean; action: boolean };
const views: Record<string, StatusView> = {
  queued: { key: 'queued', label: 'Queued', tone: 'neutral', description: 'Received and waiting for processing.', pending: true, action: false },
  processing: { key: 'processing', label: 'Checking', tone: 'blue', description: 'The workflow and saved result are being checked.', pending: true, action: false },
  retry_scheduled: { key: 'retry_scheduled', label: 'Retry scheduled', tone: 'amber', description: 'Processing is delayed. We will try again.', pending: true, action: false },
  needs_review: { key: 'needs_review', label: 'Additional review', tone: 'blue', description: 'An additional check is needed. No action is needed from you.', pending: true, action: false },
  consent_required: { key: 'consent_required', label: 'Needs consent', tone: 'coral', description: 'Review your sharing consent before resubmitting.', pending: false, action: true },
  published: { key: 'published', label: 'Published', tone: 'green', description: 'A verified version is available to other agents.', pending: false, action: false },
  supported: { key: 'supported', label: 'Supported', tone: 'neutral', description: 'This contribution supports an existing memory. No new publication points.', pending: false, action: false },
  rejected: { key: 'rejected', label: 'Not accepted', tone: 'coral', description: 'The contribution was not published. You can revise and submit again.', pending: false, action: false },
  withdrawn: { key: 'withdrawn', label: 'Withdrawn', tone: 'neutral', description: 'Your contribution was withdrawn.', pending: false, action: false },
  expired: { key: 'expired', label: 'Expired', tone: 'neutral', description: 'This submission expired. You can submit it again.', pending: false, action: false },
  unknown: { key: 'unknown', label: 'Result unconfirmed', tone: 'neutral', description: 'The review is recorded, but its publication result has not been linked yet.', pending: false, action: false }
};
export function contributionStatus(item: Contribution): StatusView {
  const reflection = item.reflection;
  if (reflection?.memory_id) return views[reflection.kind === 'supported' ? 'supported' : 'published'];
  const stage = item.progress?.stage;
  if (stage && views[stage] && !['published', 'supported'].includes(stage)) return views[stage];
  if (item.status === 'approved') return views.unknown;
  if (item.status === 'in_review') return views.needs_review;
  return views[item.status] || views.unknown;
}
export function contributionDescription(item: Contribution) {
  switch (item.progress?.reason) {
    case 'model_budget': return 'Waiting for the worker’s shared model budget. The retry time is not a completion estimate.';
    case 'temporary_processing_failure': return 'A temporary processing issue delayed this submission. We will retry it automatically.';
    case 'additional_review': return 'An additional check is needed. No action is needed from you.';
    case 'not_accepted': return 'This contribution was not accepted. You can revise the workflow and submit again.';
    default: return contributionStatus(item).description;
  }
}
export function serviceFor(url: string) {
  if (/spreadsheets/.test(url)) return { name: 'Google Sheets', glyph: 'sheet' as const, color: 'sheets' };
  if (/docs\.google/.test(url)) return { name: 'Google Docs', glyph: 'doc' as const, color: 'docs' };
  if (/notion/.test(url)) return { name: 'Notion', glyph: 'folder' as const, color: 'notion' };
  if (/calendar/.test(url)) return { name: 'Google Calendar', glyph: 'calendar' as const, color: 'calendar' };
  if (/mail\.google/.test(url)) return { name: 'Gmail', glyph: 'mail' as const, color: 'mail' };
  try { return { name: new URL(url).hostname, glyph: 'route' as const, color: 'other' }; }
  catch { return { name: url || 'Workflow', glyph: 'route' as const, color: 'other' }; }
}
export function contributionTitle(item: Contribution) {
  return item.title || `${serviceFor(item.url_pattern).name} · ${item.task_category.replaceAll('-', ' ')}`;
}
export function matchesContribution(item: Contribution, filter: ContributionFilter, query: string) {
  const status = contributionStatus(item);
  const selected = filter === 'all' || (filter === 'pending' ? status.pending : filter === 'action' ? status.action : status.key === filter);
  return selected && `${contributionTitle(item)} ${item.url_pattern} ${item.id} ${status.label}`.toLowerCase().includes(query.toLowerCase().trim());
}
export function relativeTime(seconds?: number) {
  if (!seconds) return 'Time not recorded';
  const minutes = Math.max(0, Math.floor((Date.now() / 1000 - seconds) / 60));
  return minutes < 1 ? 'Just now' : minutes < 60 ? `${minutes}m ago` : minutes < 1440 ? `${Math.floor(minutes / 60)}h ago` : `${Math.floor(minutes / 1440)}d ago`;
}
export function metricTotal(memory: MemoryAnalytics['memories'][number], key: keyof Omit<MemoryAnalyticsPoint, 'period_start'>) {
  return memory.points.reduce((sum, point) => sum + point[key], 0);
}
export function impactTotals(data: MemoryAnalytics | null) {
  const memories = data?.memories || [];
  return { recall: memories.reduce((n, m) => n + metricTotal(m, 'other_account_recall_hits'), 0),
    detail: memories.reduce((n, m) => n + metricTotal(m, 'other_account_detail_views'), 0),
    success: memories.reduce((n, m) => n + metricTotal(m, 'success_outcomes'), 0),
    anonymous: memories.reduce((n, m) => n + metricTotal(m, 'anonymous_recall_hits') + metricTotal(m, 'anonymous_detail_views'), 0) };
}
export function pointReason(event: ScoreEvent) {
  if (event.points_delta < 0) return 'Credit reversed';
  if (event.event_type === 'successful_use') return 'Successful use by another account';
  if (['verified_publication', 'published_version', 'publication_approved'].includes(event.event_type)) return 'Memory published';
  return 'Point adjustment';
}
