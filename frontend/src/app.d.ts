// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	/** The bundle's version, read out of `tauri.conf.json` at build time by
	 * `vite.config.ts`. There is no second copy of it in this half. */
	const __COFFER_VERSION__: string;

	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
