import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
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
			'/v1': 'http://127.0.0.1:3000',
			'/mcp': 'http://127.0.0.1:3000',
			'/healthz': 'http://127.0.0.1:3000'
		}
	}
});
