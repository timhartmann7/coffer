//! Input chosen to break things: damaged files, hostile headers, credentials at
//! the edges, and databases larger than anybody would build on purpose.

use std::path::Path;

use keepass::db::{Value, fields};
use zeroize::Zeroizing;

use vault_core::{LockPolicy, MasterKey, Vault, VaultError};

use crate::support::{self, BUILT_PASSWORD, built, built_with, open, password};

const RICH: &str = "rich-kdbx41.kdbx";
const SECRET: &str = "coffer-test";

fn open_error(path: &Path, secret: &str) -> VaultError {
    Vault::open(path, password(secret), LockPolicy::Respect).expect_err("this should not open")
}

fn write_bytes(directory: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

// ---------------------------------------------------------------- damaged files

#[test]
fn a_zero_byte_file_is_not_a_database() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = write_bytes(scratch.path(), "empty.kdbx", b"");
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::NotADatabase
    ));
}

#[test]
fn a_file_that_is_not_kdbx_at_all_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    for (name, bytes) in [
        (
            "text.kdbx",
            b"this is a text file, not a database".as_slice(),
        ),
        ("short.kdbx", &[0x03, 0xd9, 0xa2]),
        ("zeros.kdbx", &[0u8; 64]),
        ("png.kdbx", b"\x89PNG\r\n\x1a\n and then some"),
    ] {
        let path = write_bytes(scratch.path(), name, bytes);
        assert!(
            matches!(
                open_error(&path, SECRET),
                VaultError::NotADatabase | VaultError::UnsupportedFormat
            ),
            "{name} was not refused cleanly"
        );
    }
}

#[test]
fn a_keepass_1_identifier_is_refused_rather_than_parsed() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    // The KDBX signature with the KeePass 2 pre-release identifier, which the
    // library declines. There is no way to produce a real KDB or KDBX 2 file:
    // KeePassXC 2.7 dropped both writers.
    let mut header = vec![0x03, 0xd9, 0xa2, 0x9a];
    header.extend_from_slice(&0xb54b_fb66u32.to_le_bytes());
    header.extend_from_slice(&[0, 0, 2, 0]);
    header.extend_from_slice(&[0u8; 64]);

    let path = write_bytes(scratch.path(), "kdb2.kdbx", &header);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::UnsupportedFormat
    ));
}

#[test]
fn a_database_truncated_anywhere_is_refused_without_panicking() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let whole = std::fs::read(support::fixture(RICH)).expect("the fixture reads");

    // Every tenth of the file, plus the boundaries that matter: inside the
    // version block, inside the header, at the header hash, and inside the
    // attachment payload.
    let mut lengths: Vec<usize> = (1..40).map(|step| whole.len() * step / 40).collect();
    lengths.extend([1, 4, 11, 12, 13, 100, whole.len() - 1]);

    for length in lengths {
        let path = write_bytes(
            scratch.path(),
            "cut.kdbx",
            whole.get(..length).expect("the prefix exists"),
        );
        let error = open_error(&path, SECRET);
        assert!(
            matches!(
                error,
                VaultError::NotADatabase
                    | VaultError::DamagedHeader
                    | VaultError::DamagedPayload
                    | VaultError::DamagedContent
                    | VaultError::WrongCredentials
                    | VaultError::Io(_)
            ),
            "a file cut at {length} bytes gave {error:?}"
        );
    }
}

#[test]
fn a_single_flipped_bit_anywhere_is_caught() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let whole = std::fs::read(support::fixture(RICH)).expect("the fixture reads");

    for offset in [
        12,
        20,
        40,
        80,
        140,
        200,
        400,
        whole.len() / 2,
        whole.len() - 8,
    ] {
        let mut damaged = whole.clone();
        if let Some(byte) = damaged.get_mut(offset) {
            *byte ^= 0xff;
        }

        let path = write_bytes(scratch.path(), "flipped.kdbx", &damaged);
        let error = open_error(&path, SECRET);
        assert!(
            !matches!(error, VaultError::Locked(_)),
            "a bit flipped at {offset} produced {error:?}"
        );
    }
}

// ------------------------------------------------------------- hostile headers

/// Overwrites the eight-byte value of a one-letter key in the header's key
/// derivation dictionary. The dictionary is in the clear, outside everything the
/// header signature covers, which is exactly why it needs checking before it is
/// used.
fn forge_kdf_value(bytes: &mut [u8], key: u8, value: u64) -> bool {
    let pattern = [0x05, 0x01, 0x00, 0x00, 0x00, key, 0x08, 0x00, 0x00, 0x00];
    let Some(at) = bytes
        .windows(pattern.len())
        .position(|window| window == pattern)
    else {
        return false;
    };

    let start = at + pattern.len();
    let Some(slot) = bytes.get_mut(start..start + 8) else {
        return false;
    };
    slot.copy_from_slice(&value.to_le_bytes());
    true
}

