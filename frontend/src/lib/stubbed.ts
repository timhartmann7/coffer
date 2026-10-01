/**
 * `ipc.ts` as a test that mocks it sees it.
 *
 * Every command is a mock, made from the real module's own list of exports, so
 * a command added to `ipc.ts` is mocked in every test without anybody writing
 * it into a list - and a test that forgot one cannot reach Tauri by accident.
 * `asFailure` is the real one: it is not a command but the reading of a
 * refusal, and a test is about what the screen does with the reading the
 * window really makes. Nothing in the application imports this.
 *
 * A test mocks the module with it, keeping a handle it can set answers on:
 *
 * ```ts
 * const ipc = vi.hoisted(() => ({}) as Stubbed);
 * vi.mock(import('$lib/ipc'), async (real) =>
 * 	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
 * );
 * ```
 */

import { vi, type Mock } from 'vitest';
import type * as ipc from './ipc';

type Ipc = typeof ipc;

export type Stubbed = { [Name in keyof Ipc]: Name extends 'asFailure' ? Ipc[Name] : Mock };

export function stubbed(real: Ipc): Stubbed {
	return Object.fromEntries(
		Object.entries(real).map(([name, value]) => [name, name === 'asFailure' ? value : vi.fn()])
	) as Stubbed;
}
