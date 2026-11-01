<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { api } from '#lib/api';
	import { browserSession, loadBrowserSession, setBrowserSession } from '#lib/browser-session';
	import { githubRepositoryUrl } from '#lib/site.js';
	import MyPageIcon from '#lib/components/MyPageIcon.svelte';
	import '../app.css';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	let menuOpen = $state(false);
	let menuReady = $state(false);
	let signingOut = $state(false);
	onMount(() => { menuReady = true; if (!adminPage) void loadBrowserSession().catch(() => {}); });
	async function signOut() {
		signingOut = true;
		try { await api<void>('/v1/auth/logout', { method: 'POST' }); setBrowserSession(null); window.location.assign('/login'); }
		finally { signingOut = false; }
	}

	const adminPage = $derived(page.url.pathname === '/admin' || page.url.pathname.startsWith('/admin/'));
	const korean = $derived(adminPage || page.url.pathname === '/ko' || page.url.pathname.startsWith('/ko/'));
	const guidePath = $derived(korean ? '/ko/guide' : '/guide');
	$effect(() => { document.documentElement.lang = korean ? 'ko' : 'en'; });
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
		{ href: '/memories', label: korean ? '메모리' : 'Memories' },
		{ href: '/leaderboard', label: copy.ranking },
		{ href: guidePath, label: copy.setup },
		{ href: '/contribute', label: copy.contribute },
		{ href: '/dashboard', label: korean ? '마이 페이지' : 'My Page' }
	]);

	function isActivePath(href: string) {
		const pathname = page.url.pathname;
		if (pathname === href || pathname.startsWith(`${href}/`)) return true;
		if (href === '/dashboard') return pathname.startsWith('/settings/');
		if (href === '/leaderboard') return pathname.startsWith('/u/');
		return false;
	}
</script>

{#if adminPage}
	{@render children()}
{:else}
<a class="skip-link" href="#main-content">{copy.skip}</a>
<header class="site-header">
	<div class="nav-wrap">
		<a class="brand" href={korean ? '/ko' : '/'} onclick={() => (menuOpen = false)}>
			<img class="brand-mark" src="/brand/wam-mark.svg" width="39" height="39" alt="" />
			<span class="brand-copy"><strong>web agentic memory</strong><small>{copy.tagline}</small></span>
		</a>
		<nav class="desktop-nav" aria-label={copy.navigation}>
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href}>{item.label}</a>{/each}
			<a href={githubRepositoryUrl} target="_blank" rel="noopener noreferrer">GitHub <span aria-hidden="true">↗</span></a>
		</nav>
		<div class="nav-actions">{#if $browserSession}<details class="account-menu"><summary aria-label={korean ? '내 계정 메뉴' : 'Account menu'}><span class="account-avatar" aria-hidden="true"><MyPageIcon name="user" size={15}/></span><span>{korean ? '내 계정' : 'Account'}</span><span aria-hidden="true">⌄</span></summary><div class="account-dropdown"><a href="/dashboard">My Page</a><a href="/settings/profile">{copy.profile}</a><a href="/settings/devices">{copy.devices}</a><button type="button" onclick={signOut} disabled={signingOut}>{signingOut ? 'Signing out…' : korean ? '로그아웃' : 'Sign out'}</button></div></details>{:else}<a class="nav-signin" href="/login">{copy.login}</a>{/if}<a class="button button-dark button-small" href={guidePath}>{copy.connect} <span aria-hidden="true">↗</span></a></div>
		<button class="mobile-menu-button" type="button" disabled={!menuReady} aria-label={menuOpen ? copy.close : copy.open} aria-expanded={menuOpen} aria-controls="mobile-navigation" onclick={() => (menuOpen = !menuOpen)}><span></span><span></span></button>
	</div>
	{#if menuOpen}
		<nav id="mobile-navigation" class="mobile-nav" aria-label={copy.mobile}>
			{#each navItems as item}<a class:current={isActivePath(item.href)} aria-current={isActivePath(item.href) ? 'page' : undefined} href={item.href} onclick={() => (menuOpen = false)}>{item.label}<span aria-hidden="true">↗</span></a>{/each}
			<a href={githubRepositoryUrl} target="_blank" rel="noopener noreferrer" onclick={() => (menuOpen = false)}>GitHub <span aria-hidden="true">↗</span></a>
			<a href="/settings/profile" onclick={() => (menuOpen = false)}>{copy.profile} <span aria-hidden="true">↗</span></a>
			<a href="/settings/devices" onclick={() => (menuOpen = false)}>{copy.devices} <span aria-hidden="true">↗</span></a>
			{#if $browserSession}<button class="mobile-signout" type="button" onclick={signOut} disabled={signingOut}>{korean ? '로그아웃' : 'Sign out'}</button>{:else}<a href="/login" onclick={() => (menuOpen = false)}>{copy.login} <span aria-hidden="true">↗</span></a>{/if}
		</nav>
	{/if}
</header>
<main id="main-content" tabindex="-1">{@render children()}</main>
<footer class="site-footer">
	<div class="footer-inner">
		<a class="brand footer-brand" href={korean ? '/ko' : '/'}><img class="brand-mark" src="/brand/wam-mark.svg" width="35" height="35" alt="" /><span class="brand-copy"><strong>web agentic memory</strong><small>{copy.footerTagline}</small></span></a>
			<div class="footer-links"><a href="/leaderboard">{copy.ranking}</a><a href={guidePath}>{copy.setup}</a><a href="/memories">{copy.workflows}</a><a href="/contribute">{copy.contribute}</a><a href="/metrics">{copy.metrics}</a><a href={githubRepositoryUrl} target="_blank" rel="noopener noreferrer">GitHub <span aria-hidden="true">↗</span></a></div>
			<p>{copy.privacy}</p>
	</div>
</footer>
{/if}

<style>
.account-menu{position:relative}.account-menu>summary{display:flex;align-items:center;gap:8px;list-style:none;color:#4c6656;font-size:11px;font-weight:600;cursor:pointer}.account-menu>summary::-webkit-details-marker{display:none}.account-avatar{display:grid;place-items:center;width:31px;height:31px;border:1px solid #dbe4cf;border-radius:50%;background:#edf3e6;color:#3d6038;font:600 9px Manrope,sans-serif}.account-dropdown{position:absolute;right:0;top:43px;z-index:40;display:grid;min-width:180px;padding:7px;border:1px solid #dfe6d4;border-radius:9px;background:#fffef8;box-shadow:0 14px 35px #1c382018}.account-dropdown a,.account-dropdown button{padding:10px;border:0;border-radius:5px;background:none;text-align:left;color:#4b674c;font:500 11px 'DM Sans',sans-serif;cursor:pointer}.account-dropdown a:hover,.account-dropdown button:hover{background:#eff4e8}.account-dropdown button{border-top:1px solid #e6ecdd;border-radius:0}.mobile-signout{padding:14px 0;border:0;background:none;text-align:left;color:inherit;font:inherit;cursor:pointer}
</style>
