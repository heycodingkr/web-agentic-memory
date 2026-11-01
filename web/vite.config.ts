import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';
import { myPagePreview } from './dev/my-page-preview.ts';

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, '.', '');
	const apiTarget = env.VITE_WAM_API_TARGET || 'http://127.0.0.1:3000';

	return {
		plugins: [
			...(env.WAM_UI_PREVIEW === '1' ? [myPagePreview()] : []),
			sveltekit({
				adapter: adapter({ fallback: '200.html' }),
				compilerOptions: {
					runes: ({ filename }) =>
						filename.split(/[/\\\\]/).includes('node_modules') ? undefined : true
				}
			})
		],
		server: {
			proxy: {
				'/v1': apiTarget,
				'/mcp': apiTarget,
				'/.well-known/oauth-protected-resource': apiTarget,
				'/.well-known/oauth-authorization-server': apiTarget,
				'^/oauth/(authorize|register|token|revoke)(?:[/?]|$)': apiTarget,
				'/healthz': apiTarget
			}
		}
	};
});
