//! The credentials that open a database.

use std::fmt;
use std::path::Path;

use keepass::DatabaseKey;
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
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MasterKey")
            .field("password", &"[redacted]")
            .field("key_file", &self.key_file.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl MasterKey {
    /// Takes ownership of the password bytes. An empty password is a valid
    /// credential: some databases have one.
    pub fn from_password(password: Zeroizing<Vec<u8>>) -> Self {
        MasterKey {
            password,
            key_file: None,
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

    /// Builds the crate's key type.
    ///
    /// The password has to become a `&str` here because that is the only way in.
    /// It is borrowed rather than copied, and the `DatabaseKey` that owns the
    /// copy it makes zeroizes it on drop.
    pub(crate) fn to_database_key(&self) -> Result<DatabaseKey, VaultError> {
        let password =
            std::str::from_utf8(&self.password).map_err(|_| VaultError::PasswordNotUtf8)?;
        let key = DatabaseKey::new().with_password(password);

        match &self.key_file {
            None => Ok(key),
            Some(bytes) => Ok(key.with_keyfile(&mut bytes.as_slice())?),
        }
    }
}
