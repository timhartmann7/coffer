/**
 * The only module that knows Coffer is a Tauri application.
 *
 * Everything the screens know about the database comes through here, and the
 * one value that is a secret - a revealed field - is handed straight to the DOM
 * node that shows it and is never kept.
 */

import { Channel, invoke } from '@tauri-apps/api/core';
import type {
	Action,
	Adopted,
	Attached,
	Calibration,
	Command,
	Database,
	Deleting,
	Deletion,
	Entry,
	Failure,
	Generated,
	Generator,
	Group,
	History,
	Kinds,
	Made,
	Move,
	Moved,
	Point,
	Position,
	Purpose,
	Recipe,
	Removed,
	Rival,
	SavedCopy,
	Settings,
	Snapshot,
	Span,
	Status,
	Subject,
	Tagged,
	Target
} from './model';

export function status(): Promise<Status> {
	return invoke('status');
}

/** Opens the system's file picker. `null` when the user closed it. */
export function chooseDatabase(): Promise<Database | null> {
	return invoke('choose_database');
}

/** Points the session at the vault Rust found in Coffer's own folder. It takes
 * nothing: Rust opens the file the last status named, or answers `gone` when it
 * is no longer a vault, so nothing this window sends can name a file. */
export function chooseFound(): Promise<Database> {
	return invoke('choose_found');
}

/** Points the session at what already sits where the new vault would go, for a
 * reader who meant to open it. The place is the one Rust settled, so nothing is
 * sent. Anything there but a vault, or the copy a lock left of one, is answered
 * `gone`. */
export function chooseExisting(): Promise<Database> {
	return invoke('choose_existing');
}

/**
 * Sends the master password to Rust as bytes.
 *
 * The bytes are the whole body of the message: not a JSON string, not a field
 * of an object, and nothing that a JavaScript string has to hold on the way.
 * The buffer is wiped once the call has finished with it.
 */
export async function unlock(password: Uint8Array): Promise<void> {
	try {
		await invoke('unlock', password);
	} finally {
		password.fill(0);
	}
}

/**
 * Opens the vault even though somebody's lock file sits beside it.
 *
 * Only ever after `unlock` rejected with `heldByAnother` and the reader was
 * shown who holds it. A lock left by a Mac that lost power cannot be told from
 * one held right now, so the answer is a person's rather than Coffer's.
 */
export async function unlockTakingOver(password: Uint8Array): Promise<void> {
	try {
		await invoke('unlock_over', password);
	} finally {
		password.fill(0);
	}
}

/** Opens the system's file picker for the key file some databases need beside
 * the password, and keeps it for the next unlock. `null` when the reader closed
 * the picker. */
export function chooseKeyFile(): Promise<Database | null> {
	return invoke('choose_key_file');
}

/** Takes the key file back off, for a reader who picked the wrong one. */
export function forgetKeyFile(): Promise<void> {
	return invoke('forget_key_file');
}

/** Where a vault goes when the reader has not said. Chosen without a panel, so
 * that making the first one is a password and nothing else. */
export function defaultNewDatabase(): Promise<Target> {
	return invoke('default_new_database');
}

/** Opens the system's save panel and keeps where the reader wants the new vault.
 * `null` when they closed it. */
export function chooseNewDatabase(): Promise<Target | null> {
	return invoke('choose_new_database');
}

/** What is at the place Rust is holding for the new vault now, read again
 * after the disk turned out to differ from what the screen said. */
export function target(): Promise<Target> {
	return invoke('target');
}

/**
 * Measures what a one-second unlock costs on this machine.
 *
 * Several seconds of work with every core busy, and the number it answers with
 * is the only one the screen has to show: it does not exist until the measuring
 * is over.
 */
export function calibrate(): Promise<Calibration> {
	return invoke('calibrate');
}

/**
 * Makes the chosen file into a vault and opens it.
 *
 * The password goes the same way an unlock's does - the whole body of the
 * message, as bytes - which is why where the vault goes and what it costs to
 * open were settled by the two calls above.
 */
export async function createDatabase(password: Uint8Array): Promise<void> {
	try {
		await invoke('create_database', password);
	} finally {
		password.fill(0);
	}
}