#[test]
fn an_absurd_aes_round_count_is_refused_before_it_is_run() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut bytes = std::fs::read(support::fixture(RICH)).expect("the fixture reads");

    assert!(
        forge_kdf_value(&mut bytes, b'R', u64::MAX),
        "the fixture should carry an AES round count to forge"
    );

    let path = write_bytes(scratch.path(), "forged.kdbx", &bytes);
    let error = open_error(&path, SECRET);

    // The header hash no longer matches either. Reporting the key derivation
    // rather than the hash is the proof that the check ran first, which is the
    // whole point: the library derives the key before it checks the hash.
    assert!(
        matches!(error, VaultError::AbsurdKeyDerivation),
        "an impossible round count gave {error:?}"
    );
}

#[test]
fn an_absurd_argon2_memory_request_is_refused_before_it_is_allocated() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    // Written with parameters small enough to save quickly, then forged.
    let path = built_with(scratch.path(), "argon2.kdbx", SECRET, |database| {
        database.config.kdf_config = keepass::config::KdfConfig::Argon2id {
            iterations: 1,
            memory: 8 * 1024,
            parallelism: 1,
            version: argon2::Version::Version13,
        };
        database.root_mut().add_entry();
    });

    open(&path, SECRET);

    let mut bytes = std::fs::read(&path).expect("the database reads");
    assert!(
        forge_kdf_value(&mut bytes, b'M', 4 * 1024 * 1024 * 1024 * 1024),
        "the header should carry an Argon2 memory request to forge"
    );
    std::fs::write(&path, &bytes).expect("the forged database is written");

    let error = open_error(&path, SECRET);
    assert!(
        matches!(error, VaultError::AbsurdKeyDerivation),
        "a four terabyte memory request gave {error:?}"
    );
}

#[test]
fn a_zero_iteration_count_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built_with(scratch.path(), "argon2.kdbx", SECRET, |database| {
        database.config.kdf_config = keepass::config::KdfConfig::Argon2id {
            iterations: 1,
            memory: 8 * 1024,
            parallelism: 1,
            version: argon2::Version::Version13,
        };
        database.root_mut().add_entry();
    });

    let mut bytes = std::fs::read(&path).expect("the database reads");
    assert!(forge_kdf_value(&mut bytes, b'I', 0));
    std::fs::write(&path, &bytes).expect("the forged database is written");

    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::AbsurdKeyDerivation
    ));
}

// ----------------------------------------------------------------- credentials

#[test]
fn an_empty_password_opens_a_database_that_has_one() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built_with(scratch.path(), "open.kdbx", "", |database| {
        database.root_mut().add_entry();
    });

    let vault = Vault::open(
        &path,
        MasterKey::from_password(Zeroizing::new(Vec::new())),
        LockPolicy::Respect,
    )
    .expect("an empty password is a password");
    assert_eq!(vault.tree().entries.len(), 1);
}

#[test]
fn a_ten_megabyte_password_works() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let secret = "a".repeat(10 * 1024 * 1024);
    let path = built_with(scratch.path(), "long.kdbx", &secret, |database| {
        database.root_mut().add_entry();
    });

    open(&path, &secret);
    assert!(matches!(
        open_error(&path, &secret[..secret.len() - 1]),
        VaultError::WrongCredentials
    ));
}

#[test]
fn a_password_of_combining_marks_only_works_and_is_not_normalised() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    // Combining acute, grave, circumflex: no base characters at all.
    let decomposed = "\u{301}\u{300}\u{302}";
    let path = built_with(scratch.path(), "marks.kdbx", decomposed, |database| {
        database.root_mut().add_entry();
    });

    open(&path, decomposed);

    // A form that a normaliser might consider equivalent must not open it.
    // Coffer hashes the bytes it is handed and never touches Unicode form.
    assert!(matches!(
        open_error(&path, "\u{300}\u{301}\u{302}"),
        VaultError::WrongCredentials
    ));
}

#[test]
fn a_password_that_is_not_text_is_refused_before_key_derivation() {
    let (_scratch, database) = support::scratch(RICH);
    let error = Vault::open(
        &database,
        MasterKey::from_password(Zeroizing::new(vec![0xff, 0xfe, 0xfd])),
        LockPolicy::Respect,
    )
    .expect_err("invalid text is not a password");

    assert!(matches!(error, VaultError::PasswordNotUtf8));
}

// -------------------------------------------------------------------- content

