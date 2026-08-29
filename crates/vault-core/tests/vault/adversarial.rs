//! Input chosen to break things: damaged files, hostile headers, credentials at
//! the edges, and databases larger than anybody would build on purpose.

use std::path::Path;

use keepass::db::{Value, fields};
use zeroize::Zeroizing;

use vault_core::{LockPolicy, MasterKey, Vault, VaultError};

use crate::support::{self, BUILT_PASSWORD, RICH, SECRET, built, built_with, open, password};

fn open_error(path: &Path, secret: &str) -> VaultError {
    Vault::open(path, password(secret), LockPolicy::Respect).expect_err("this should not open")
}

fn write_bytes(directory: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

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
            matches!(
                error,
                VaultError::NotADatabase
                    | VaultError::UnsupportedFormat
                    | VaultError::DamagedHeader
                    | VaultError::DamagedPayload
                    | VaultError::DamagedContent
                    | VaultError::WrongCredentials
                    | VaultError::AbsurdKeyDerivation
            ),
            "a bit flipped at {offset} produced {error:?}"
        );
    }
}

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

/// Builds a KDBX outer header by hand, so that a test can state exactly what a
/// hostile file says.
struct Header {
    bytes: Vec<u8>,
    wide_lengths: bool,
}

impl Header {
    fn kdbx4() -> Header {
        Header {
            bytes: Header::version(1, 4),
            wide_lengths: true,
        }
    }

    fn kdbx3() -> Header {
        Header {
            bytes: Header::version(1, 3),
            wide_lengths: false,
        }
    }

    fn version(minor: u16, major: u16) -> Vec<u8> {
        let mut bytes = vec![0x03, 0xd9, 0xa2, 0x9a];
        bytes.extend_from_slice(&0xb54b_fb67u32.to_le_bytes());
        bytes.extend_from_slice(&minor.to_le_bytes());
        bytes.extend_from_slice(&major.to_le_bytes());
        bytes
    }

    fn field(mut self, id: u8, data: &[u8]) -> Header {
        self.bytes.push(id);
        if self.wide_lengths {
            let length = u32::try_from(data.len()).expect("the field fits");
            self.bytes.extend_from_slice(&length.to_le_bytes());
        } else {
            let length = u16::try_from(data.len()).expect("the field fits");
            self.bytes.extend_from_slice(&length.to_le_bytes());
        }
        self.bytes.extend_from_slice(data);
        self
    }

    fn end(self) -> Vec<u8> {
        self.field(0, b"\r\n\r\n").bytes
    }
}

/// A variant dictionary, as the key derivation parameters are written.
fn dictionary(entries: &[(&[u8], u8, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x01];
    for (key, tag, value) in entries {
        bytes.push(*tag);
        bytes.extend_from_slice(
            &u32::try_from(key.len())
                .expect("the key fits")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(
            &u32::try_from(value.len())
                .expect("the value fits")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(value);
    }
    bytes.push(0);
    bytes
}

const AES_KDF_UUID: [u8; 16] = [
    0xc9, 0xd9, 0xf3, 0x9a, 0x62, 0x8a, 0x44, 0x60, 0xbf, 0x74, 0x0d, 0x08, 0xc1, 0x8a, 0x4f, 0xea,
];

fn aes_kdf(rounds: u64) -> Vec<u8> {
    dictionary(&[
        (b"$UUID", 0x42, AES_KDF_UUID.to_vec()),
        (b"R", 0x05, rounds.to_le_bytes().to_vec()),
        (b"S", 0x42, vec![0; 32]),
    ])
}

/// Whether the library panics on these bytes, which is what the pre-flight
/// exists to prevent. Run in a scratch process of its own so that a panic does
/// not take the test with it.
fn library_panics(bytes: &[u8]) -> bool {
    let hushed = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(|| {
        let _ = keepass::Database::parse(bytes, keepass::DatabaseKey::new().with_password("x"));
    });
    std::panic::set_hook(hushed);
    outcome.is_err()
}

#[test]
fn a_header_field_narrower_than_the_parser_reads_is_refused_before_it_panics() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    // Compression id is read as a four-byte integer with no length check.
    let bytes = Header::kdbx4().field(3, &[]).end();
    assert!(
        library_panics(&bytes),
        "the library was supposed to panic on this, which is the reason the pre-flight exists"
    );

    let path = write_bytes(scratch.path(), "narrow.kdbx", &bytes);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::DamagedHeader
    ));
}

