#!/usr/bin/env node
// Renders demo/video/stage.html into a silent 1080p MP4 plus an English SRT.
//
//   node demo/video/build.mjs --at 3,12,60       PNG stills in /tmp/wam-video/stills (quick layout check; no real footage)
//   node demo/video/build.mjs                     full render -> demo/out/video/wam-demo-day.mp4 (+ .en.srt, poster)
//   node demo/video/build.mjs --from 44 --to 62   render only that range (checks the real-footage overlays quickly)
//   node demo/video/build.mjs --fps 24 --out /tmp/draft.mp4
//
// The stage is a pure function of time, so a render does not depend on machine speed. There is no voice track:
// the captions in stage.html carry the story, and the SRT lets you add a voice-over or a translation later.
// Real footage (the Arise runs in demo/out/slides-real/) is composited in a second ffmpeg pass: stage.html lists
// each clip in window.__overlays (source window, speed, crop, privacy masks, destination box, time range).
// Needs Google Chrome (Playwright's `chrome` channel from ../../web), ffmpeg, and nothing else.
import http from 'node:http';
import { createReadStream, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { overlayOutputLimitArgs, posterTimeForRange } from './render-options.mjs';

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
page.on('console', (m) => { if (m.type() === 'error' && !/Failed to load resource/.test(m.text())) console.error('console:', m.text()); });
page.on('requestfailed', (r) => console.error('request failed:', r.url()));
page.on('response', (r) => { if (r.status() >= 400 && !r.url().endsWith('/favicon.ico')) console.error(`HTTP ${r.status()}:`, r.url()); });
await page.goto(`${base}/demo/video/stage.html`);
await page.evaluate(() => window.__init());
await page.waitForFunction(() => window.__ready === true);
const total = await page.evaluate(() => window.__total);
const to = Number(opt('--to', String(total)));
const captions = await page.evaluate(() => window.__captions);
const overlays = await page.evaluate(() => window.__overlays || []);
const posterAt = await page.evaluate(() => window.__poster || 104);

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
  const used = overlays.filter((o) => o.to > from && o.from < to);
  for (const o of used) if (!existsSync(path.resolve(repo, 'demo/out', o.file))) throw new Error(`overlay clip missing: demo/out/${o.file}`);
  const stageOut = used.length ? outMp4.replace(/\.mp4$/, '.stage.mp4') : outMp4;
  const frames = Math.round((to - from) * fps);
  const quality = used.length ? ['-preset', 'veryfast', '-crf', '14'] : ['-preset', 'medium', '-crf', '19'];
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'warning', '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', '-', '-an',
    '-vf', 'scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p', '-c:v', 'libx264', ...quality,
    '-color_range', 'tv', '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-movflags', '+faststart', stageOut], { stdio: ['pipe', 'inherit', 'inherit'] });
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

  if (used.length) {
    // Second pass: put the real clips into their holes. Times are relative to the rendered range.
    const inputs = ['-i', stageOut];
    const chains = [];
    let last = '0:v';
    used.forEach((o, i) => {
      inputs.push('-i', path.resolve(repo, 'demo/out', o.file));
      const speed = o.speed || 1, src = o.src || {}, a = o.from - from, b = o.to - from;
      const f = [];
      if (src.from != null || src.to != null) f.push(`trim=start=${src.from ?? 0}${src.to != null ? `:end=${src.to}` : ''}`);
      f.push(`setpts=(PTS-STARTPTS)/${speed}+${a}/TB`);
      if (o.crop) f.push(`crop=${o.crop.w}:${o.crop.h}:${o.crop.x}:${o.crop.y}`);
      (o.mask || []).forEach((m) => f.push(`drawbox=x=${m.x}:y=${m.y}:w=${m.w}:h=${m.h}:color=${m.color || 'white'}:t=fill`));
      f.push(`scale=${o.w}:${o.h}:flags=lanczos`, `fps=${fps}`, 'format=yuv420p');
      chains.push(`[${i + 1}:v]${f.join(',')}[ov${i}]`);
      chains.push(`[${last}][ov${i}]overlay=${o.x}:${o.y}:eof_action=${o.hold ? 'repeat' : 'pass'}:enable='between(t,${a},${b})'[b${i}]`);
      last = `b${i}`;
    });
    const r = spawnSync('ffmpeg', ['-y', '-loglevel', 'warning', ...inputs, '-filter_complex', chains.join(';'), '-map', `[${last}]`, '-an',
      '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p', '-color_range', 'tv', '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709',
      '-movflags', '+faststart', ...overlayOutputLimitArgs(frames), outMp4], { stdio: 'inherit' });
    if (r.status !== 0) throw new Error('overlay pass failed');
    rmSync(stageOut, { force: true });
    console.log(`composited ${used.length} real clip(s): ${used.map((o) => o.id).join(', ')}`);
  }
  // Captions as SRT (times are absolute in the video; shift by --from when rendering a partial range).
  const srt = captions.filter((c) => c[1] > from && c[0] < to).map((c, i) => `${i + 1}\n${stamp(Math.max(0, c[0] - from))} --> ${stamp(Math.min(c[1], to) - from)}\n${c[2]}\n`).join('\n');
  writeFileSync(outMp4.replace(/\.mp4$/, '.en.srt'), srt);
  const posterTime = posterTimeForRange(posterAt, from, to);
  const posterOffset = (posterTime - from).toFixed(3);
  const posterPath = outMp4.replace(/\.mp4$/, '-poster.jpg');
  const poster = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-ss', posterOffset, '-i', outMp4,
    '-frames:v', '1', '-q:v', '2', posterPath], { stdio: 'inherit' });
  if (poster.status !== 0) throw new Error(`poster extraction failed at ${posterOffset}s`);
  const probe = spawnSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration,size', '-of', 'default=nw=1', outMp4], { encoding: 'utf8' });
  console.log(`done: ${outMp4}\n${probe.stdout.trim()}`);
}
await browser.close();
server.close();
