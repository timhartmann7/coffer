import { describe, expect, it, vi } from 'vitest';
import { asFailure, clearHistory, deleteVersion, unlock, versions } from './ipc';

describe('the master password on its way to Rust', () => {
	it('is wiped once the call has finished with it', async () => {
		const sent: Uint8Array[] = [];
		vi.stubGlobal('window', {
			__TAURI_INTERNALS__: {
				invoke: async (_command: string, payload: Uint8Array) => {
					// What Tauri would read: the bytes, while the call is running.
					sent.push(new Uint8Array(payload));
					return undefined;
				}
			}
		});

		const bytes = new TextEncoder().encode('correct horse battery staple');
		await unlock(bytes);

		expect([...sent[0]]).toEqual([...new TextEncoder().encode('correct horse battery staple')]);
		expect([...bytes]).toEqual(Array.from({ length: bytes.length }, () => 0));
	});

	/** The buffer is wiped whatever happened, because a refused unlock is the
	 * common case and its password is the same secret. */
	it('is wiped when the command rejects as well', async () => {
		vi.stubGlobal('window', {
			__TAURI_INTERNALS__: {
				invoke: async () => {
					throw { code: 'wrongCredentials', message: 'wrong password or key file' };
				}
			}
		});

		const bytes = new TextEncoder().encode('hunter2');
		await expect(unlock(bytes)).rejects.toBeDefined();
		expect([...bytes]).toEqual([0, 0, 0, 0, 0, 0, 0]);
	});
});

describe('what a rejected command comes back as', () => {
	/** A command rejects with the value Rust serialised, which is a plain object
	 * and never an Error, so nothing may read `.message` off it blindly. */
	it('reads a failure Rust sent', () => {
		expect(asFailure({ code: 'wrongCredentials', message: 'wrong password or key file' })).toEqual({
			code: 'wrongCredentials',
			message: 'wrong password or key file'
		});
	});

	it('says something sensible about anything else', () => {
		for (const thrown of [null, undefined, 'a string', 42, new Error('boom'), {}, { code: 1 }]) {
			const failure = asFailure(thrown);
			expect(failure.code).toBe('other');
			expect(failure.message).toBe('Coffer could not finish that.');
		}
	});
});

describe('a list of versions on its way to the window', () => {
	/** The commands answer with the list and the revision it was read at, and
	 * a position means nothing without the entry it is a position in. Every
	 * list comes back holding the entry it was asked about, whichever of the
	 * three asked, and a position goes back with the revision it was read at. */
	it('carries the entry it was asked about and the revision it was read at', async () => {
		const asked: [string, unknown][] = [];
		const listed = [{ index: 0, modified: null }];
		vi.stubGlobal('window', {
			__TAURI_INTERNALS__: {
				invoke: async (command: string, payload: unknown) => {
					asked.push([command, payload]);
					return { revision: 4, versions: listed };
				}
			}
		});

		expect(await versions('Gmail')).toEqual({ entry: 'Gmail', revision: 4, versions: listed });
		expect(await deleteVersion('Google Drive', { index: 2, revision: 3 })).toEqual({
			entry: 'Google Drive',
			revision: 4,
			versions: listed
		});
		expect(await clearHistory('Bank')).toEqual({ entry: 'Bank', revision: 4, versions: listed });
		expect(asked).toEqual([
			['versions', { entry: 'Gmail' }],
			['delete_version', { entry: 'Google Drive', index: 2, revision: 3 }],
			['clear_history', { entry: 'Bank' }]
		]);
	});
});