/**
 * Gives the open vault a new master password.
 *
 * Both passwords are the whole body of the message, as bytes, for the reason an
 * unlock's is. A body cannot carry named arguments beside it, so the two are
 * framed in it: the current password's length in four bytes, big-endian, then
 * the current password, then the new one. The two buffers handed in are wiped as
 * soon as the frame holds them, and the frame once the call has finished with
 * it, whatever it answered. Answers with how many automatic backups beside the
 * vault still open with the old password.
 */
export async function changeMasterPassword(current: Uint8Array, next: Uint8Array): Promise<number> {
	const body = new Uint8Array(4 + current.length + next.length);
	new DataView(body.buffer).setUint32(0, current.length);
	body.set(current, 4);
	body.set(next, 4 + current.length);
	current.fill(0);
	next.fill(0);
	try {
		return await invoke('change_master_password', body);
	} finally {
		body.fill(0);
	}
}

/** Removes the automatic backups that open with the password the vault had
 * before its last change, and answers with how many went - and, when some would
 * not go, how many are left and why. Nothing is sent: Rust removes the files it
 * counted when the password changed, wherever later saves have moved them, and
 * no others. */
export function removeOldSnapshots(): Promise<Removed> {
	return invoke('remove_old_snapshots');
}

export function lock(): Promise<void> {
	return invoke('lock');
}

/**
 * Says the reader is there, so that the idle timer starts again.
 *
 * Unless it had already run out: the first key after a Mac slept through the
 * deadline arrives before Rust's own timer wakes, and Rust locks the vault
 * rather than handing it a fresh timeout. This window is destroyed with it, and
 * the one built in its place says why.
 *
 * Sent on real input and no more than a few times a minute. What comes back is
 * the seconds the open vault has left, which nothing draws: a status bar that
 * asked once a second would be an idle timer resetting itself, and a vault that
 * never locks.
 */
export function stirred(): Promise<number | null> {
	return invoke('stirred');
}

/**
 * Hands Rust the way to tell this window what the reader chose outside it: an
 * item of the menu bar, or the close button. One way per window: the window a
 * lock builds next hands over its own, and Rust lets go of this one when its
 * window goes. A channel and not an event: it needs no capability, and nothing
 * of it outlives the window (see docs/ipc.md).
 */
export function listen(heard: (action: Action) => void): Promise<void> {
	const channel = new Channel<Action>(heard);
	return invoke('listen', { channel });
}

/** Tells the menu bar which of Coffer's items the window can do now. Rust greys
 * out the rest, and Lock Vault and Open Vault… follow the vault as well. */
export function menuState(enabled: Command[]): Promise<void> {
	return invoke('menu_state', { enabled });
}

/** Locks the vault and takes this window down for good; the Dock brings it
 * back. */
export function closeWindow(): Promise<void> {
	return invoke('close_window');
}

/**
 * Draws Coffer's menu under the pointer for what the reader right-clicked, in
 * place of WebKit's, and answers once it has closed. What is chosen from it
 * arrives on its own, through the way in `listen` handed over, with `serial`
 * and the ids it is about - before this answers, or after. Nothing of any
 * value is sent: ids and names, and for a revealed value the positions of the
 * part selected.
 */
export function contextMenu(serial: number, subject: Subject, at: Point): Promise<void> {
	return invoke('context_menu', { serial, subject, at });
}

/** What the reader chose, and the values they may choose instead. Both come
 * from Rust: a screen holding its own list would be a second place the answer
 * lives. */
export function settings(): Promise<Settings> {
	return invoke('settings');
}

/**
 * Puts a choice into effect.
 *
 * Answers with what was actually stored, which is not always what was sent: a
 * value Rust does not offer is settled onto one it does. The screen draws what
 * came back.
 */
export function setSettings(settings: Settings): Promise<Settings> {
	return invoke('set_settings', { settings });
}

export function tree(): Promise<Group> {
	return invoke('tree');
}

export function entry(id: string): Promise<Entry> {
	return invoke('entry', { id });
}

/**
 * One field of one entry, once. The caller writes what comes back straight into
 * the DOM: it does not belong in a variable, a store or an attribute.
 */
export function reveal(entry: string, field: string): Promise<string> {
	return invoke('reveal', { entry, field });
}

