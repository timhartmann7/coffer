//! The one type a secret is allowed to leave the vault in.

use std::fmt;

use zeroize::Zeroizing;

/// A value revealed out of the vault: a protected field, or the bytes of an
/// attachment.
///
/// It is deliberately hard to misuse. There is no `Clone`, so a secret cannot be
/// duplicated by accident; no `Display` and no `Serialize`, so it cannot be
/// formatted into a log line or an IPC payload; and `Debug` prints `[redacted]`,
/// so a struct that contains one is safe to print. The buffer is wiped when the
/// value is dropped.
pub struct SecretValue(Zeroizing<Vec<u8>>);

impl SecretValue {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        SecretValue(Zeroizing::new(bytes))
    }

    /// Hands out the bytes. Every caller of this is a place where a secret can
    /// escape, so there should be very few of them.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Hands out the bytes as text, or `None` when they are not UTF-8. Field
    /// values are text; attachments are not.
    pub fn expose_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}