#[test]
fn a_key_derivation_value_narrower_than_its_type_is_refused_before_it_panics() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    // A round count declared as a 64-bit integer, with one byte of value.
    let parameters = dictionary(&[
        (b"$UUID", 0x42, AES_KDF_UUID.to_vec()),
        (b"R", 0x05, vec![1]),
    ]);
    let bytes = Header::kdbx4().field(11, &parameters).end();

    assert!(
        library_panics(&bytes),
        "the library was supposed to panic on this"
    );

    let path = write_bytes(scratch.path(), "short-value.kdbx", &bytes);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::DamagedHeader
    ));
}

#[test]
fn a_second_set_of_key_derivation_parameters_cannot_hide_behind_the_first() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    // The library keeps the last set it reads. A pre-flight that checked only
    // the first would let a file state something harmless and then state what
    // it means.
    let bytes = Header::kdbx4()
        .field(11, &aes_kdf(1_000))
        .field(11, &aes_kdf(u64::MAX))
        .end();

    let path = write_bytes(scratch.path(), "two-kdfs.kdbx", &bytes);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::AbsurdKeyDerivation
    ));
}

#[test]
fn an_absurd_kdbx3_round_count_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    // KDBX 3 states its round count in the header rather than a dictionary, and
    // states field lengths in two bytes rather than four.
    let bytes = Header::kdbx3().field(6, &u64::MAX.to_le_bytes()).end();

    let path = write_bytes(scratch.path(), "legacy-rounds.kdbx", &bytes);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::AbsurdKeyDerivation
    ));
}

#[test]
fn argon2_parameters_that_are_each_legal_but_absurd_together_are_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    const ARGON2ID: [u8; 16] = [
        0x9e, 0x29, 0x8b, 0x19, 0x56, 0xdb, 0x47, 0x73, 0xb2, 0x3d, 0xfc, 0x3e, 0xc6, 0xf0, 0xa1,
        0xe6,
    ];

    // Four gigabytes and a hundred thousand passes over it: both inside their
    // own ceilings, and days of work together.
    let parameters = dictionary(&[
        (b"$UUID", 0x42, ARGON2ID.to_vec()),
        (
            b"M",
            0x05,
            (4u64 * 1024 * 1024 * 1024).to_le_bytes().to_vec(),
        ),
        (b"I", 0x05, 100_000u64.to_le_bytes().to_vec()),
        (b"P", 0x04, 4u32.to_le_bytes().to_vec()),
        (b"S", 0x42, vec![0; 32]),
        (b"V", 0x04, 0x13u32.to_le_bytes().to_vec()),
    ]);
    let bytes = Header::kdbx4().field(11, &parameters).end();

    let path = write_bytes(scratch.path(), "slow.kdbx", &bytes);
    assert!(matches!(
        open_error(&path, SECRET),
        VaultError::AbsurdKeyDerivation
    ));
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
            .expose_str(),
        Some(long.as_str()),
        "a megabyte of text came back changed"
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
        open(&path, BUILT_PASSWORD).tree().entries[0].title.open(),
        Some("written elsewhere")
    );
}

#[test]
fn no_short_header_field_can_take_the_process_down() {
    // Every known KDBX 4 header field, given a value shorter than the parser
    // might read from it. The pre-flight stops what it can name; anything it
    // lets through has to come back as an error rather than a panic, which is
    // what the parse is wrapped for.
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut caught = 0;

    for id in 1..=12u8 {
        for length in [0usize, 1, 3, 7, 15] {
            let bytes = Header::kdbx4().field(id, &vec![0u8; length]).end();
            let path = write_bytes(scratch.path(), "short.kdbx", &bytes);

            if library_panics(&bytes) {
                caught += 1;
            }

            let error = open_error(&path, SECRET);
            assert!(
                matches!(
                    error,
                    VaultError::NotADatabase
                        | VaultError::UnsupportedFormat
                        | VaultError::DamagedHeader
                        | VaultError::DamagedPayload
                        | VaultError::DamagedContent
                        | VaultError::WrongCredentials
                        | VaultError::AbsurdKeyDerivation
                ),
                "header field {id} of {length} bytes gave {error:?}"
            );
        }
    }

    assert!(
        caught > 0,
        "none of these inputs panics the library any more, so this test is no longer testing anything"
    );
}
