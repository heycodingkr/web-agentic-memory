#!/usr/bin/env node
// Renders demo/video/stage.html into a silent 1080p MP4 plus an English SRT.
//
//   node demo/video/build.mjs --at 3,12,60       PNG stills in /tmp/wam-video/stills (quick layout check)
//   node demo/video/build.mjs                     full render -> demo/out/video/wam-demo-day.mp4 (+ .en.srt, poster)
//   node demo/video/build.mjs --fps 24 --out /tmp/draft.mp4
//
// The stage is a pure function of time, so a render does not depend on machine speed. There is no voice track:
// the captions in stage.html carry the story, and the SRT lets you add a voice-over or a translation later.
// Needs Google Chrome (Playwright's `chrome` channel from ../../web), ffmpeg, and nothing else.
import http from 'node:http';
import { createReadStream, existsSync, mkdirSync, statSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const require = createRequire(path.join(repo, 'web/package.json')); // reuse the web app's Playwright install
const { chromium } = require('@playwright/test');

const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const fps = Number(opt('--fps', '30'));
const stills = opt('--at', '');
const outMp4 = path.resolve(opt('--out', path.join(repo, 'demo/out/video/wam-demo-day.mp4')));
const from = Number(opt('--from', '0'));

const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.txt': 'text/plain', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const rel = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  const file = path.resolve(repo, '.' + rel);
  if (!file.startsWith(repo + path.sep) || !existsSync(file) || !statSync(file).isFile()) { res.writeHead(404); res.end('not found'); return; }
  res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
  createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch({ channel: 'chrome', args: ['--force-device-scale-factor=1', '--disable-lcd-text', '--hide-scrollbars'] });
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
page.on('pageerror', (e) => console.error('page error:', e.message));
page.on('console', (m) => { if (m.type() === 'error') console.error('console:', m.text()); });
await page.goto(`${base}/demo/video/stage.html`);
await page.evaluate(() => window.__init());
await page.waitForFunction(() => window.__ready === true);
const total = await page.evaluate(() => window.__total);
const captions = await page.evaluate(() => window.__captions);

const stamp = (s) => { const ms = Math.round(s * 1000); const p = (n, w = 2) => String(n).padStart(w, '0'); return `${p(Math.floor(ms / 3600000))}:${p(Math.floor(ms / 60000) % 60)}:${p(Math.floor(ms / 1000) % 60)},${p(ms % 1000, 3)}`; };

if (stills) {
  const dir = '/tmp/wam-video/stills';
  mkdirSync(dir, { recursive: true });
  for (const t of stills.split(',').map(Number)) {
    await page.evaluate((x) => window.__render(x), t);
    const file = path.join(dir, `still-${String(t).replace('.', '_')}.png`);
    await page.screenshot({ path: file, type: 'png' });
    console.log(file);
  }
} else {
  mkdirSync(path.dirname(outMp4), { recursive: true });
  const frames = Math.round((total - from) * fps);
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'warning', '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', '-', '-an',
    '-vf', 'scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p', '-c:v', 'libx264', '-preset', 'medium', '-crf', '19',
    '-color_range', 'tv', '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-movflags', '+faststart', outMp4], { stdio: ['pipe', 'inherit', 'inherit'] });
  const closed = new Promise((resolve, reject) => { ff.on('close', (code) => (code === 0 ? resolve() : reject(new Error(`ffmpeg exited with ${code}`)))); ff.stdin.on('error', () => {}); });
  const started = Date.now();
  for (let f = 0; f < frames; f++) {
    await page.evaluate((t) => window.__render(t), from + f / fps);
    const buf = await page.screenshot({ type: 'jpeg', quality: 92 });
    if (!ff.stdin.write(buf)) await new Promise((r) => ff.stdin.once('drain', r));
    if (f % 300 === 0) console.log(`frame ${f}/${frames}  (${Math.round((Date.now() - started) / 1000)} s)`);
  }
  ff.stdin.end();
  await closed;
  // Captions as SRT (times are absolute in the video; shift by --from when rendering a partial range).
  const srt = captions.filter((c) => c[1] > from).map((c, i) => `${i + 1}\n${stamp(Math.max(0, c[0] - from))} --> ${stamp(c[1] - from)}\n${c[2]}\n`).join('\n');
  writeFileSync(outMp4.replace(/\.mp4$/, '.en.srt'), srt);
  await page.evaluate(() => window.__render(104.0));
  await page.screenshot({ path: outMp4.replace(/\.mp4$/, '-poster.jpg'), type: 'jpeg', quality: 92 });
  const probe = spawnSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration,size', '-of', 'default=nw=1', outMp4], { encoding: 'utf8' });
  console.log(`done: ${outMp4}\n${probe.stdout.trim()}`);
}
await browser.close();
server.close();
