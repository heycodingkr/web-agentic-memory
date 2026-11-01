<script lang="ts">
	import { onMount } from 'svelte';

	// A demo clip from /media: H.264 MP4 with a poster and no audio track.
	// mode "loop": muted, loops, plays on its own (unless the visitor prefers reduced motion) and has a pause button.
	// mode "player": native controls; it starts muted when scrolled into view and pauses when scrolled away.
	// mode "manual": native controls and no autoplay, so several clips can sit side by side. It fetches nothing until the
	// clip is within a screen or so of the viewport, then metadata like the player does (preload "none" on a clip that
	// is on screen shows Chrome's loading spinner over the poster). autoplay starts a clip that a visitor asked for.
	let { name, label, mode = 'player', autoplay = false }: { name: string; label: string; mode?: 'loop' | 'player' | 'manual'; autoplay?: boolean } = $props();

	let video = $state<HTMLVideoElement>();
	let frame = $state<HTMLDivElement>();
	let playing = $state(false);
	let reduced = $state(false);
	let near = $state(false);

	onMount(() => {
		reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
		if (mode === 'manual' && frame) {
			const lazy = new IntersectionObserver(
				([entry]) => {
					if (entry.isIntersecting) {
						near = true;
						lazy.disconnect();
					}
				},
				{ rootMargin: '600px 0px' }
			);
			lazy.observe(frame);
			return () => lazy.disconnect();
		}
		if (reduced || !video || !frame) return;
		const target = video;
		const watcher = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting && entry.intersectionRatio >= 0.55) target.play().catch(() => {});
				else target.pause();
			},
			{ threshold: [0, 0.55, 1] }
		);
		watcher.observe(frame);
		return () => watcher.disconnect();
	});

	function toggle() {
		if (!video) return;
		if (video.paused) video.play().catch(() => {});
		else video.pause();
	}
</script>

<div class="demo-video" bind:this={frame}>
	<video
		bind:this={video}
		poster={`/media/${name}-poster.jpg`}
		muted
		playsinline
		loop={mode === 'loop'}
		controls={mode !== 'loop'}
		preload={mode === 'loop' ? 'auto' : mode === 'manual' && !near && !autoplay ? 'none' : 'metadata'}
		{autoplay}
		aria-label={label}
		onplay={() => (playing = true)}
		onpause={() => (playing = false)}
	>
		<source src={`/media/${name}.mp4`} type="video/mp4" />
	</video>
	{#if mode === 'loop'}
		<button class="video-toggle" type="button" onclick={toggle} aria-label={playing ? 'Pause the demo video' : 'Play the demo video'}>
			{#if playing}
				<svg viewBox="0 0 20 20" aria-hidden="true"><rect x="5" y="4" width="3.6" height="12" rx="1" /><rect x="11.4" y="4" width="3.6" height="12" rx="1" /></svg>
			{:else}
				<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M6 4.2v11.6a.8.8 0 0 0 1.2.7l9-5.8a.8.8 0 0 0 0-1.4l-9-5.8A.8.8 0 0 0 6 4.2z" /></svg>
			{/if}
		</button>
	{/if}
</div>

<style>
	.demo-video { position: relative; overflow: hidden; border-radius: 16px; background: #0f2a21; box-shadow: 0 28px 70px rgba(19, 47, 40, .22), 0 0 0 1px rgba(19, 47, 40, .1); }
	video { display: block; width: 100%; height: auto; aspect-ratio: 16 / 9; background: #0f2a21; }
	.video-toggle { position: absolute; right: 14px; bottom: 14px; display: grid; place-items: center; width: 42px; height: 42px; border: 1px solid rgba(245, 243, 236, .28); border-radius: 50%; color: #f5f3ec; background: rgba(15, 42, 33, .72); backdrop-filter: blur(6px); cursor: pointer; }
	.video-toggle:hover { background: rgba(15, 42, 33, .92); }
	.video-toggle svg { width: 18px; height: 18px; fill: currentColor; }
</style>