/**
 * Copies a field to the clipboard without the value passing through here, or
 * the part of it a reader selected on the screen. Answers with the number of
 * seconds until Coffer takes it off again.
 */
export function copy(entry: string, field: string, range: Span | null = null): Promise<number> {
	return invoke('copy', { entry, field, range });
}

/** Opens an entry's address, if Coffer opens addresses of that kind. */
export function openUrl(entry: string): Promise<void> {
	return invoke('open_url', { entry });
}

/** The snapshots beside the chosen database - or, when it is one of them,
 * beside the vault it was taken from - most recent first. Rust keeps the list,
 * and the slot `chooseSnapshot` sends back means what it meant in it. */
export function snapshots(): Promise<Snapshot[]> {
	return invoke('snapshots');
}

/**
 * Points Coffer at the snapshot the last list showed at `index`, wherever later
 * saves have moved it, and answers with what is chosen afterwards. Rejects with
 * `gone`, changing nothing, when it has been pushed out of the chain or was
 * never listed.
 *
 * With a vault open, that vault is locked first, the way any is: this window
 * may be gone before the answer arrives, and what is chosen afterwards is the
 * vault itself when the lock kept its work elsewhere, lost it, or pushed the
 * snapshot out with its own save.
 */
export function chooseSnapshot(index: number): Promise<Database> {
	return invoke('choose_snapshot', { index });
}

/** Points the session at the unsaved copy a lock left beside the vault. It takes
 * nothing: the path is built in Rust from the database the reader chose, so
 * nothing this window sends can name a file. */
export function chooseRescue(): Promise<Database> {
	return invoke('choose_rescue');
}

/** Takes that copy off the disk, at the reader's word. Coffer never removes it
 * on its own: it holds the one version of work the vault has not got. */
export function discardRescue(): Promise<void> {
	return invoke('discard_rescue');
}

/** Moves that copy into the vault's own name, for a vault whose file has gone.
 * No password and nothing sent: the copy is moved rather than opened, and Rust
 * refuses with `taken` when anything is at the vault's name, and with
 * `needsOpening` on a disk that cannot take it back unopened. */
export function putBackRescue(): Promise<Database> {
	return invoke('put_back_rescue');
}

/** Makes the open copy the vault it was taken from, the vault's file as it
 * stood kept as its newest snapshot, and answers with the vault, which is what
 * is open now. Refused with `externalChange` when the vault's file no longer
 * stands as the last `status` said. */
export function promoteRescue(): Promise<Database> {
	return invoke('promote_rescue');
}

/** Makes the open snapshot the vault it was taken beside, and answers with the
 * vault, which is what is open now, and with what became of the file it
 * replaced. No password: the vault opens with the one the snapshot opened with
 * from then on. Refused with `externalChange` when the vault's file no longer
 * stands as the last `status` said. */
export function adoptSnapshot(): Promise<Adopted> {
	return invoke('adopt_snapshot');
}

/** Goes back from the copy or the snapshot to the vault it was taken from, and
 * answers with what is chosen afterwards: the vault, or the copy when the lock
 * that closed it had to keep its work elsewhere or lost it. A file that is open
 * is locked on the way, so this window may be gone before the answer arrives. */
export function backToVault(): Promise<Database> {
	return invoke('back_to_vault');
}

/** The kinds of entry Coffer makes, a login first, and the names offered for
 * a field of the reader's own. The same answer every time. */
export function kinds(): Promise<Kinds> {
	return invoke('kinds');
}

/** Makes an entry of a kind - one of the words `kinds` offered - in a folder,
 * and answers with the tree it changed and the entry. Rust refuses a folder in
 * the recycle bin with `refused`. */
export function createEntry(group: string, kind: string): Promise<Made> {
	return invoke('create_entry', { group, kind });
}

/** Makes an entry in a folder from one of the vault's templates, every value
 * and file copied in Rust. Rust refuses with `refused` an entry that is not
 * one of them any more. */
export function createFromTemplate(group: string, template: string): Promise<Made> {
	return invoke('create_from_template', { group, template });
}

/** Makes a copy of an entry beside it, every value and file copied in Rust,
 * and answers with the tree and the copy. */
export function duplicateEntry(entry: string): Promise<Made> {
	return invoke('duplicate_entry', { entry });
}

