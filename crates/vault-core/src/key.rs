//! The credentials that open a database.

use std::fmt;
use std::path::Path;

use constant_time_eq::constant_time_eq_32;
use keepass::DatabaseKey;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::VaultError;

/// A master password, and optionally a key file, on their way to key
/// derivation.
///
/// The password arrives as raw bytes rather than a `String` because that is how
/// it crosses the process boundary: a `Uint8Array` over IPC, wrapped here
/// without an intermediate JSON copy. The buffer is wiped when this value is
/// dropped.
pub struct MasterKey {
    password: Zeroizing<Vec<u8>>,
    key_file: Option<Zeroizing<Vec<u8>>>,
    /// Whether the password is left out of the composite key entirely.
    ///
    /// A vault whose owner chose a key file and no password at all is opened by
    /// the file alone, and one whose owner chose a key file and an empty
    /// password is opened by both. Nothing in a KDBX header says which of the
    /// two a file is, so the only way to tell them apart is to try one and then
    /// the other.
    alone: bool,
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MasterKey")
            .field("password", &"[redacted]")
            .field("key_file", &self.key_file.as_ref().map(|_| "[redacted]"))
            .field("alone", &self.alone)
            .finish()
    }
}

/// The password a key held before it was replaced, kept until the write under
/// the new one has answered, so that a change the file did not take can be put
/// back. Wiped when it goes.
pub(crate) struct Former {
    password: Zeroizing<Vec<u8>>,
    alone: bool,
}

impl fmt::Debug for Former {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Former")
            .field("password", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// A password's SHA-256, wiped when it goes. The digest is itself one of the
/// elements KeePass derives the key from, so it is as much a secret as the
/// password it was made of.
fn digest(bytes: &[u8]) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(Sha256::digest(bytes).into())
}

impl MasterKey {
    /// Takes ownership of the password bytes. An empty password is a valid
    /// credential: some databases have one.
    pub fn from_password(password: Zeroizing<Vec<u8>>) -> Self {
        MasterKey {
            password,
            key_file: None,
            alone: false,
        }
    }

    /// Whether there is no password at all.
    ///
    /// An empty password opens a database that has one, so this is never asked
    /// on the way in. It is asked when a vault is being made, where a file
    /// anybody can open is not a vault.
    pub fn is_empty(&self) -> bool {
        self.password.is_empty() && self.key_file.is_none()
    }

    /// Adds a key file. Coffer reads databases that use one and never creates
    /// one.
    pub fn with_key_file(mut self, path: &Path) -> Result<Self, VaultError> {
        self.key_file = Some(Zeroizing::new(std::fs::read(path)?));
        Ok(self)
    }

    /// Leaves the password out of the composite key, and says whether that was
    /// a thing this key could still do.
    ///
    /// Only ever the second attempt at one file. A key file with a password
    /// somebody typed has one composition and no alternative, and so has a
    /// password with no key file, so both answer `false` and the refusal they
    /// were given stands.
    pub(crate) fn stand_alone(&mut self) -> bool {
        if self.alone || self.key_file.is_none() || !self.password.is_empty() {
            return false;
        }
        self.alone = true;
        true
    }

    /// Whether `typed` is this key's password.
    ///
    /// Both are hashed and the two digests compared in constant time, so how
    /// long the answer takes says nothing about how much of what was typed was
    /// right. Hashed first because the comparison the crate offers is constant
    /// only between two values of one length, and two passwords compared as
    /// they are would answer a wrong length sooner than a wrong letter. A key
    /// used with its key file alone holds no password, and the empty password
    /// is what matches it.
    pub(crate) fn password_is(&self, typed: &[u8]) -> bool {
        constant_time_eq_32(&digest(&self.password), &digest(typed))
    }

    /// Puts `password` in place of the one this key holds, beside the same key
    /// file, and hands back the old one. A key that was a key file alone has a
    /// password beside its key file from now on.
    pub(crate) fn replace_password(&mut self, password: Zeroizing<Vec<u8>>) -> Former {
        Former {
            password: std::mem::replace(&mut self.password, password),
            alone: std::mem::replace(&mut self.alone, false),
        }
    }

    /// Puts back what [`MasterKey::replace_password`] took. The password it
    /// replaced is wiped.
    pub(crate) fn restore(&mut self, former: Former) {
        self.password = former.password;
        self.alone = former.alone;
    }

