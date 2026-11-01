import type { Plugin } from 'vite';
import { previewData } from './my-page-data.ts';

// Enabled only on an explicitly started local preview server. No production routes or auth bypass.
export function myPagePreview(): Plugin {
  return { name: 'wam-my-page-local-preview', apply: 'serve', configureServer(server) {
    server.middlewares.use((req, res, next) => {
      const request = req as typeof req & { url?: string; method?: string; headers: Record<string, string | string[] | undefined> };
      const url = new URL(request.url || '/', 'http://127.0.0.1');
      if (url.pathname === '/__preview/reset') { res.writeHead(303, { Location: '/dashboard', 'Set-Cookie': 'wam_ui_signed_out=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly' }); res.end(); return; }
      if (!url.pathname.startsWith('/__preview/v1/')) return next();
      const path = url.pathname.replace('/__preview', '');
      const mode = String(request.headers['x-wam-preview-case'] || 'populated');
      const data = previewData();
      let status = 200;
      let body: any = data[path];
      if (path === '/v1/auth/logout' && request.method === 'POST') { res.writeHead(204, { 'Set-Cookie': 'wam_ui_signed_out=1; Path=/; SameSite=Lax; HttpOnly', 'Cache-Control': 'no-store' }); res.end(); return; }
      if (String(request.headers.cookie || '').includes('wam_ui_signed_out=1') && path !== '/v1/auth/config') { status = 401; body = { error: 'authentication_required' }; }
      else if (path.startsWith('/v1/telemetry/')) { status = 202; body = {}; }
      else if (mode === 'signed-out' && path !== '/v1/auth/config') { status = 401; body = { error: 'authentication_required' }; }
      else if (mode === 'error' && path === '/v1/me/memories/analytics') { status = 503; body = { error: 'service_unavailable' }; }
      else if (path.startsWith('/v1/me/contributions/')) {
        const id = decodeURIComponent(path.split('/').at(-1)!);
        body = data['/v1/me/contributions'].contributions.find((item: any) => item.id === id);
      }
      if (path === '/v1/me/score-events' && url.searchParams.has('before') && status === 200) body = { schema_version: 1, events: Array.from({ length: 13 }, (_, index) => ({ id: 'sample-older-' + index, event_type: index < 10 ? 'verified_publication' : 'successful_use', points_delta: index < 10 ? 20 : 2, memory_id: ['sample-docs-report', 'sample-sheets-summary', 'sample-notion-database'][index % 3], memory_version_id: 'earlier-v' + (index + 1), created_at: Math.floor(Date.now() / 1000) - (index + 31) * 86400 })), next_cursor: null };
      if (path === '/v1/me/memories/analytics' && body?.memories && status === 200) {
        const from = url.searchParams.get('from') || body.from_date;
        const to = url.searchParams.get('to') || body.to_date;
        const bucket = url.searchParams.get('bucket') || 'day';
        body = { ...body, from_date: from, to_date: to, bucket, memories: body.memories.map((memory: any) => {
          const points = memory.points.filter((p: any) => p.period_start >= from && p.period_start <= to);
          if (bucket !== 'week') return { ...memory, points };
          const weeks = new Map<string, any>();
          for (const point of points) { const day = new Date(point.period_start + 'T00:00:00Z'); day.setUTCDate(day.getUTCDate() - (day.getUTCDay() + 6) % 7); const date = day.toISOString().slice(0, 10); const current = weeks.get(date) || Object.fromEntries(Object.keys(point).map(key => [key, key === 'period_start' ? date : 0])); for (const key of Object.keys(point)) if (key !== 'period_start') current[key] += point[key]; weeks.set(date, current); }
          return { ...memory, points: [...weeks.values()] };
        }) };
      }
      if (mode === 'empty' && status === 200) {
        if (path === '/v1/me/dashboard') body = { schema_version: 1, total_points: 0, overall_rank: null, public_rank: null, published_memories: 0, pending_contributions: 0, action_required_contributions: 0 };
        if (path === '/v1/me/contributions') body = { schema_version: 1, contributions: [] };
        if (path === '/v1/me/memories/analytics') body = { ...body, memories: [] };
        if (path === '/v1/me/score-events') body = { schema_version: 1, events: [], next_cursor: null };
      }
      if (!body) { status = 404; body = { error: 'not_found' }; }
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', 'X-WAM-Sample-Data': 'true' });
      res.end(JSON.stringify(body));
    });
  } };
}
