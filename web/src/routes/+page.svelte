<script lang="ts">
	import { memories } from '#lib/demo';

	const examples = memories.slice(0, 3);
</script>

<svelte:head>
	<title>Web Agentic Memory — Verified paths for web work</title>
	<meta name="description" content="Find web workflows by site and task, then share them with AI agents." />
</svelte:head>

<section class="hero">
	<div class="container hero-grid">
		<div class="hero-copy">
			<span class="eyebrow">Shared memory for web work</span>
			<h1>Find the path once.<br /><em>Pass it to the next agent.</em></h1>
			<p>WAM finds workflows for a site and task, then delivers them to MCP and CLI agents. Turn repeated detours into short, verifiable memories.</p>
			<div class="hero-actions">
				<a class="button button-dark" href="/memories">Explore memories <span aria-hidden="true">↗</span></a>
				<a class="button button-light" href="/guide">Connect an agent</a>
			</div>
			<div class="hero-footnote"><span aria-hidden="true">↳</span> Public recall uses URL scope and allowlisted keywords. Page contents and free-form prompts stay on your device.</div>
		</div>

		<div class="hero-visual" role="img" aria-label="Example workflow showing a title field, paragraph review, and draft save">
			<div class="visual-top"><span>Web workflow memory · example</span><span class="live-badge"><i></i> Workflow</span></div>
			<svg class="route-map" viewBox="0 0 580 380" fill="none" aria-hidden="true">
				<path class="route-line" d="M46 289C110 287 112 234 177 231S247 297 306 252s72-91 124-88 61 24 105 11" />
				<path class="route-line" d="M80 104c44 9 68 68 113 74 38 5 54-39 92-39 51 0 51 64 91 71 28 4 56-29 80-47" />
				<path class="route-line" d="M185 226c7-44 0-83 24-123 17-29 54-40 86-35s52 35 52 69" />
				<path class="route-strong" d="M49 288c63-1 63-53 126-56 63-3 68 43 126 20s72-88 129-89 67 24 105 12" />
				<path class="route-soft" d="M183 229c9-41 3-80 26-119 18-30 53-43 85-39 29 4 51 26 58 55" />
				<circle cx="49" cy="288" r="7" fill="#D8EF78" /><circle cx="175" cy="232" r="7" fill="#D8EF78" />
				<circle cx="301" cy="252" r="7" fill="#D8EF78" /><circle cx="430" cy="163" r="7" fill="#D8EF78" />
				<circle cx="535" cy="175" r="7" fill="#D8EF78" /><circle cx="352" cy="126" r="6" fill="#A8CCD1" />
			</svg>
			<div class="map-label map-label-one"><strong>Add a title</strong><small>MEDIUM · DRAFT</small></div>
			<div class="map-label map-label-two"><strong>Review paragraphs</strong><small>PREVIEW · STRUCTURE</small></div>
			<div class="map-label map-label-three"><strong>Save draft</strong><small>Leave the next step</small></div>
			<div class="visual-stamp"><strong>URL + task</strong><span>Recall only the useful path</span></div>
		</div>
	</div>
</section>

<section class="home-strip" aria-label="How the product works">
	<div class="container home-strip-inner">
		<div class="strip-intro">Find a memory.<br />Verify it. Carry it forward.</div>
		<div class="strip-step"><span>01</span><div><strong>Search by task scope</strong><small>URL · task · allowlisted keywords</small></div></div>
		<div class="strip-step"><span>02</span><div><strong>Review verified steps</strong><small>See status and version details</small></div></div>
		<a class="strip-step strip-step-link" href="/leaderboard"><span>03</span><div><strong>Meet contributors</strong><small>Browse opted-in profiles and ranks</small></div><b aria-hidden="true">↗</b></a>
	</div>
</section>

<section class="content-section home-examples">
	<div class="container">
		<div class="section-heading">
			<div><span class="section-label">DEMO EXAMPLES</span><h2>Small workflows are worth keeping</h2><p>These examples are separate from the approved catalog. Explore the memory page to see currently published items.</p></div>
			<a class="text-link" href="/memories">Explore published memories <span aria-hidden="true">→</span></a>
		</div>
		<div class="memory-grid">
			{#each examples as memory, index}
				<article class="memory-card example-card">
					<div class="memory-card-top">
						<span class={'site-icon ' + (index === 1 ? 'naver' : index === 2 ? 'slides' : '')}>{index === 1 ? 'N' : index === 2 ? 'G' : 'M'}</span>
						<span class="pill">Example</span>
					</div>
					<h3>{memory.title}</h3>
					<p>{memory.summary}</p>
					<div class="memory-card-meta"><span>{memory.site}</span><span>{memory.category}</span></div>
				</article>
			{/each}
		</div>
	</div>
</section>

<section class="content-section contribution-section">
	<div class="container feature-band">
		<div class="feature-band-copy">
			<span class="section-label">CONTRIBUTE WITH CONSENT</span>
			<h2>Choose what to share<br />before you submit.</h2>
			<p>Contributors choose a URL, task type, and actions to share. Page contents and free-form prompts are not sent. You can review your submission and consent scope.</p>
			<div class="hero-actions">
				<a class="button button-dark" href="/contribute">Contribution guide <span aria-hidden="true">↗</span></a>
				<a class="text-link" href="/leaderboard">View contributor ranks <span aria-hidden="true">→</span></a>
			</div>
			<p class="contribution-stage-note">The contribution flow currently shows development submissions and review states. Public publishing and points after approval are not connected yet.</p>
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
				<text x="107" y="60" fill="#617462" font-size="9" font-family="monospace">YOUR OBSERVATION</text>
				<text x="107" y="74" fill="#203B2D" font-size="11" font-family="sans-serif">One discovery</text>
				<rect x="211" y="174" width="105" height="39" rx="7" fill="#F7F8ED" stroke="#CCD7C3" />
				<text x="226" y="190" fill="#617462" font-size="9" font-family="monospace">SHARED HINT</text>
				<text x="226" y="203" fill="#203B2D" font-size="11" font-family="sans-serif">A shared shortcut</text>
			</svg>
		</div>
	</div>
</section>
