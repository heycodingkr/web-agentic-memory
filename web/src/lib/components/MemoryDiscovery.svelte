<script lang="ts">
	import { librarySearchHref } from '#lib/memories';
	import SiteGlyph from '#lib/components/SiteGlyph.svelte';

	let { lang = 'en' }: { lang?: 'en' | 'ko' } = $props();
	let query = $state('');
	const korean = $derived(lang === 'ko');
	const sites = [
		{ name: 'Google Docs', url: 'docs.google.com/document/', glyph: 'doc' },
		{ name: 'Google Sheets', url: 'docs.google.com/spreadsheets/', glyph: 'sheet' },
		{ name: 'Gmail', url: 'mail.google.com', glyph: 'mail' },
		{ name: 'Calendar', url: 'calendar.google.com', glyph: 'calendar' },
		{ name: 'Notion', url: 'notion.so', glyph: 'draft' }
	] as const;

	function search(event: SubmitEvent) {
		event.preventDefault();
		window.location.assign(librarySearchHref(query));
	}
</script>

<section class="discovery" id="discover" aria-labelledby="discover-title">
	<div class="container discovery-grid">
		<div class="discovery-copy">
			<span class="section-label">{korean ? '사이트에서 시작해 보세요' : 'Start with a website'}</span>
			<h2 id="discover-title">{korean ? '이 사이트에서,\n무엇을 할 수 있을까요?' : 'Your website.\nYour agent’s next move.'}</h2>
			<p>{korean ? 'URL을 넣고 그 사이트에 맞는 메모리를 찾아보세요. 문서 만들기부터 시트 정리, 메일 작성까지. 에이전트에 연결하기 전에 작업 방법을 먼저 살펴볼 수 있습니다.' : 'Enter a URL to find workflows for that site. Create documents, organize a sheet, or draft an email. Read the steps before connecting your agent.'}</p>
			<a class="text-link" href="/memories">{korean ? '전체 메모리 둘러보기' : 'Browse all memories'} <span aria-hidden="true">→</span></a>
		</div>
		<div class="discovery-panel">
			<form action="/memories" onsubmit={search} role="search" aria-label={korean ? '사이트 메모리 찾기' : 'Find website memories'}>
				<label for="discovery-url">{korean ? '어느 사이트에서 작업하시나요?' : 'Where are you working?'}</label>
				<div class="discovery-input">
					<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.4" stroke="currentColor" stroke-width="1.7" /><path d="m15.5 15.5 4.2 4.2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg>
					<input id="discovery-url" type="search" bind:value={query} placeholder={korean ? 'URL, 사이트 이름 또는 하고 싶은 작업' : 'A URL, website, or task you have in mind'} autocomplete="off" spellcheck="false" />
					<button class="button button-dark" type="submit">{korean ? '메모리 찾기' : 'Find workflows'} <span aria-hidden="true">→</span></button>
				</div>
			</form>
			<div class="discovery-sites" aria-label={korean ? '사이트별 메모리' : 'Memories by website'}>
				{#each sites as site}
					<a href={librarySearchHref(site.url)}><SiteGlyph kind={site.glyph} />{site.name}</a>
				{/each}
			</div>
			<div class="discovery-preview">
				<span>{korean ? '어떤 작업이 있나요?' : 'What could you try?'}</span>
				<a href={librarySearchHref('docs.google.com/document/')}><span class="preview-icon"><SiteGlyph kind="doc" /></span><span><b>{korean ? '회의록 · 신청서 · 제안서' : 'Meeting notes, forms & proposals'}</b><small>Google Docs</small></span><span aria-hidden="true">↗</span></a>
				<a href={librarySearchHref('docs.google.com/spreadsheets/')}><span class="preview-icon"><SiteGlyph kind="sheet" /></span><span><b>{korean ? '업무 일지 · 지출 내역 정리' : 'Work logs & expense trackers'}</b><small>Google Sheets</small></span><span aria-hidden="true">↗</span></a>
				<a href={librarySearchHref('mail.google.com')}><span class="preview-icon"><SiteGlyph kind="mail" /></span><span><b>{korean ? '메일 작성 · 전송 전 확인' : 'Compose emails & check before sending'}</b><small>Gmail</small></span><span aria-hidden="true">↗</span></a>
			</div>
			<p class="discovery-note"><span aria-hidden="true">↳</span>{korean ? '로그인 없이 탐색 · 검색어와 입력한 URL은 이 기기에서만 처리' : 'Explore without signing in · Your search stays on this device'}</p>
		</div>
	</div>
</section>

<style>
	.discovery { padding: clamp(36px, 5vw, 64px) 0; border-top: 1px solid #dce3d4; border-bottom: 1px solid #dce3d4; background: #edf2e5; scroll-margin-top: 86px; }
	.discovery-grid { display: grid; grid-template-columns: minmax(0, .85fr) minmax(0, 1.15fr); align-items: center; gap: clamp(28px, 5vw, 76px); }
	.discovery-copy h2 { margin: 14px 0 18px; white-space: pre-line; color: #1d3c2d; font: 800 clamp(28px, 3.1vw, 42px)/1.18 Manrope, 'Apple SD Gothic Neo', sans-serif; letter-spacing: -.045em; text-wrap: balance; }
	.discovery-copy p { max-width: 430px; margin: 0 0 22px; color: #4b6252; font-size: 15px; line-height: 1.8; word-break: keep-all; }
	.discovery-panel { min-width: 0; padding: 26px; border: 1px solid #d4dfca; border-radius: 18px; background: #fffef9; box-shadow: 0 12px 30px #29432108; }
	form label { display: block; margin-bottom: 11px; color: #2b4937; font-size: 14px; font-weight: 700; }
	.discovery-input { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; padding: 6px; border: 1px solid #9cac8e; border-radius: 10px; background: white; }
	.discovery-input:focus-within { outline: 2px solid #54783c; outline-offset: 3px; }
	.discovery-input > svg { flex: none; width: 20px; height: 20px; margin-left: 7px; color: #53694c; }
	input { flex: 1; min-width: 80px; width: 0; padding: 10px 0; border: 0; outline: 0; color: #203d2e; background: transparent; font: 13px 'DM Sans', sans-serif; }
	input::placeholder { color: #63745d; opacity: 1; }
	.discovery-input .button { min-height: 42px; padding: 10px 14px; border-radius: 6px; font-size: 12px; white-space: nowrap; }
	.discovery-sites { display: flex; flex-wrap: wrap; gap: 7px; margin: 14px 0 22px; }
	.discovery-sites a { display: inline-flex; align-items: center; gap: 5px; padding: 6px 9px; border: 1px solid #dde5d6; border-radius: 6px; color: #3d5c43; background: #f4f7ef; font-size: 11px; font-weight: 600; }
	.discovery-sites :global(svg) { width: 14px; height: 14px; }
	.discovery-preview { display: grid; gap: 2px; padding-top: 18px; border-top: 1px solid #e3e8dc; }
	.discovery-preview > span { margin-bottom: 6px; color: #64725c; font-size: 11px; }
	.discovery-preview a { display: flex; align-items: center; gap: 12px; min-width: 0; padding: 9px 4px; border-radius: 6px; }
	.preview-icon { display: grid; flex: none; place-items: center; width: 34px; height: 38px; border: 1px solid #dce5d5; border-radius: 5px; color: #527747; background: #f1f5e9; }
	.preview-icon :global(svg) { width: 19px; height: 19px; }
	.discovery-preview a > span:nth-child(2) { flex: 1; min-width: 0; }
	.discovery-preview b { display: block; color: #2a4935; font-size: 13px; font-weight: 600; line-height: 1.5; }
	.discovery-preview small { color: #64725c; font-size: 10px; }
	.discovery-preview a > span:last-child { margin-right: 4px; color: #547746; }
	.discovery-sites a:hover, .discovery-preview a:hover { background: #e8efdc; }
	a:focus-visible, button:focus-visible { outline: 2px solid #54783c; outline-offset: 3px; }
	.discovery-note { display: flex; gap: 6px; margin: 18px 0 0; color: #5c6f55; font-size: 11px; line-height: 1.6; }
	@media (max-width: 900px) { .discovery-grid { grid-template-columns: 1fr; gap: 24px; } .discovery-copy p { max-width: 620px; } .discovery-copy h2 { white-space: normal; } }
	@media (max-width: 520px) { .discovery-panel { padding: 18px; } .discovery-input .button { width: 100%; } .discovery-input input { min-width: 150px; } .discovery-copy h2 { font-size: 29px; } }
</style>
