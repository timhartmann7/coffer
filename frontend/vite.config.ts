import tailwindcss from '@tailwindcss/vite';
import { defaultClientConditions } from 'vite';
import { defineConfig } from 'vitest/config';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter()
			// No `files.assets`: nothing in the window is fetched by path. The mark
			// is markup, the icons are a sprite, and the brand files under
			// `assets/brand` are inputs to the icon build rather than things a
			// webview should be able to ask for.
		})
	],
	server: {
		// Tauri points its dev window at exactly this address. A port that
		// silently moved would leave a window with nothing in it.
		port: 1420,
		strictPort: true,
		host: 'localhost'
	},
	test: {
		expect: { requireAssertions: true },
		clearMocks: true,
		restoreMocks: true,
		unstubGlobals: true,
		// KDBX keeps its timestamps in UTC and the screen writes them in local
		// time. Pinning the zone is what makes an assertion about a date mean
		// the same thing on a laptop and on a build machine.
		env: { TZ: 'UTC' },
		projects: [
			{
				extends: './vite.config.ts',
				// Without the client conditions, `svelte` resolves to its server
				// build and mounting a component fails.
				resolve: { conditions: [...defaultClientConditions] },
				test: {
					name: 'client',
					environment: 'happy-dom',
					environmentOptions: {
						happyDOM: {
							settings: {
								disableJavaScriptFileLoading: true,
								disableCSSFileLoading: true,
								navigation: { disableMainFrameNavigation: true }
							}
						}
					},
					include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
					setupFiles: ['./src/lib/no-network.ts']
				}
			},
			{
				extends: './vite.config.ts',
				test: {
					name: 'server',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			}
		]
	}
});