/**
 * Deletes entries, every one of them or none, each with what the window showed
 * deleting it would do: Rust rejects the whole batch with `deletionChanged`,
 * deleting nothing, when that is no longer what happens to any of them. One
 * entry is a batch of one. `sequence` is the number `drafts.release` gave for
 * them: whatever was typed into them and said before it is let go.
 */
export function deleteEntries(entries: Deleting[], sequence: number): Promise<Group> {
	return invoke('delete_entries', { entries, sequence });
}

export function createGroup(parent: string, name: string): Promise<Group> {
	return invoke('create_group', { parent, name });
}

export function renameGroup(group: string, name: string): Promise<Group> {
	return invoke('rename_group', { group, name });
}

/** Deletes a folder and everything in it, on the terms `deleteEntries` gives
 * each entry. */
export function deleteGroup(group: string, deletion: Deletion): Promise<Group> {
	return invoke('delete_group', { group, deletion });
}

/**
 * Takes entries out of the recycle bin, every one of them or none, each back
 * to the folder it was deleted from, or to the top of the vault when that
 * folder is nowhere to go. Rust rejects with `refused` when any of them is no
 * longer in the bin, so an undo that arrives after one has already come back
 * moves nothing, and with `noSuchEntry` when one has gone.
 */
export function putBackEntries(entries: string[]): Promise<Group> {
	return invoke('put_back_entries', { entries });
}

/** Puts a tag on every entry named that lacks it, and answers with the tree
 * and those entries. Rust refuses the whole batch with `refused`, saying why,
 * for a tag the file would split or trim, and with `noSuchEntry` when one has
 * gone. */
export function tagEntries(entries: string[], tag: string): Promise<Tagged> {
	return invoke('tag_entries', { entries, tag });
}

/** Takes a tag off every entry named that has it: the undo of `tagEntries`,
 * sent only the entries that one answered it changed. */
export function untagEntries(entries: string[], tag: string): Promise<Group> {
	return invoke('untag_entries', { entries, tag });
}

/** Takes a folder out of the recycle bin with everything in it, on the same
 * terms. */
export function putBackGroup(group: string): Promise<Group> {
	return invoke('put_back_group', { group });
}

/**
 * Moves entries into a folder, or to the top of the vault with the root's id,
 * every one of them or none. Rust refuses the whole batch with `refused` when
 * one of them, or the folder, is in the recycle bin, and with `noSuchEntry`
 * when one of them or the folder is not there. One already in the folder stays
 * where it is and is not among those answered.
 */
export function moveEntries(entries: string[], into: string): Promise<Moved> {
	return invoke('move_entries', { entries, into });
}

/**
 * Takes a move back: each entry `moveEntries` answered goes out of `into` to
 * the folder it left, every one of them or none. Rust refuses with
 * `superseded`, moving nothing, once the file says any of them has moved since.
 */
export function moveEntriesBack(moved: Move[], into: string): Promise<Group> {
	return invoke('move_entries_back', { moved, into });
}

/** Moves a folder with everything in it, on the terms `moveEntries` gives. A
 * folder cannot go inside itself or a folder under it. */
export function moveGroup(group: string, into: string): Promise<Group> {
	return invoke('move_group', { group, into });
}

/**
 * Takes a folder's move back out of `into` to `from`, the folder it left. Rust
 * refuses with `superseded`, moving nothing, once the file says the folder has
 * moved since, or either folder has gone or is in the recycle bin.
 */
export function moveGroupBack(group: string, from: string, into: string): Promise<Group> {
	return invoke('move_group_back', { group, from, into });
}

export function emptyRecycleBin(): Promise<Group> {
	return invoke('empty_recycle_bin');
}

/**
 * Writes one field.
 *
 * `protect` is how a field the entry does not have yet is made. A field it has
 * keeps its own protection whatever this says - `setProtection` is the one way
 * to change that - so a value written on the way out of a field the reader has
 * just hidden cannot put it back into the file as plain text.
 *
 * `sequence` is the write's place in the count the drafts of the field carry
 * (`drafts.ts`), so that a draft sent before it and arriving after it is known
 * for what it is.
 */
export function setField(
	entry: string,
	field: string,
	value: string,
	protect: boolean,
	sequence: number
): Promise<Entry> {
	return invoke('set_field', { entry, field, value, protect, sequence });
}