#[test]
fn fifty_thousand_entries_open_and_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "many.kdbx", |database| {
        for index in 0..50_000 {
            database.root_mut().add_entry().edit(|entry| {
                entry.set_unprotected(fields::TITLE, format!("entry {index}"));
                entry.set_unprotected(fields::USERNAME, "someone");
            });
        }
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    assert_eq!(vault.tree().entries.len(), 50_000);
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(open(&path, BUILT_PASSWORD).tree().entries.len(), 50_000);
}

#[test]
fn a_hundred_levels_of_nesting_open_and_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "deep.kdbx", |database| {
        let mut group = database.root_mut().id();
        for level in 0..100 {
            group = database
                .group_mut(group)
                .expect("the group is there")
                .add_group()
                .edit(|section| section.name = format!("level {level}"))
                .id();
        }
        database
            .group_mut(group)
            .expect("the deepest group is there")
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "at the bottom"));
    });

    let mut vault = open(&path, BUILT_PASSWORD);

    let mut depth = 0;
    let mut node = vault.tree();
    while let Some(next) = node.sections.into_iter().next() {
        depth += 1;
        node = next;
    }
    assert_eq!(depth, 100);
    assert_eq!(node.entries.len(), 1);

    vault.save().expect("the database saves");
}

#[test]
fn two_hundred_custom_fields_survive() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "wide.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "wide");
            for index in 0..200 {
                entry.set(
                    format!("custom {index:03}"),
                    if index % 2 == 0 {
                        Value::unprotected(format!("value {index}"))
                    } else {
                        Value::protected(format!("secret {index}"))
                    },
                );
            }
        });
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    assert_eq!(
        vault.entry(id).expect("the entry is there").fields.len(),
        201
    );
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    let entry = vault.entry(id).expect("the entry is there");
    assert_eq!(entry.fields.len(), 201);
    assert_eq!(
        vault
            .reveal(id, "custom 001")
            .and_then(|value| value.expose_str().map(str::to_owned)),
        Some("secret 1".to_owned())
    );
}

#[test]
fn a_one_megabyte_field_value_survives() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let long = "x".repeat(1024 * 1024);
    let path = built(scratch.path(), "long-field.kdbx", |database| {
        let text = "x".repeat(1024 * 1024);
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "long");
            entry.set_protected(fields::NOTES, text);
        });
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        vault
            .reveal(id, fields::NOTES)
            .expect("the notes reveal")
            .expose()
            .len(),
        long.len()
    );
}

#[test]
fn a_hundred_megabyte_attachment_survives() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let size = 100 * 1024 * 1024;
    let path = built(scratch.path(), "huge.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "huge");
            entry.add_attachment("blob.bin", Value::unprotected(vec![0x5au8; size]));
        });
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    let entry = vault.entry(id).expect("the entry is there");
    assert_eq!(entry.attachments[0].size, size);
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    let bytes = vault
        .attachment(id, "blob.bin")
        .expect("the attachment is there");
    assert_eq!(bytes.expose().len(), size);
    assert!(bytes.expose().iter().all(|&byte| byte == 0x5a));
}

// ------------------------------------------------------------------ behaviour

#[test]
fn a_hundred_open_and_close_cycles_leave_nothing_behind() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "cycled.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "one");
            entry.set_protected(fields::PASSWORD, "hunter2");
        });
    });

    for _ in 0..100 {
        let vault = open(&path, BUILT_PASSWORD);
        let id = vault.tree().entries[0].id;
        assert!(vault.reveal(id, fields::PASSWORD).is_some());
    }

    let leftovers: Vec<_> = std::fs::read_dir(scratch.path())
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "a hundred cycles left {leftovers:?} behind"
    );
}

#[test]
fn another_client_writing_the_file_is_noticed_in_either_order() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "shared.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "one");
        });
    });

    // Coffer first, then the other client: Coffer's save succeeds and the other
    // client's write is what Coffer notices next time.
    let mut vault = open(&path, BUILT_PASSWORD);
    vault.save().expect("the database saves");

    let elsewhere = built(scratch.path(), "other.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "written elsewhere");
        });
    });
    std::fs::copy(&elsewhere, &path).expect("the other client writes");

    assert!(matches!(
        vault.save().expect_err("the save is refused"),
        VaultError::ExternalChange
    ));
    drop(vault);

    // The other way round: the other client writes before Coffer ever saves.
    let mut vault = open(&path, BUILT_PASSWORD);
    std::fs::copy(&elsewhere, &path).expect("the other client writes again");
    assert!(matches!(
        vault.save().expect_err("the save is refused"),
        VaultError::ExternalChange
    ));

    // Whatever happened, the other client's database is the one on disk.
    drop(vault);
    assert_eq!(
        open(&path, BUILT_PASSWORD).tree().entries[0].title(),
        "written elsewhere"
    );
}
