<script lang="ts">
	import { onMount } from 'svelte';
	import DemoVideo from '#lib/components/DemoVideo.svelte';
	import InstallCode from '#lib/components/InstallCode.svelte';
	import SiteGlyph from '#lib/components/SiteGlyph.svelte';
	import { museInstallCommand, museSettingsJson, remoteInstallCommand } from '#lib/install';
	import { githubRepositoryUrl, publicMcpEndpoint } from '#lib/site.js';
	import type { LandingCopy } from '#lib/landing/copy';
	import { change, hosts, iframeErrors, percent, pilotForLanguage, pilotRows, pilotSiteUrls, productivityPilot, recallExample, recallLimits } from '#lib/landing';

	// The whole landing page, in the language of `copy` (src/lib/landing/en.ts, ko.ts). Numbers come from #lib/landing.
	let { copy }: { copy: LandingCopy } = $props();

	const site = String(import.meta.env.VITE_SITE_URL ?? '').replace(/\/$/, '');
	const ogImage = $derived(`${site}${copy.ogImage}`);
	const pilot = $derived(pilotForLanguage(copy.lang));
	const longest = $derived(Math.max(...pilot.rows.map((row) => row.secondsBefore)));
	const medium = pilotRows[0];
	const workflowGlyph = { slides: 'slides', docs: 'doc', blog: 'draft', sheets: 'sheet', email: 'mail', forms: 'form' } as const;
	// Cards that also have a side-by-side clip (without memory next to with hints): which of them is showing it.
	let sideBySide = $state<Record<string, boolean>>({});

	const guidePath = $derived(copy.lang === 'ko' ? '/ko/guide' : '/guide');

	// Quick install: the same commands the setup guide generates, for the public service. Muse Code first.
	const quickHosts = [{ id: 'muse', name: 'Muse Code' }, { id: 'claude-code', name: 'Claude Code' }, { id: 'codex', name: 'Codex' }] as const;
	let quickHost = $state<'muse' | 'claude-code' | 'codex'>('muse');
	// Tabs stay disabled until the page can handle the click, so an early click is never swallowed.
	let ready = $state(false);
	onMount(() => { ready = true; });

	// Bars start empty only once JavaScript is running and can also fill them, so nothing is hidden without it.
	function reveal(node: HTMLElement) {
		node.classList.add('armed');
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
			node.classList.add('in-view');
			return;
		}
		const watcher = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting)) {
					node.classList.add('in-view');
					watcher.disconnect();
				}
			},
			{ threshold: 0.2 }
		);
		watcher.observe(node);
		return { destroy: () => watcher.disconnect() };
	}
</script>