/**
 * Tells Rust what is in a field the reader is typing into and has not left, so
 * that a lock can write it: `value` is the field's text, or `null` when what
 * was typed was taken back. `beside` is a new value typed in a Change field,
 * which a lock keeps beside the value it was for rather than writing it over
 * one. Only `drafts.ts` calls this.
 */
export function draft(
	entry: string,
	field: string,
	value: string | null,
	protect: boolean,
	beside: boolean,
	sequence: number
): Promise<void> {
	return invoke('draft', { entry, field, value, protect, beside, sequence });
}

/**
 * Takes a field of the reader's own off an entry.
 *
 * Rejects with `forGood`, and does nothing, when the vault's limits leave no
 * version to bring the field back from: the removal would be for good, and
 * `forever` is the reader having said that is what they want.
 */
export function removeField(entry: string, field: string, forever: boolean): Promise<Entry> {
	return invoke('remove_field', { entry, field, forever });
}

/**
 * Hides a field of the reader's own, or stops hiding it. The value moves
 * between the two kinds of storage inside Rust and is never sent from here. A
 * field hidden comes back with no value; a field shown is an open field from
 * then on, and its value arrives in the entry that comes back like any other
 * open value.
 */
export function setProtection(entry: string, field: string, protect: boolean): Promise<Entry> {
	return invoke('set_protection', { entry, field, protect });
}

/** Gives a field of the reader's own another name; its value and protection go
 * with it inside Rust. Rejects, changing nothing, when the name is taken. */
export function renameField(entry: string, from: string, to: string): Promise<Entry> {
	return invoke('rename_field', { entry, from, to });
}

/**
 * Puts back a field that just came off, by the version its removal wrote.
 *
 * Rust decides which version that is, under the same lock the restore runs in,
 * and rejects with `superseded`, doing nothing, when the removal is no longer
 * the last thing that happened to the entry: restoring whatever is newest then
 * would take back more than the field.
 */
export function undoRemoval(entry: string, field: string): Promise<Entry> {
	return invoke('undo_removal', { entry, field });
}

export function setTags(entry: string, tags: string[]): Promise<Entry> {
	return invoke('set_tags', { entry, tags });
}

/**
 * Opens the system's file picker and puts what it chose on the entry. The bytes
 * are read in Rust and never come near here.
 *
 * Nothing comes back when the reader closed the panel without choosing a file.
 * A name the entry already gives a file changes nothing: Rust holds the file
 * and answers `taken`, and one of the three calls below is the reader's answer.
 */
export function addAttachment(entry: string): Promise<Attached | null> {
	return invoke('add_attachment', { entry });
}

/** Puts the file waiting on this entry beside the one that has its name. */
export function keepBothAttachments(entry: string): Promise<Entry> {
	return invoke('keep_both_attachments', { entry });
}

/** Puts the file waiting on this entry in place of the one that has its name.
 * Rejects with `attachmentInHistory` while earlier versions hold that one, and
 * the file goes on waiting. */
export function replaceAttachment(entry: string): Promise<Entry> {
	return invoke('replace_attachment', { entry });
}

/** Lets go of the file waiting on this entry. Only this entry's: one chosen for
 * another entry since stays where it is. */
export function withdrawAttachment(entry: string): Promise<void> {
	return invoke('withdraw_attachment', { entry });
}

/** Opens the system's save panel and writes the file out from Rust. */
export function exportAttachment(entry: string, name: string): Promise<void> {
	return invoke('export_attachment', { entry, name });
}

export function removeAttachment(entry: string, name: string): Promise<Entry> {
	return invoke('remove_attachment', { entry, name });
}

/**
 * Takes a file off along with the previous versions that are holding it back.
 *
 * One call rather than two: the versions go only if the file then goes, so a
 * reader who asked to be rid of a file is not left having lost the history and
 * still having the file.
 */
export function removeAttachmentAndVersions(entry: string, name: string): Promise<Entry> {
	return invoke('remove_attachment_and_versions', { entry, name });
}

/**
 * Pairs a list of versions with the entry it was asked about, which the answer
 * does not say. This is the one place that holds both, so a list never exists
 * in the window without its entry (see `History`).
 */
