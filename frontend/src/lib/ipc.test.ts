import { describe, expect, it, vi } from 'vitest';
import {
	asFailure,
	changeMasterPassword,
	clearHistory,
	deleteVersion,
	listen,
	unlock,
	versions
} from './ipc';
import type { Action } from './model';

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

describe('two master passwords on their way to Rust', () => {
	const encode = (text: string) => new TextEncoder().encode(text);

	/** Tauri's invoke, answering with `answer`, and what it was handed: the
	 * frame itself, and a copy of it as it was while the call ran. */
	function sending(answer: () => Promise<unknown>) {
		const frames: Uint8Array[] = [];
		const read: number[][] = [];
		vi.stubGlobal('window', {
			__TAURI_INTERNALS__: {
				invoke: async (command: string, payload: Uint8Array) => {
					expect(command).toBe('change_master_password');
					frames.push(payload);
					read.push([...payload]);
					return answer();
				}
			}
		});
		return { frames, read };
	}

	/** The frame Rust cuts the two apart by. The same bytes are pinned on the
	 * Rust side, so a change to either half fails a test. */
	it("are framed as the current one's length, the current one, then the new one", async () => {
		const { read } = sending(async () => 0);

		await changeMasterPassword(encode('é'), encode('x\0y'));
		await changeMasterPassword(new Uint8Array(), encode('new'));

		expect(read).toEqual([
			[0, 0, 0, 2, 0xc3, 0xa9, 0x78, 0x00, 0x79],
			[0, 0, 0, 0, 0x6e, 0x65, 0x77]
		]);
	});

	it('answers with how many backups still open with the old password', async () => {
		sending(async () => 7);
		expect(await changeMasterPassword(encode('old'), encode('new'))).toBe(7);
	});

	it('wipes both passwords and the frame once the call is over', async () => {
		const { frames } = sending(async () => 0);
		const current = encode('the one it had');
		const next = encode('the one it has');

		await changeMasterPassword(current, next);

		for (const wiped of [current, next, frames[0]]) {
			expect(wiped.every((byte) => byte === 0)).toBe(true);
		}
	});

	/** A refusal is the common answer - a current password typed wrong - and
	 * its two passwords are as secret as an accepted pair. */
	it('wipes them when the command rejects as well', async () => {
		const { frames } = sending(async () => {
			throw { code: 'wrongCredentials', message: "that is not the vault's current password" };
		});
		const current = encode('typed wrong');
		const next = encode('wanted');

		await expect(changeMasterPassword(current, next)).rejects.toMatchObject({
			code: 'wrongCredentials'
		});

		for (const wiped of [current, next, frames[0]]) {
			expect(wiped.every((byte) => byte === 0)).toBe(true);
		}
	});

	it('frames a password longer than two bytes of length could count', async () => {
		const { read } = sending(async () => 0);
		const current = new Uint8Array(70_000).fill(0x61);

		await changeMasterPassword(current, encode('new'));

		expect(read[0].slice(0, 4)).toEqual([0, 1, 0x11, 0x70]);
		expect(read[0]).toHaveLength(4 + 70_000 + 3);
		expect(read[0].slice(-3)).toEqual([0x6e, 0x65, 0x77]);
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

describe('the way Rust tells the window what was chosen outside it', () => {
	/** Tauri's own internals as far as a channel uses them: callbacks by number,
	 * and the one command that hands Rust the channel. */
	function internals() {
		const callbacks = new Map<number, (message: unknown) => void>();
		const sent: [string, unknown][] = [];
		vi.stubGlobal('window', {
			__TAURI_INTERNALS__: {
				transformCallback: (callback: (message: unknown) => void) => {
					const id = callbacks.size + 1;
					callbacks.set(id, callback);
					return id;
				},
				unregisterCallback: (id: number) => callbacks.delete(id),
				invoke: async (command: string, payload: unknown) => {
					sent.push([command, payload]);
					return undefined;
				}
			}
		});
		return { callbacks, sent };
	}

	/** What Rust names the channel by, which is the one argument `listen`
	 * takes: anything else and the command is refused before it runs. */
	it('sends the channel as the one argument listen takes', async () => {
		const { sent } = internals();

		await listen(() => {});

		expect(sent).toHaveLength(1);
		const [command, payload] = sent[0];
		expect(command).toBe('listen');
		expect(Object.keys(payload as object)).toEqual(['channel']);
		expect(JSON.parse(JSON.stringify(payload))).toEqual({ channel: '__CHANNEL__:1' });
	});

	/** Each message is evaluated into the page on its own, and two can arrive
	 * out of the order Rust sent them. The reader chose New Entry and then
	 * Lock Vault, and that is the order the page runs them in. */
	it('hands the page what Rust sends in the order Rust sent it, whatever order it arrives in', async () => {
		const { callbacks } = internals();
		const heard: Action[] = [];

		await listen((action) => heard.push(action));
		const deliver = callbacks.get(1);
		deliver?.({ message: { action: 'command', command: 'lock' }, index: 1 });
		expect(heard, 'a later choice ran before an earlier one arrived').toEqual([]);
		deliver?.({ message: { action: 'command', command: 'newEntry' }, index: 0 });

		expect(heard).toEqual([
			{ action: 'command', command: 'newEntry' },
			{ action: 'command', command: 'lock' }
		]);

		// Rust letting go of the channel - its window went - is the end of it.
		deliver?.({ end: true, index: 2 });
		expect(callbacks.has(1)).toBe(false);
	});
});
