/**
 * Coffer makes no network requests, and neither does its test suite.
 *
 * happy-dom's `fetch` is built on Node's own HTTP stack, so a component that
 * asked for something would really ask for it. This turns that into a failed
 * test rather than a request nobody notices.
 */

import type { DetachedWindowAPI } from 'happy-dom';
import { beforeEach } from 'vitest';

beforeEach(() => {
	const happyDOM = (window as unknown as { happyDOM: DetachedWindowAPI }).happyDOM;
	happyDOM.settings.fetch.interceptor = {
		beforeAsyncRequest: () => {
			throw new Error('a test tried to make a network request');
		}
	};
});