async function history(entry: string, answer: Promise<Omit<History, 'entry'>>): Promise<History> {
	return { entry, ...(await answer) };
}

export function versions(entry: string): Promise<History> {
	return history(entry, invoke('versions', { entry }));
}

/**
 * One previous version, read the way an entry is read.
 *
 * This and every command below that takes a `Position` rejects with
 * `versionsChanged` when the vault has changed since the position was read,
 * and does nothing.
 */
export function version(entry: string, at: Position): Promise<Entry> {
	return invoke('version', { entry, index: at.index, revision: at.revision });
}

/** One field of one version, once - the same terms as a reveal. */
export function revealVersion(entry: string, at: Position, field: string): Promise<string> {
	return invoke('reveal_version', { entry, index: at.index, revision: at.revision, field });
}

/** Copies one field of one version, or the part of it a reader selected - the
 * same terms as a copy. */
export function copyVersion(
	entry: string,
	at: Position,
	field: string,
	range: Span | null
): Promise<number> {
	return invoke('copy_version', { entry, index: at.index, revision: at.revision, field, range });
}

export function restoreVersion(entry: string, at: Position): Promise<Entry> {
	return invoke('restore_version', { entry, index: at.index, revision: at.revision });
}

export function deleteVersion(entry: string, at: Position): Promise<History> {
	return history(
		entry,
		invoke('delete_version', { entry, index: at.index, revision: at.revision })
	);
}

export function clearHistory(entry: string): Promise<History> {
	return history(entry, invoke('clear_history', { entry }));
}

/** The recipe a generator opens with - the last one it made a password from -
 * and what the screen draws around it. */
export function generator(purpose: Purpose): Promise<Generator> {
	return invoke('generator', { purpose });
}

/**
 * Makes a password, and has Rust remember what it was made from for the
 * generator that asked.
 *
 * The password comes back the way a revealed value does, and it is treated the
 * same way: it goes into the node that shows it and into the field it was made
 * for, and it is not kept anywhere else.
 */
export function generatePassword(recipe: Recipe, purpose: Purpose): Promise<Generated> {
	return invoke('generate_password', { recipe, purpose });
}

/** Writes the database back. Rejects with `externalChange` when the file is not
 * the one the window opened. */
export function save(): Promise<void> {
	return invoke('save');
}

/** Writes over a file somebody else changed. What was there goes into the first
 * snapshot on the way. */
export function saveOver(): Promise<void> {
	return invoke('save_over');
}

/** Writes what is in the window to a file of its own. */
export function saveCopy(): Promise<Database | null> {
	return invoke('save_copy');
}

/**
 * Saves a copy of the open vault on another disk, in a panel Rust opens on the
 * folder the last such copy went to, another disk this Mac has mounted, or
 * beside the vault, and writes down when and where. Answers with whether it
 * went to the vault's own disk and what Coffer now knows of copies, or null
 * when the panel was closed. Rejects with `taken`, replacing nothing, for a
 * name that holds a file.
 */
export function copyVault(): Promise<SavedCopy | null> {
	return invoke('copy_vault');
}

/** Shows the chosen vault's file in the Finder, selected. Nothing is sent: it
 * is the file Rust chose. Rejects with `gone` when nothing is at its name. */
export function showInFinder(): Promise<void> {
	return invoke('show_in_finder');
}

/** Throws away what is in the window and reads the file again, typing that was
 * never finished included. `sequence` is the number `drafts.release` gave. */
export function reload(sequence: number): Promise<Group> {
	return invoke('reload', { sequence });
}

/** What the file on disk holds. Reading it means decrypting it, so it is asked
 * for when there is a decision to make and never on a timer. */
export function rival(): Promise<Rival> {
	return invoke('rival');
}

/**
 * What a rejected command said.
 *
 * A command rejects with the value Rust serialised, which is a plain object and
 * never an `Error`, so `instanceof` and `.message` are both useless on it.
 */
export function asFailure(thrown: unknown): Failure {
	const failed = thrown as Partial<Failure> | null;
	if (failed && typeof failed.message === 'string' && typeof failed.code === 'string') {
		return failed as Failure;
	}
	return { code: 'other', message: 'Coffer could not finish that.' };
}