<svelte:head>
	<title>{copy.meta.title}</title>
	<meta name="description" content={copy.meta.description} />
	<meta property="og:type" content="website" />
	<meta property="og:locale" content={copy.lang === 'ko' ? 'ko_KR' : 'en_US'} />
	<meta property="og:title" content={copy.meta.title} />
	<meta property="og:description" content={copy.meta.description} />
	<meta property="og:image" content={ogImage} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:image" content={ogImage} />
	{#if site}
		<link rel="alternate" hreflang="en" href={`${site}/`} />
		<link rel="alternate" hreflang="ko" href={`${site}/ko`} />
		<link rel="alternate" hreflang="x-default" href={`${site}/`} />
	{/if}
</svelte:head>

<div class="lp" lang={copy.lang}>
<section class="lp-hero" aria-labelledby="hero-title">
	<nav class="lp-lang" aria-label={copy.switcher.label}>
		<a href="/" hreflang="en" lang="en" aria-current={copy.lang === 'en' ? 'page' : undefined}>{copy.switcher.en}</a>
		<span aria-hidden="true">·</span>
		<a href="/ko" hreflang="ko" lang="ko" aria-current={copy.lang === 'ko' ? 'page' : undefined}>{copy.switcher.ko}</a>
	</nav>
	<div class="container lp-hero-grid">
		<div class="lp-hero-copy">
			<span class="lp-eyebrow">{copy.hero.eyebrow}</span>
			<h1 id="hero-title">{copy.hero.titleBefore}<em>{copy.hero.titleEm}</em>{copy.hero.titleAfter}</h1>
			<p class="lp-lede">{copy.hero.lede}</p>
			<div class="lp-actions">
				<a class="button button-dark" href={guidePath}>{copy.hero.connect}</a>
				<a class="button button-light" href="#workflows">
					<svg class="lp-play" viewBox="0 0 20 20" aria-hidden="true"><path d="M6 4.2v11.6a.8.8 0 0 0 1.2.7l9-5.8a.8.8 0 0 0 0-1.4l-9-5.8A.8.8 0 0 0 6 4.2z" /></svg>
					{copy.hero.watch}
				</a>
			</div>
			<p class="lp-availability">{copy.hero.availability}</p>
			<div class="lp-resources">
				<a class="text-link" href={githubRepositoryUrl} target="_blank" rel="noopener noreferrer">{copy.hero.github} <span aria-hidden="true">↗</span></a>
				<a class="text-link lp-benchmark-entry" href="#productivity-benchmarks">{copy.hero.benchmarks} <span aria-hidden="true">→</span></a>
			</div>
		</div>

		<figure class="lp-hero-video">
			<DemoVideo name={copy.videos.compare} mode="loop" label={copy.hero.videoLabel} />
			<figcaption>{copy.hero.videoCaption}</figcaption>
		</figure>
	</div>
</section>

<section class="lp-section" id="how" aria-labelledby="how-title">
	<div class="container">
		<header class="lp-head">
			<span class="section-label">{copy.how.label}</span>
			<h2 id="how-title">{copy.how.title}</h2>
			<p>{copy.how.intro}</p>
		</header>
		<ol class="lp-how">
			<li>
				<svg viewBox="0 0 120 84" fill="none" aria-hidden="true">
					<rect x="6" y="6" width="108" height="72" rx="12" fill="#fffef9" stroke="#cdd8c8" stroke-width="2" />
					<path d="M6 24h108" stroke="#cdd8c8" stroke-width="2" /><circle cx="18" cy="15" r="3" fill="#cdd8c8" /><circle cx="29" cy="15" r="3" fill="#cdd8c8" />
					<path d="M20 62c14-2 14-22 30-22s20 14 32 6 12-12 18-16" stroke="#9db98a" stroke-width="2.4" stroke-dasharray="3 6" stroke-linecap="round" />
					<circle cx="20" cy="62" r="5.5" fill="#d8ef78" stroke="#1b4338" stroke-width="2.4" /><circle cx="100" cy="30" r="7" fill="#d8ef78" stroke="#1b4338" stroke-width="2.4" />
				</svg>
				<span class="lp-num" aria-hidden="true">01</span>
				<h3>{copy.how.cards[0].title}</h3>
				<p>{copy.how.cards[0].text}</p>
			</li>
			<li>
				<svg viewBox="0 0 120 84" fill="none" aria-hidden="true">
					<rect x="14" y="8" width="92" height="20" rx="8" fill="#fffef9" stroke="#cdd8c8" stroke-width="2" />
					<rect x="14" y="32" width="92" height="20" rx="8" fill="#fffef9" stroke="#cdd8c8" stroke-width="2" />
					<rect x="14" y="56" width="92" height="20" rx="8" fill="#eaf3d0" stroke="#9db98a" stroke-width="2" />
					<circle cx="28" cy="18" r="5" fill="#d8ef78" stroke="#1b4338" stroke-width="2" /><circle cx="28" cy="42" r="5" fill="#d8ef78" stroke="#1b4338" stroke-width="2" /><circle cx="28" cy="66" r="5" fill="#d8ef78" stroke="#1b4338" stroke-width="2" />
					<path d="M42 18h46M42 42h38M42 66h50" stroke="#a9b9a6" stroke-width="3" stroke-linecap="round" />
				</svg>
				<span class="lp-num" aria-hidden="true">02</span>
				<h3>{copy.how.cards[1].title}</h3>
				<p>{copy.how.cards[1].text}</p>
			</li>
			<li>
				<svg viewBox="0 0 120 84" fill="none" aria-hidden="true">
					<circle cx="30" cy="42" r="16" fill="#fffef9" stroke="#cdd8c8" stroke-width="2" /><circle cx="90" cy="42" r="16" fill="#eaf3d0" stroke="#9db98a" stroke-width="2" />
					<path d="M46 34c9-9 19-9 28 0" stroke="#1b4338" stroke-width="2.4" stroke-linecap="round" /><path d="m70 28 5 6-7 2" stroke="#1b4338" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
					<path d="M74 50c-9 9-19 9-28 0" stroke="#9db98a" stroke-width="2.4" stroke-linecap="round" stroke-dasharray="3 5" /><path d="m50 56-5-6 7-2" stroke="#9db98a" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
					<circle cx="30" cy="42" r="5" fill="#d8ef78" stroke="#1b4338" stroke-width="2" /><circle cx="90" cy="42" r="5" fill="#d8ef78" stroke="#1b4338" stroke-width="2" />
				</svg>
				<span class="lp-num" aria-hidden="true">03</span>
				<h3>{copy.how.cards[2].title}</h3>
				<p>{copy.how.cards[2].text}</p>
			</li>
		</ol>
		<div class="lp-quick">
			<div class="lp-quick-head">
				<span class="lp-quick-label">{copy.how.quick.label}</span>
				<h3>{copy.how.quick.title}</h3>
				<p>{copy.how.quick.intro}</p>
			</div>
			<div class="lp-quick-body">
				<div class="lp-quick-tabs" role="group" aria-label={copy.how.quick.tabsLabel}>
					{#each quickHosts as host}
						<button type="button" disabled={!ready} class:chosen={quickHost === host.id} aria-pressed={quickHost === host.id} onclick={() => (quickHost = host.id)}>{host.name}</button>
					{/each}
				</div>
				{#if quickHost === 'muse'}
					<InstallCode korean={copy.lang === 'ko'} label={copy.how.quick.museInstall} code={museInstallCommand()} />
					<InstallCode korean={copy.lang === 'ko'} label={copy.how.quick.museConfig} code={museSettingsJson(publicMcpEndpoint)} />
					<p class="lp-quick-note">{copy.how.quick.museNote}</p>
				{:else if quickHost === 'claude-code'}
					<InstallCode korean={copy.lang === 'ko'} label={copy.how.quick.claudeLabel} code={remoteInstallCommand('claude-code', publicMcpEndpoint)} />
					<p class="lp-quick-note">{copy.how.quick.claudeNote}</p>
				{:else}
					<InstallCode korean={copy.lang === 'ko'} label={copy.how.quick.codexLabel} code={remoteInstallCommand('codex', publicMcpEndpoint)} />
					<p class="lp-quick-note">{copy.how.quick.codexNote}</p>
				{/if}
				<p class="lp-quick-note"><a class="text-link" href={guidePath + '#connect'}>{copy.how.quick.more} <span aria-hidden="true">→</span></a></p>
			</div>
		</div>
		<div class="lp-hosts">
			<span>{copy.how.hostsLabel}</span>
			{#each hosts as host}<b>{host}</b>{/each}
			<a class="text-link" href={guidePath}>{copy.how.guideLink} <span aria-hidden="true">→</span></a>
		</div>
	</div>
</section>

<section class="lp-section lp-workflows" id="workflows" aria-labelledby="workflows-title">
	<div class="container">
		<header class="lp-head">
			<span class="section-label">{copy.workflows.label}</span>
			<h2 id="workflows-title">{copy.workflows.title}</h2>
			<p>{copy.workflows.intro}</p>
		</header>
		<ul class="lp-wf-grid">
			{#each copy.workflows.items as item (item.id)}
				{@const compare = !!item.compareVideo && sideBySide[item.id] === true}
				<li class="lp-wf" data-workflow={item.id}>
					{#key compare}
						<DemoVideo name={compare && item.compareVideo ? copy.videos[item.compareVideo] : copy.videos[item.id]} label={compare ? (item.compareVideoLabel ?? item.videoLabel) : item.videoLabel} mode="manual" autoplay={compare} />
					{/key}
					<div class="lp-wf-body">
						<span class="lp-kicker"><span class="site-icon"><SiteGlyph kind={workflowGlyph[item.id]} /></span>{item.kicker}</span>
						<h3>{item.title}</h3>
						<dl class="lp-wf-flow">
							<div><dt>{copy.workflows.askLabel}</dt><dd class="lp-wf-ask">{item.ask}</dd></div>
							<div><dt>{copy.workflows.hintsLabel}</dt><dd><ul class="lp-wf-hints">{#each item.hints as hint}<li>{hint}</li>{/each}</ul></dd></div>
							<div><dt>{copy.workflows.getLabel}</dt><dd class="lp-wf-get">{item.result}</dd></div>
						</dl>
						{#if item.compareVideo}
							<button class="lp-wf-compare" type="button" onclick={() => (sideBySide[item.id] = !sideBySide[item.id])}>{compare ? copy.workflows.flowLabel : copy.workflows.compareLabel}<span aria-hidden="true">{compare ? '↺' : '▸'}</span></button>
						{/if}
						{#if item.id === 'blog'}
							<div class="lp-chips" aria-label={copy.workflows.chips.label}>
								<span>{copy.workflows.chips.time(percent(change(medium.secondsBefore, medium.secondsAfter)))}</span>
								<span>{copy.workflows.chips.calls(percent(change(medium.callsBefore, medium.callsAfter)))}</span>
								<small>{copy.workflows.chips.note}</small>
							</div>
						{:else if item.note}
							<p class="lp-fine">{item.note}</p>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
		<p class="lp-wf-more">{copy.workflows.libraryText} <a class="text-link" href="/memories">{copy.workflows.libraryLink} <span aria-hidden="true">→</span></a></p>
	</div>
</section>

<section class="lp-section lp-examples" id="example" aria-labelledby="examples-title">
	<div class="container">
		<div class="section-heading">
			<div>
				<span class="section-label">{copy.examples.label}</span>
				<h2 id="examples-title">{copy.examples.title}</h2>
				<p>{copy.examples.intro}</p>
			</div>
			<a class="text-link" href="/memories">{copy.examples.link} <span aria-hidden="true">→</span></a>
		</div>
		<div class="lp-example-flow">
			<div class="lp-context">
				<span class="section-label">{copy.examples.contextLabel}</span>
				<dl>
					<div><dt>{copy.examples.urlLabel}</dt><dd><code>{recallExample.url}</code></dd></div>
					<div><dt>{copy.examples.taskLabel}</dt><dd>{copy.examples.task}</dd></div>
					<div><dt>{copy.examples.keywordsLabel}</dt><dd class="lp-keywords">{#each recallExample.keywords as keyword}<code>{keyword}</code>{/each}</dd></div>
				</dl>
				<span class="lp-flow-arrow" aria-hidden="true">→</span>
			</div>
			<article class="lp-hint">
				<div class="lp-hint-head"><span>{copy.examples.hintLabel}</span><span class="pill">{copy.examples.pill}</span></div>
				<h3>{copy.examples.hintTitle}</h3>
				<ol>{#each copy.examples.steps as step}<li>{step}</li>{/each}</ol>
				<p>{copy.examples.apply}</p>
			</article>
		</div>
		<p class="lp-example-limits">{copy.examples.limits(recallLimits.hints, recallLimits.tokensPerHint)}</p>
	</div>
</section>

<section class="lp-section lp-productivity" id="productivity-benchmarks" aria-labelledby="productivity-title">
	<div class="container">
		<header class="lp-head">
			<span class="section-label">{copy.productivity.label}</span>
			<h2 id="productivity-title">{copy.productivity.title}</h2>
			<p>{copy.productivity.intro}</p>
		</header>
		<p class="lp-productivity-scope">{copy.productivity.scope(productivityPilot.date, productivityPilot.browser, productivityPilot.model, productivityPilot.limitSeconds)}</p>
		<ul class="lp-productivity-list">
			{#each productivityPilot.rows as row (row.id)}
				<li>
					<article class="lp-productivity-row" data-service={row.id} aria-labelledby={`benchmark-${row.id}`}>
						<div class="lp-productivity-context">
							<div class="lp-productivity-meta">
								<span>{row.round} · {row.hints}</span>
								<span class="lp-productivity-pass">{copy.productivity.passed}</span>
							</div>
							<h3 id={`benchmark-${row.id}`}><a href={row.url} target="_blank" rel="noreferrer">{row.name} <span aria-hidden="true">↗</span></a></h3>
							<p class="lp-productivity-workflow">{copy.productivity.workflows[row.id]}</p>
							<a class="text-link" href={`/benchmarks/${row.id}.md`}>{copy.productivity.report} <span aria-hidden="true">→</span></a>
						</div>
						<div class="lp-productivity-comparisons">
							{#each row.comparisons as comparison (comparison.variant)}
								{@const timeChange = percent(change(comparison.secondsBefore, comparison.secondsAfter))}
								{@const callChange = percent(change(comparison.callsBefore, comparison.callsAfter))}
								<div class="lp-productivity-comparison">
									<table>
										<caption class:single={row.comparisons.length === 1}>{row.name} · {copy.productivity.candidate(comparison.variant)}</caption>
										<thead>
											<tr><th scope="col">{copy.productivity.metric}</th><th scope="col">{copy.productivity.taskOnly}</th><th scope="col" class="lp-with-hints">{copy.productivity.withHints}</th><th scope="col" class="lp-delta-column">{copy.productivity.delta}</th></tr>
										</thead>
										<tbody>
											<tr>
												<th scope="row">{copy.productivity.time}<span class="lp-productivity-delta lp-mobile-delta" class:regressed={comparison.secondsAfter > comparison.secondsBefore}>{timeChange}</span></th>
												<td>{copy.productivity.seconds(comparison.secondsBefore)}</td><td class="lp-with-hints">{copy.productivity.seconds(comparison.secondsAfter)}</td>
												<td class="lp-delta-column"><span class="lp-productivity-delta" class:regressed={comparison.secondsAfter > comparison.secondsBefore}>{timeChange}</span></td>
											</tr>
											<tr>
												<th scope="row">{copy.productivity.calls}<span class="lp-productivity-delta lp-mobile-delta" class:regressed={comparison.callsAfter > comparison.callsBefore}>{callChange}</span></th>
												<td>{comparison.callsBefore}</td><td class="lp-with-hints">{comparison.callsAfter}</td>
												<td class="lp-delta-column"><span class="lp-productivity-delta" class:regressed={comparison.callsAfter > comparison.callsBefore}>{callChange}</span></td>
											</tr>
										</tbody>
									</table>
								</div>
							{/each}
						</div>
					</article>
				</li>
			{/each}
		</ul>
		<div class="lp-productivity-notes">
			<p>{copy.productivity.caveats}</p>
			<p><a href="https://docs.google.com/presentation/" target="_blank" rel="noreferrer">Google Slides ↗</a> {copy.productivity.unmeasured}</p>
		</div>
	</div>
</section>

<section class="lp-band" id="results" aria-labelledby="results-title">
	<div class="container">
		<header class="lp-head lp-head-light">
			<span class="section-label">{copy.results.label}</span>
			<h2 id="results-title">{copy.results.titleBefore}<em>{copy.results.titleEm}</em></h2>
			<p>{copy.results.intro}</p>
		</header>
		<p class="lp-result-scope">{copy.results.scope(pilot.rows.length)}</p>

		<div class="lp-tiles">
			<div class="lp-tile"><b>{percent(pilot.averageTimeChange)}</b><span>{copy.results.tileTime.label}</span><small>{copy.results.range(pilot.timeRange[0], pilot.timeRange[1])}</small></div>
			<div class="lp-tile"><b>{percent(pilot.averageCallChange)}</b><span>{copy.results.tileCalls.label}</span><small>{copy.results.range(pilot.callRange[0], pilot.callRange[1])}</small></div>
			<div class="lp-tile"><b>{iframeErrors.before} → {iframeErrors.after}</b><span>{copy.results.tileErrors.label}</span><small>{copy.results.tileErrors.small}</small></div>
			<div class="lp-tile"><b>≤ {recallLimits.tokensPerHint}</b><span>{copy.results.tileTokens.label}</span><small>{copy.results.tileTokens.small(recallLimits.hints)}</small></div>
		</div>

		<div class="lp-bars" use:reveal>
			<div class="lp-legend" aria-hidden="true"><span><i class="without"></i>{copy.results.legendWithout}</span><span><i class="with"></i>{copy.results.legendWith}</span></div>
			{#each pilot.rows as row, i (row.id)}
				{@const drop = change(row.secondsBefore, row.secondsAfter)}
				{@const label = copy.results.rowLabels[row.id]}
				{@const note = copy.results.rowNotes[row.id]}
				<div class="lp-row" style:--i={i} role="group" aria-label={copy.results.rowAria({ label, before: row.secondsBefore, after: row.secondsAfter, drop: percent(drop), callsBefore: row.callsBefore, callsAfter: row.callsAfter })}>
					<div class="lp-row-label">
						<b>{#if pilotSiteUrls[row.id]}<a href={pilotSiteUrls[row.id]} target="_blank" rel="noreferrer">{label} ↗</a>{:else}{label}{/if}</b>
						<small>{copy.results.rowKinds[row.kind]}</small>
					</div>
					<div class="lp-row-bars" aria-hidden="true">
						<div class="lp-bar"><span class="fill without" style:width={`${(row.secondsBefore / longest) * 100}%`}></span><em>{copy.results.seconds(row.secondsBefore)}</em></div>
						<div class="lp-bar"><span class="fill with" style:width={`${(row.secondsAfter / longest) * 100}%`}></span><em>{copy.results.seconds(row.secondsAfter)}</em><strong>{percent(drop)}</strong></div>
					</div>
					<div class="lp-row-calls" aria-hidden="true"><b>{row.callsBefore} → {row.callsAfter}</b><small>{copy.results.callsLabel}</small></div>
					{#if note}<p class="lp-row-note">{note}</p>{/if}
				</div>
			{/each}
		</div>

		<div class="lp-caveats">
			<p>{copy.results.caveats}</p>
		</div>
	</div>
</section>

<section class="lp-section lp-contribute" id="contribute" aria-labelledby="contribute-title">
	<div class="container feature-band">
		<div class="feature-band-copy">
			<span class="section-label">{copy.contribute.label}</span>
			<h2 id="contribute-title">{copy.contribute.titleLine1}<br />{copy.contribute.titleLine2}</h2>
			<p>{copy.contribute.text}</p>
			<ul class="lp-points">
				{#each copy.contribute.points as point}
					<li><b>{point.value}</b><span>{point.text}</span></li>
				{/each}
			</ul>
			<div class="lp-actions">
				<a class="button button-dark" href="/contribute">{copy.contribute.guide} <span aria-hidden="true">↗</span></a>
				<a class="text-link" href="/leaderboard">{copy.contribute.ranks} <span aria-hidden="true">→</span></a>
			</div>
			<p class="contribution-stage-note">{copy.contribute.note}</p>
		</div>
		<div class="feature-band-art" aria-hidden="true">
			<svg viewBox="0 0 360 240" fill="none">
				<path d="M54 180c34-2 38-45 77-45 35 0 41 29 74 18 34-12 41-52 77-52 17 0 31 6 44 15" stroke="#7B9569" stroke-width="2" stroke-dasharray="5 7" />
				<path d="M55 180c35-2 41-45 77-45s45 28 77 18c34-12 40-52 76-52 15 0 27 5 39 12" stroke="#416851" stroke-width="3" />
				<circle cx="55" cy="180" r="9" fill="#D7EF78" stroke="#3D654F" stroke-width="3" />
				<circle cx="132" cy="135" r="9" fill="#D7EF78" stroke="#3D654F" stroke-width="3" />
				<circle cx="209" cy="153" r="9" fill="#D7EF78" stroke="#3D654F" stroke-width="3" />
				<circle cx="285" cy="101" r="9" fill="#D7EF78" stroke="#3D654F" stroke-width="3" />
				<rect x="92" y="42" width="112" height="42" rx="7" fill="#F7F8ED" stroke="#CCD7C3" />
				<text x="107" y="60" fill="#617462" font-size="9" font-family="monospace">{copy.contribute.art.observationLabel}</text>
				<text x="107" y="74" fill="#203B2D" font-size="11" font-family="sans-serif">{copy.contribute.art.observation}</text>
				<rect x="211" y="174" width="105" height="39" rx="7" fill="#F7F8ED" stroke="#CCD7C3" />
				<text x="226" y="190" fill="#617462" font-size="9" font-family="monospace">{copy.contribute.art.hintLabel}</text>
				<text x="226" y="203" fill="#203B2D" font-size="11" font-family="sans-serif">{copy.contribute.art.hint}</text>
			</svg>
		</div>
	</div>
</section>

<section class="lp-section lp-faq" id="faq" aria-labelledby="faq-title">
	<div class="container">
		<header class="lp-head">
			<span class="section-label">{copy.faq.label}</span>
			<h2 id="faq-title">{copy.faq.title}</h2>
		</header>
		<div class="lp-faq-items">
			{#each copy.faq.items as item}
				<details><summary>{item.question}<span aria-hidden="true">+</span></summary><p>{item.answer}</p></details>
			{/each}
		</div>
	</div>
</section>

<section class="lp-cta" aria-labelledby="cta-title">
	<div class="container lp-cta-card">
		<div>
			<h2 id="cta-title">{copy.cta.titleBefore}<em>{copy.cta.titleEm}</em></h2>
			<p>{copy.cta.text}</p>
		</div>
		<div class="lp-actions">
			<a class="button button-lime" href={guidePath}>{copy.cta.connect} <span aria-hidden="true">↗</span></a>
			<a class="button lp-ghost" href="/memories">{copy.cta.explore}</a>
		</div>
	</div>
</section>
</div>

<style>
	.lp-hero { padding: clamp(54px, 7vw, 96px) 0 clamp(48px, 6vw, 84px); }
	.lp-hero-grid { display: grid; grid-template-columns: minmax(0, .92fr) minmax(0, 1.2fr); align-items: center; gap: clamp(32px, 4.4vw, 60px); }
	.lp-eyebrow { display: inline-flex; align-items: center; gap: 10px; color: #4a6a55; font-size: 13px; font-weight: 700; letter-spacing: .01em; }
	.lp-eyebrow::before { width: 8px; height: 8px; content: ''; border-radius: 50%; background: #9bb83a; box-shadow: 0 0 0 4px rgba(155, 184, 58, .22); }
	h1 { margin: 22px 0 20px; font: 800 clamp(42px, 5vw, 66px)/1.02 Manrope, 'Apple SD Gothic Neo', sans-serif; letter-spacing: -.06em; text-wrap: balance; }
	h1 em, h2 em { color: #5c7956; font-style: normal; }
	.lp-lede { max-width: 520px; margin: 0 0 26px; color: #42584d; font-size: 16.5px; line-height: 1.7; }
	.lp-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
	.lp-play { width: 15px; height: 15px; fill: currentColor; }
	.lp-resources { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 22px; margin-top: 8px; }
	.lp-availability { max-width: 520px; margin: 20px 0 0; color: #526659; font-size: 13px; line-height: 1.7; }
	.lp-fine { max-width: 520px; margin: 0; color: #556a5e; font-size: 12.5px; line-height: 1.6; }
	.lp-hero-video { margin: 0; }
	.lp-hero-video figcaption { margin-top: 12px; color: #556a5e; font-size: 12.5px; }

	.lp-section { padding: clamp(56px, 7vw, 96px) 0; }
	.lp-section, .lp-band { scroll-margin-top: 82px; }
	.lp-head { max-width: 700px; margin-bottom: 36px; }
	.lp-head h2, .lp-cta h2 { margin: 10px 0 12px; font: 800 clamp(30px, 3.6vw, 46px)/1.08 Manrope, sans-serif; letter-spacing: -.055em; text-wrap: balance; }
	.lp-head p { margin: 0; color: #4c6154; font-size: 16px; line-height: 1.7; }

	.lp-kicker { display: inline-flex; align-items: center; gap: 10px; color: #4a6a55; font-size: 13px; font-weight: 700; }
	.lp-chips { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
	.lp-chips span { padding: 8px 14px; border-radius: 999px; color: #132f28; background: var(--lime); font: 800 15px Manrope, sans-serif; letter-spacing: -.02em; }
	.lp-chips small { color: #556a5e; font-size: 12px; }

	.lp-workflows { padding-bottom: clamp(28px, 4vw, 48px); }
	.lp-wf-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 22px; margin: 0; padding: 0; list-style: none; }
	.lp-wf { display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--line); border-radius: 20px; background: var(--paper); box-shadow: var(--shadow); }
	.lp-wf :global(.demo-video) { border-radius: 0; box-shadow: none; }
	.lp-wf-body { display: flex; flex: 1; flex-direction: column; gap: 14px; padding: 22px 24px 26px; }
	.lp-wf .lp-kicker { gap: 10px; }
	.lp-wf h3 { margin: 0; font: 800 clamp(21px, 1.9vw, 25px)/1.2 Manrope, sans-serif; letter-spacing: -.04em; text-wrap: balance; }
	.lp-wf-flow { display: grid; gap: 14px; margin: 2px 0 0; }
	.lp-wf-flow > div { display: grid; gap: 6px; }
	.lp-wf-flow dt { color: #607464; font: 500 11px/1.3 'DM Mono', monospace; letter-spacing: .08em; text-transform: uppercase; }
	.lp-wf-flow dd { margin: 0; color: #2f4a3c; font-size: 14.5px; line-height: 1.6; }
	.lp-wf-ask { width: fit-content; max-width: 100%; padding: 11px 15px; border-radius: 16px 16px 16px 4px; color: var(--green-dark); background: var(--canvas); font-weight: 700; }
	.lp-wf-hints { display: grid; gap: 7px; margin: 0; padding: 0; list-style: none; }
	.lp-wf-hints li { display: flex; align-items: flex-start; gap: 10px; }
	.lp-wf-hints li::before { content: ''; flex: none; box-sizing: content-box; width: 6px; height: 6px; margin-top: .55em; border: 2px solid var(--green); border-radius: 50%; background: var(--lime); }
	.lp-wf-get { padding-left: 12px; border-left: 3px solid var(--lime); color: var(--green-dark); font-weight: 700; }
	.lp-wf-compare { align-self: flex-start; display: inline-flex; align-items: center; gap: 8px; padding: 9px 15px; border: 1px solid #c9d8c1; border-radius: 999px; color: var(--green); background: #f3f8ea; font: 700 13.5px Manrope, sans-serif; letter-spacing: -.01em; cursor: pointer; }
	.lp-wf-compare:hover { background: #e7f1d2; }
	.lp-wf-compare:focus-visible { outline: 2px solid #55783a; outline-offset: 2px; }
	.lp-wf-more { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; max-width: 760px; margin: 28px 0 0; color: #4c6154; font-size: 14.5px; line-height: 1.7; }
	.lp-wf .lp-chips { margin-top: auto; }
	.lp-wf .lp-fine { margin-top: auto; }

	.lp-band { padding: clamp(60px, 7vw, 100px) 0; color: #eaf0e0; background: #112c23; position: relative; overflow: hidden; }
	.lp-band::before { content: ''; position: absolute; inset: 0; opacity: .4; background-image: radial-gradient(rgba(221, 239, 196, .22) 1.2px, transparent 1.2px); background-size: 30px 30px; -webkit-mask-image: linear-gradient(115deg, #000 0%, transparent 60%); mask-image: linear-gradient(115deg, #000 0%, transparent 60%); }
	.lp-band .container { position: relative; }
	.lp-head-light .section-label { color: #9db8a2; }
	.lp-head-light h2 { color: #f5f3ec; }
	.lp-head-light h2 em { color: var(--lime); }
	.lp-head-light p { color: #b9cabb; }
	.lp-result-scope { margin: -16px 0 24px; color: #c4d8b7; font-size: 13px; line-height: 1.7; }
	.lp-tiles { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px; margin-bottom: 34px; }
	.lp-tile { display: flex; flex-direction: column; gap: 6px; padding: 22px 22px 20px; border: 1px solid rgba(216, 239, 120, .22); border-radius: 18px; background: rgba(255, 255, 255, .05); }
	.lp-tile b { color: var(--lime); font: 800 clamp(34px, 3.6vw, 46px)/1 Manrope, sans-serif; letter-spacing: -.05em; }
	.lp-tile span { color: #eef3e6; font-size: 15px; font-weight: 700; }
	.lp-tile small { color: #9db8a2; font-size: 12.5px; line-height: 1.5; }

	.lp-bars { display: grid; gap: 0; padding: 8px 26px 18px; border: 1px solid rgba(245, 243, 236, .14); border-radius: 20px; background: rgba(255, 255, 255, .04); }
	.lp-legend { display: flex; gap: 22px; justify-content: flex-end; padding: 14px 0 6px; color: #b9cabb; font-size: 13px; }
	.lp-legend i { display: inline-block; width: 12px; height: 12px; margin-right: 8px; border-radius: 4px; vertical-align: -1px; }
	.lp-legend .without, .fill.without { background: #7f9686; }
	.lp-legend .with, .fill.with { background: var(--lime); }
	.lp-row { display: grid; grid-template-columns: 200px minmax(0, 1fr) 96px; gap: 8px 24px; align-items: center; padding: 20px 0; border-top: 1px solid rgba(245, 243, 236, .12); }
	.lp-row-label b { display: block; color: #f5f3ec; font: 800 19px Manrope, sans-serif; letter-spacing: -.03em; }
	.lp-row-label small { color: #9db8a2; font-size: 12.5px; }
	.lp-row-bars { display: grid; gap: 8px; }
	.lp-bar { position: relative; display: flex; align-items: center; gap: 12px; height: 28px; }
	.fill { display: block; height: 100%; min-width: 6px; border-radius: 8px; transform-origin: left center; }
	.lp-bar em { color: #dfe8d6; font: 500 14px 'DM Mono', monospace; font-style: normal; white-space: nowrap; }
	.lp-bar strong { padding: 3px 9px; border-radius: 999px; color: #132f28; background: var(--lime); font: 800 13px Manrope, sans-serif; }
	.lp-row-calls { text-align: right; }
	.lp-row-calls b { display: block; color: #f5f3ec; font: 800 22px Manrope, sans-serif; letter-spacing: -.04em; white-space: nowrap; }
	.lp-row-calls small { color: #9db8a2; font-size: 12px; }
	.lp-row-note { grid-column: 2 / -1; margin: -2px 0 0; color: #9db8a2; font-size: 12.5px; line-height: 1.5; }
	.lp-bars:global(.armed) .fill { transform: scaleX(0); }
	.lp-bars:global(.armed.in-view) .fill { transform: scaleX(1); transition: transform 1s cubic-bezier(.2, .7, .2, 1); transition-delay: calc(var(--i, 0) * .09s); }
	.lp-caveats { display: grid; gap: 10px; max-width: 860px; margin-top: 26px; }
	.lp-caveats p { margin: 0; color: #a9bcac; font-size: 13px; line-height: 1.7; }

	.lp-benchmark-entry { font-size: 13px; }
	.lp-productivity { padding-top: 0; }
	.lp-productivity-scope { margin: -16px 0 24px; color: #607463; font-size: 13px; line-height: 1.7; }
	.lp-productivity-list { margin: 0; padding: 0; list-style: none; border-top: 1px solid #bdcbbb; }
	.lp-productivity-list > li { border-bottom: 1px solid #bdcbbb; }
	.lp-productivity-row { display: grid; grid-template-columns: minmax(0, .9fr) minmax(0, 1.5fr); gap: 48px; padding: 32px 0; align-items: start; }
	.lp-productivity-context, .lp-productivity-comparisons { min-width: 0; }
	.lp-productivity-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; color: #536959; font-size: 12px; }
	.lp-productivity-pass { padding: 4px 8px; border-radius: 5px; color: #31513c; background: #eaf3d0; font-size: 11px; font-weight: 700; }
	.lp-productivity-row h3 { margin: 14px 0 12px; font: 800 25px/1.3 Manrope, sans-serif; letter-spacing: -.03em; }
	.lp-productivity-row h3 a { color: inherit; text-decoration: none; }
	.lp-productivity-row h3 a:hover { text-decoration: underline; text-underline-offset: 4px; }
	.lp-productivity-row h3 span { font-size: 19px; color: #607463; }
	.lp-productivity-workflow { max-width: 430px; margin: 0 0 18px; color: #4c6154; font-size: 14px; line-height: 1.7; }
	.lp-productivity-row .text-link { font-size: 13px; }
	.lp-productivity-comparisons { display: grid; gap: 22px; }
	.lp-productivity-comparison table { width: 100%; border-collapse: collapse; table-layout: fixed; font-size: 15px; line-height: 1.5; }
	.lp-productivity-comparison caption { margin: 0 0 10px; color: #4c6154; font-size: 12px; font-weight: 700; text-align: left; }
	.lp-productivity-comparison caption.single { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
	.lp-productivity-comparison th, .lp-productivity-comparison td { padding: 15px 12px; border-bottom: 1px solid var(--line); text-align: right; vertical-align: middle; }
	.lp-productivity-comparison th:first-child { width: 25%; padding-left: 0; color: #2f4a3c; text-align: left; font-size: 13px; font-weight: 600; }
	.lp-productivity-comparison thead th { padding-top: 11px; color: #536959; font-size: 12px; font-weight: 600; }
	.lp-productivity-comparison td { color: #193e31; white-space: nowrap; font-variant-numeric: tabular-nums; }
	.lp-productivity-comparison .lp-with-hints { background: #edf2e7; font-weight: 700; }
	.lp-productivity-delta { display: inline-block; padding: 4px 7px; border-radius: 5px; color: #31513c; background: #e6eedc; font-size: 13px; font-weight: 700; white-space: nowrap; font-variant-numeric: tabular-nums; }
	.lp-productivity-delta.regressed { color: #87501e; background: #f7ead9; }
	.lp-productivity-comparison .lp-mobile-delta { display: none; }
	.lp-row-label a { color: inherit; text-underline-offset: 3px; }
	.lp-productivity-notes { max-width: 960px; margin-top: 24px; color: #566c5e; font-size: 13px; line-height: 1.8; }
	.lp-productivity-notes p { margin: 10px 0 0; }

	.lp-how { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 18px; margin: 0; padding: 0; list-style: none; position: relative; }
	.lp-how li { position: relative; padding: 26px 24px 28px; border: 1px solid var(--line); border-radius: 18px; background: var(--paper); }
	.lp-how svg { width: 120px; height: 84px; margin-bottom: 18px; }
	.lp-num { position: absolute; top: 24px; right: 24px; color: #5b6c5f; font: 500 13px 'DM Mono', monospace; }
	.lp-how h3 { margin: 0 0 8px; font: 800 20px/1.25 Manrope, sans-serif; letter-spacing: -.04em; }
	.lp-how p { margin: 0; color: #5a6d61; font-size: 14.5px; line-height: 1.65; }
	.lp-quick { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: 28px; align-items: start; margin-top: 30px; padding: clamp(22px, 3vw, 34px); border: 1px solid var(--line); border-radius: 20px; background: var(--paper); }
	.lp-quick-label { color: #456a4c; font: 500 11px 'DM Mono', monospace; letter-spacing: .12em; }
	.lp-quick h3 { margin: 8px 0 8px; font: 800 24px/1.2 Manrope, sans-serif; letter-spacing: -.045em; }
	.lp-quick-head p { margin: 0; color: #5a6d61; font-size: 14.5px; line-height: 1.65; }
	.lp-quick-tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 4px; }
	.lp-quick-tabs button { padding: 8px 14px; border: 1px solid var(--line); border-radius: 999px; color: #3f574a; background: transparent; font: 600 12.5px 'DM Sans', sans-serif; cursor: pointer; }
	.lp-quick-tabs button.chosen { border-color: #688354; color: #254b3c; background: #f0f5e7; box-shadow: inset 0 0 0 1px #688354; }
	.lp-quick-body :global(.code-panel) { margin-top: 10px; }
	.lp-quick-body :global(.code-panel pre) { font-size: 11.5px; line-height: 1.8; overflow-wrap: anywhere; }
	.lp-quick-note { margin: 12px 0 0; color: #5a6d61; font-size: 13px; line-height: 1.7; }
	.lp:lang(ko) .lp-quick h3 { letter-spacing: -.02em; line-height: 1.3; }
	.lp:lang(ko) .lp-quick-head p, .lp:lang(ko) .lp-quick-note { line-height: 1.8; text-wrap: pretty; }
	.lp-hosts { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 12px; margin-top: 26px; color: #556a5e; font-size: 13px; }
	.lp-hosts b { padding: 7px 12px; border: 1px solid var(--line); border-radius: 999px; color: #3f574a; background: var(--paper); font-size: 12.5px; }
	.lp-hosts .text-link { margin-left: auto; }

	.lp-examples { padding-top: 0; }
	.lp-example-flow { display: grid; grid-template-columns: minmax(0, .8fr) minmax(0, 1.2fr); gap: 26px; }
	.lp-context { position: relative; min-width: 0; padding: 30px; border: 1px solid var(--line); border-radius: 18px; background: var(--paper); }
	.lp-context dl { display: grid; gap: 23px; margin: 27px 0 0; }
	.lp-context dt { margin-bottom: 7px; color: #607464; font-size: 12px; }
	.lp-context dd { margin: 0; color: #254a3b; font-size: 16px; font-weight: 600; overflow-wrap: anywhere; }
	.lp-context code { font: 12px/1.7 'DM Mono', monospace; }
	.lp-keywords { display: flex; flex-wrap: wrap; gap: 8px; }
	.lp-keywords code { padding: 5px 9px; border-radius: 6px; background: #eaf0df; }
	.lp-flow-arrow { position: absolute; top: calc(50% - 21px); right: -35px; z-index: 1; display: grid; place-items: center; width: 42px; height: 42px; border: 1px solid #c9d8c1; border-radius: 50%; color: #365a44; background: #e9f0dc; font-size: 24px; }
	.lp-hint { min-width: 0; padding: 30px 32px; border-radius: 18px; color: #eef4e8; background: #193e31; }
	.lp-hint-head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; color: #b7cbb3; font: 11px/1.6 'DM Mono', monospace; letter-spacing: .03em; }
	.lp-hint-head .pill { border-color: #799666; color: #e9f2d7; background: #31533d; font-family: 'DM Sans', sans-serif; }
	.lp-hint h3 { margin: 22px 0 18px; color: #f4f6e9; font: 800 26px/1.25 Manrope, sans-serif; letter-spacing: -.03em; }
	.lp-hint ol { display: grid; gap: 12px; margin: 0; padding-left: 22px; color: #dfebd8; font-size: 15px; line-height: 1.7; }
	.lp-hint li { padding-left: 5px; }
	.lp-hint li::marker { color: #d8ef78; font-family: 'DM Mono', monospace; }
	.lp-hint > p { margin: 24px 0 0; padding-top: 18px; border-top: 1px solid #49664f; color: #b9cdb4; font-size: 13px; line-height: 1.8; }
	.lp-example-limits { margin: 16px 0 0; color: #607463; font-size: 12.5px; line-height: 1.7; }
	.lp-faq .container { max-width: 940px; }
	.lp-faq-items { border-top: 1px solid var(--line); }
	.lp-faq details { border-bottom: 1px solid var(--line); }
	.lp-faq summary { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 23px 2px; color: #274b3b; font-size: 16px; font-weight: 700; list-style: none; cursor: pointer; }
	.lp-faq summary::-webkit-details-marker { display: none; }
	.lp-faq summary:focus-visible { outline: 2px solid #55783a; outline-offset: 3px; border-radius: 4px; }
	.lp-faq summary span { flex-shrink: 0; color: #779261; font: 25px/1 'DM Mono', monospace; transition: transform .15s; }
	.lp-faq details[open] summary span { transform: rotate(45deg); }
	.lp-faq details > p { max-width: 760px; margin: 0; padding: 0 36px 24px 2px; color: #566c5e; font-size: 15px; line-height: 1.85; }
	.lp-points { display: grid; gap: 10px; margin: 0 0 24px; padding: 0; list-style: none; }
	.lp-points li { display: flex; align-items: baseline; gap: 14px; color: #52665b; font-size: 14px; line-height: 1.5; }
	.lp-points b { flex: none; min-width: 46px; color: var(--green); font: 800 22px Manrope, sans-serif; letter-spacing: -.04em; }

	.lp-cta { padding: 0 0 8px; }
	.lp-cta-card { display: flex; align-items: center; justify-content: space-between; gap: 32px; padding: clamp(30px, 4vw, 52px); border-radius: 24px; color: #f5f3ec; background: #112c23; box-shadow: 0 30px 70px rgba(19, 47, 40, .2); }
	.lp-cta h2 { margin: 0 0 10px; color: #f5f3ec; }
	.lp-cta h2 em { color: var(--lime); }
	.lp-cta p { max-width: 560px; margin: 0; color: #b9cabb; font-size: 15.5px; line-height: 1.7; }
	.lp-ghost { color: #f5f3ec; border-color: rgba(245, 243, 236, .3); background: transparent; }
	.lp-ghost:hover { background: rgba(255, 255, 255, .08); }

	@media (max-width: 1080px) {
		.lp-wf-grid { grid-template-columns: 1fr; }
		.lp-wf { display: grid; grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr); align-items: stretch; }
		.lp-wf :global(.demo-video) { align-self: start; }
	}
	@media (max-width: 1000px) {
		.lp-hero-grid { grid-template-columns: 1fr; }
		.lp-tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); }
		.lp-how { grid-template-columns: 1fr; }
		.lp-quick { grid-template-columns: 1fr; }
		.lp-productivity-row { grid-template-columns: minmax(0, .8fr) minmax(0, 1.2fr); gap: 28px; }
		.lp-row { grid-template-columns: 150px minmax(0, 1fr) 84px; gap: 8px 16px; }
	}
	@media (max-width: 760px) {
		.lp-productivity-row { grid-template-columns: 1fr; gap: 22px; padding: 28px 0; }
		.lp-productivity-workflow { max-width: none; }
		.lp-wf { display: flex; }
		.lp-wf-body { padding: 20px 20px 24px; }
		.lp-example-flow { grid-template-columns: 1fr; }
		.lp-flow-arrow { top: auto; right: calc(50% - 21px); bottom: -35px; transform: rotate(90deg); }
		.lp-context, .lp-hint { padding: 24px; }
		.lp-bars { padding: 6px 16px 14px; }
		.lp-row { grid-template-columns: minmax(0, 1fr) auto; }
		.lp-row-bars { grid-column: 1 / -1; grid-row: 2; }
		.lp-row-note { grid-column: 1 / -1; }
		.lp-legend { justify-content: flex-start; }
		.lp-cta-card { flex-direction: column; align-items: flex-start; }
		.lp-hosts .text-link { margin-left: 0; }
	}
	@media (max-width: 520px) {
		.lp-productivity-comparison .lp-delta-column { display: none; }
		.lp-productivity-comparison .lp-mobile-delta { display: table; margin-top: 6px; padding: 2px 5px; font-size: 11px; }
		.lp-productivity-comparison th, .lp-productivity-comparison td { padding: 13px 7px; }
		.lp-productivity-comparison th:first-child { width: 28%; padding-left: 0; font-size: 12px; }
		.lp-productivity-comparison td { font-size: 14px; }
		.lp-tiles { grid-template-columns: 1fr; }
		.lp-actions .button { width: 100%; }
	}

	.lp-hero { position: relative; }
	.lp-lang { position: absolute; top: 22px; right: max(32px, calc((100% - 1180px) / 2)); display: flex; align-items: center; gap: 8px; color: #6a7c70; font-size: 13px; }
	.lp-lang a { padding: 2px 4px; border-radius: 4px; color: #3f574a; }
	.lp-lang a[aria-current='page'] { color: var(--green); font-weight: 700; text-decoration: underline; text-underline-offset: 4px; }

	/* Korean: keep words whole at line breaks, loosen the tight Latin tracking and avoid one-word last lines. */
	.lp:lang(ko) { word-break: keep-all; }
	.lp:lang(ko) h1 { letter-spacing: -.03em; line-height: 1.2; }
	.lp:lang(ko) .lp-head h2, .lp:lang(ko) .lp-cta h2, .lp:lang(ko) .lp-how h3, .lp:lang(ko) .feature-band-copy h2, .lp:lang(ko) .section-heading h2 { letter-spacing: -.02em; line-height: 1.25; }
	.lp:lang(ko) .section-label { font-size: 12px; letter-spacing: .02em; }
	.lp:lang(ko) .lp-lede, .lp:lang(ko) .lp-head p, .lp:lang(ko) .lp-how p, .lp:lang(ko) .lp-cta p, .lp:lang(ko) .lp-caveats p { line-height: 1.8; text-wrap: pretty; }
	.lp:lang(ko) .lp-wf h3 { letter-spacing: -.02em; line-height: 1.3; }
	.lp:lang(ko) .lp-wf-flow dd { line-height: 1.7; }
	.lp:lang(ko) .lp-wf-flow dt { font-size: 12px; letter-spacing: .02em; }
	@media (max-width: 1000px) { .lp-lang { right: 22px; } }
	@media (max-width: 760px) { .lp-lang { right: 18px; top: 18px; } }
</style>
