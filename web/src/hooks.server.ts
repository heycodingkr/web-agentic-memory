import type { Handle } from '@sveltejs/kit/hooks';

// The Korean landing page lives under /ko; everything else is English. Set <html lang> per page at render
// and prerender time so crawlers, translators and screen readers get the right language.
export const handle: Handle = ({ event, resolve }) => {
	const path = event.url.pathname;
	const lang = path === '/ko' || path.startsWith('/ko/') ? 'ko' : 'en';
	return resolve(event, {
		transformPageChunk: ({ html }) => html.replace('<html lang="en">', `<html lang="${lang}">`)
	});
};
