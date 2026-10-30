<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import '../app.css';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	let menuOpen = $state(false);
	let menuReady = $state(false);
	onMount(() => { menuReady = true; });

	const korean = $derived(page.url.pathname === '/ko' || page.url.pathname.startsWith('/ko/'));
	const copy = $derived(korean ? {
		home: 'Web Agentic Memory 홈', tagline: '좋은 경로를 남기고, 함께 나아가요.', skip: '본문으로 건너뛰기',
		navigation: '주 메뉴', mobile: '모바일 메뉴', login: '로그인', connect: '에이전트 연결',
		open: '메뉴 열기', close: '메뉴 닫기', profile: '프로필 설정', devices: '연결된 기기',
		footerTagline: '유용한 경로를 다음 사람에게.', workflows: '공유 워크플로', setup: '설치 안내',
		contribute: '기여하기', ranking: '랭킹', metrics: '사용 통계',
		privacy: '프로필과 순위 공개는 기여자가 직접 선택합니다. 조회만으로는 점수가 생기지 않습니다.'
	} : {
		home: 'Web Agentic Memory home', tagline: 'Leave a path. Move together.', skip: 'Skip to main content',
		navigation: 'Main navigation', mobile: 'Mobile navigation', login: 'Log in', connect: 'Connect an agent',
		open: 'Open menu', close: 'Close menu', profile: 'Profile settings', devices: 'Connected devices',
		footerTagline: 'Pass useful paths along.', workflows: 'Shared workflows', setup: 'Agent setup',
		contribute: 'Contribute', ranking: 'Leaderboard', metrics: 'Usage metrics',
		privacy: 'Contributors choose whether to show their profile and rank. Views alone never earn points.'
	});
	const navItems = $derived([
		{ href: '/leaderboard', label: copy.ranking },
		{ href: '/guide', label: copy.setup },
		{ href: '/contribute', label: copy.contribute },
		{ href: '/dashboard', label: korean ? '내 활동' : 'My activity' }
	]);

	function isActivePath(href: string) {
		const pathname = page.url.pathname;
		if (pathname === href || pathname.startsWith(`${href}/`)) return true;
		if (href === '/dashboard') return pathname.startsWith('/settings/');
		if (href === '/leaderboard') return pathname.startsWith('/u/');
		return false;
	}
</script>

<a class="skip-link" href="#main-content">{copy.skip}</a>
<header class="site-header">
	<div class="nav-wrap">
		<a class="brand" href={korean ? '/ko' : '/'} onclick={() => (menuOpen = false)}>
			<img class="brand-mark" src="/brand/wam-mark.svg" width="39" height="39" alt="" />
			<span class="brand-copy"><strong>web agentic memory</strong><small>{copy.tagline}</small></span>
		</a>
		<nav class="desktop-nav" aria-label={copy.navigation}>
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href}>{item.label}</a>{/each}
		</nav>
		<div class="nav-actions"><a class="nav-signin" href="/login">{copy.login}</a><a class="button button-dark button-small" href="/guide">{copy.connect} <span aria-hidden="true">↗</span></a></div>
		<button class="mobile-menu-button" type="button" disabled={!menuReady} aria-label={menuOpen ? copy.close : copy.open} aria-expanded={menuOpen} aria-controls="mobile-navigation" onclick={() => (menuOpen = !menuOpen)}><span></span><span></span></button>
	</div>
	{#if menuOpen}
		<nav id="mobile-navigation" class="mobile-nav" aria-label={copy.mobile}>
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href} onclick={() => (menuOpen = false)}>{item.label}<span aria-hidden="true">↗</span></a>{/each}
			<a href="/settings/profile" onclick={() => (menuOpen = false)}>{copy.profile} <span aria-hidden="true">↗</span></a>
			<a href="/settings/devices" onclick={() => (menuOpen = false)}>{copy.devices} <span aria-hidden="true">↗</span></a>
			<a href="/login" onclick={() => (menuOpen = false)}>{copy.login} <span aria-hidden="true">↗</span></a>
		</nav>
	{/if}
</header>
<main id="main-content" tabindex="-1">{@render children()}</main>
<footer class="site-footer">
	<div class="footer-inner">
		<a class="brand footer-brand" href={korean ? '/ko' : '/'}><img class="brand-mark" src="/brand/wam-mark.svg" width="35" height="35" alt="" /><span class="brand-copy"><strong>web agentic memory</strong><small>{copy.footerTagline}</small></span></a>
			<div class="footer-links"><a href="/leaderboard">{copy.ranking}</a><a href="/guide">{copy.setup}</a><a href="/memories">{copy.workflows}</a><a href="/contribute">{copy.contribute}</a><a href="/metrics">{copy.metrics}</a></div>
			<p>{copy.privacy}</p>
	</div>
</footer>