    /// Builds the crate's key type.
    ///
    /// The password has to become a `&str` here because that is the only way in.
    /// It is borrowed rather than copied, and the `DatabaseKey` that owns the
    /// copy it makes zeroizes it on drop.
    pub(crate) fn to_database_key(&self) -> Result<DatabaseKey, VaultError> {
        let key = if self.alone {
            DatabaseKey::new()
        } else {
            let password =
                std::str::from_utf8(&self.password).map_err(|_| VaultError::PasswordNotUtf8)?;
            DatabaseKey::new().with_password(password)
        };

        match &self.key_file {
            None => Ok(key),
            Some(bytes) => Ok(key.with_keyfile(&mut bytes.as_slice())?),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use keepass::Database;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn key(password: &[u8]) -> MasterKey {
        MasterKey::from_password(Zeroizing::new(password.to_vec()))
    }

    /// The question a change of password asks of the key before anything else,
    /// and the only thing standing between a window left unlocked and a vault
    /// somebody else gave a password of their own. Nothing short of the whole
    /// password is the password, however much of it is right.
    #[test]
    fn a_password_is_matched_whole_and_nothing_less() -> Result<(), VaultError> {
        let held = key(b"correct horse");
        assert!(held.password_is(b"correct horse"));
        for (what, typed) in [
            ("a prefix", &b"correct hors"[..]),
            ("a suffix", b"orrect horse"),
            ("one byte more", b"correct horse!"),
            ("one byte changed", b"correct horsf"),
            ("nothing", b""),
            ("the same letters in another case", b"Correct Horse"),
        ] {
            assert!(
                !held.password_is(typed),
                "{what} was taken for the password"
            );
        }
        assert!(
            !key(b"").password_is(b"x"),
            "something was taken for nothing"
        );

        let long = vec![b'a'; 10 * 1024 * 1024];
        let mut off = long.clone();
        if let Some(last) = off.last_mut() {
            *last = b'b';
        }
        assert!(key(&long).password_is(&long));
        assert!(
            !key(&long).password_is(&off),
            "ten megabytes one byte off were taken for the password"
        );

        // A vault opened by its key file holds no password, and only the
        // empty one is it.
        let file = key(b"").with_key_file(&fixture("keyfile.key"))?;
        assert!(file.password_is(b""));
        assert!(!file.password_is(b" "));
        Ok(())
    }

    /// Replacing the password touches the password and nothing else, and what
    /// it hands back puts the key exactly as it was - the key file beside it,
    /// and whether the password was part of the key at all.
    #[test]
    fn a_replaced_password_can_be_put_back_and_the_key_file_never_moves() -> Result<(), VaultError>
    {
        let both = std::fs::read(fixture("keyfile-kdbx41.kdbx"))?;
        let mut held = key(b"coffer-keyfile").with_key_file(&fixture("keyfile.key"))?;

        let former = held.replace_password(Zeroizing::new(b"new".to_vec()));
        assert!(held.password_is(b"new"));
        assert!(!held.password_is(b"coffer-keyfile"));
        assert!(
            Database::parse(&both, held.to_database_key()?).is_err(),
            "the old file opened with the new password"
        );

        held.restore(former);
        assert!(held.password_is(b"coffer-keyfile"));
        Database::parse(&both, held.to_database_key()?)?;

        // A key file alone: the replacement puts a password beside it, and
        // the restore takes it out of the key again.
        let alone = std::fs::read(fixture("keyfile-only-kdbx41.kdbx"))?;
        let mut held = key(b"").with_key_file(&fixture("keyfile-only.key"))?;
        assert!(held.stand_alone());
        Database::parse(&alone, held.to_database_key()?)?;

        let former = held.replace_password(Zeroizing::new(b"new".to_vec()));
        assert!(
            Database::parse(&alone, held.to_database_key()?).is_err(),
            "the key file alone still made the key after a password was put beside it"
        );
        held.restore(former);
        Database::parse(&alone, held.to_database_key()?)?;
        Ok(())
    }

    #[test]
    fn nothing_a_key_or_its_former_password_prints_names_either() {
        let mut held = key(b"the one it had");
        let former = held.replace_password(Zeroizing::new(b"the one it has".to_vec()));

        for printed in [format!("{held:?}"), format!("{former:?}")] {
            assert!(printed.contains("[redacted]"), "{printed}");
            assert!(
                !printed.contains("the one it had") && !printed.contains("the one it has"),
                "a password was printed"
            );
        }
    }
}
