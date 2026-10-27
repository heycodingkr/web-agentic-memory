<script lang="ts">
	import { page } from '$app/state';
	import '../app.css';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	let menuOpen = $state(false);

	const navItems = [
		{ href: '/memories', label: 'Memories' },
		{ href: '/guide', label: 'Guide' },
		{ href: '/leaderboard', label: 'Leaderboard' },
		{ href: '/dashboard', label: 'My activity' }
	];

	function isActivePath(href: string) {
		const pathname = page.url.pathname;
		if (pathname === href || pathname.startsWith(`${href}/`)) return true;
		if (href === '/dashboard') return pathname.startsWith('/settings/');
		if (href === '/leaderboard') return pathname.startsWith('/u/');
		return false;
	}
</script>

<svelte:head>
	<meta name="description" content="Share verified web workflows so AI agents can finish tasks with fewer detours." />
</svelte:head>

<a class="skip-link" href="#main-content">Skip to main content</a>
<header class="site-header">
	<div class="nav-wrap">
		<a class="brand" href="/" aria-label="Web Agentic Memory home" onclick={() => (menuOpen = false)}>
			<img class="brand-mark" src="/brand/wam-mark.svg" width="39" height="39" alt="" />
			<span class="brand-copy"><strong>web agentic memory</strong><small>Leave a path. Move together.</small></span>
		</a>
		<nav class="desktop-nav" aria-label="Main navigation">
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href}>{item.label}</a>{/each}
		</nav>
		<div class="nav-actions"><a class="nav-signin" href="/login">Log in</a><a class="button button-dark button-small" href="/guide">Connect an agent <span aria-hidden="true">↗</span></a></div>
		<button class="mobile-menu-button" type="button" aria-label={menuOpen ? 'Close menu' : 'Open menu'} aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}><span></span><span></span></button>
	</div>
	{#if menuOpen}
		<nav class="mobile-nav" aria-label="Mobile navigation">
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href} onclick={() => (menuOpen = false)}>{item.label}<span aria-hidden="true">↗</span></a>{/each}
			<a href="/settings/profile" onclick={() => (menuOpen = false)}>Profile settings <span aria-hidden="true">↗</span></a>
			<a href="/settings/devices" onclick={() => (menuOpen = false)}>Connected devices <span aria-hidden="true">↗</span></a>
			<a href="/login" onclick={() => (menuOpen = false)}>Account login <span aria-hidden="true">↗</span></a>
		</nav>
	{/if}
</header>
<main id="main-content" tabindex="-1">{@render children()}</main>
<footer class="site-footer">
	<div class="footer-inner">
		<a class="brand footer-brand" href="/" aria-label="Web Agentic Memory home"><img class="brand-mark" src="/brand/wam-mark.svg" width="35" height="35" alt="" /><span class="brand-copy"><strong>web agentic memory</strong><small>Pass useful paths along.</small></span></a>
			<div class="footer-links"><a href="/memories">Explore memories</a><a href="/guide">Agent setup</a><a href="/contribute">Contribute</a><a href="/leaderboard">Leaderboard</a><a href="/metrics">Usage metrics</a></div>
			<p>Contributors choose whether to show their profile and rank. Views alone never earn points.</p>
	</div>
</footer>
