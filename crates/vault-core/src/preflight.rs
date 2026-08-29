//! Reading the outer header before handing the file to the parser.
//!
//! KDBX puts the key derivation parameters in the clear, outside everything the
//! header HMAC covers, and the library derives the key from them before it
//! checks that HMAC. A file claiming four terabytes of Argon2 memory therefore
//! takes the process down before anybody has proved the file is genuine.
//!
//! This module reads the same bytes first, with no indexing and no arithmetic
//! that can wrap, and refuses anything it cannot vouch for. It is deliberately
//! stricter than the parser: a header this module accepts is one the parser can
//! walk without panicking, and a header it rejects never reaches the parser at
//! all.

use crate::error::VaultError;

/// Header field identifiers Coffer cares about. The rest are skipped by length.
const END_OF_HEADER: u8 = 0;
const KDF_PARAMETERS: u8 = 11;

/// The largest a single header field may claim to be. The whole outer header of
/// a real database is a few hundred bytes; the KDF seed is the biggest thing in
/// it.
const MAX_FIELD_BYTES: u32 = 64 * 1024;

/// Ceilings on what a database may ask Coffer to compute before it has proved
/// it is genuine. They are far above anything a real database carries and far
/// below anything that would take the process down.
const MAX_ARGON2_MEMORY: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARGON2_ITERATIONS: u64 = 100_000;
const MAX_PARALLELISM: u32 = 1024;
const MAX_AES_ROUNDS: u64 = 1_000_000_000;

/// Variant dictionary value tags, from the KDBX 4 specification.
const TAG_END: u8 = 0x00;
const TAG_U32: u8 = 0x04;
const TAG_U64: u8 = 0x05;
const TAG_BOOL: u8 = 0x08;
const TAG_I32: u8 = 0x0C;
const TAG_I64: u8 = 0x0D;
const TAG_STRING: u8 = 0x18;
const TAG_BYTES: u8 = 0x42;

/// Argon2 KDF identifiers. Anything else is AES, or unknown and left to the
/// parser to reject.
const KDF_ARGON2D: [u8; 16] = [
    0xef, 0x63, 0x6d, 0xdf, 0x8c, 0x29, 0x44, 0x4b, 0x91, 0xf7, 0xa9, 0xa4, 0x03, 0xe3, 0x0a, 0x0c,
];
const KDF_ARGON2ID: [u8; 16] = [
    0x9e, 0x29, 0x8b, 0x19, 0x56, 0xdb, 0x47, 0x73, 0xb2, 0x3d, 0xfc, 0x3e, 0xc6, 0xf0, 0xa1, 0xe6,
];

/// The size of the version block every KDBX file starts with.
const VERSION_BYTES: usize = 12;

/// Checks that a KDBX 4 file's key derivation parameters are ones Coffer is
/// willing to run before it has authenticated anything.
///
/// Files that are not KDBX 4 are left alone: the parser rejects KDBX 2 itself,
/// and KDBX 3 and KDB carry their round count in a fixed-width field rather than
/// a dictionary, with no allocation to run away with.
pub(crate) fn check_key_derivation(file: &[u8]) -> Result<(), VaultError> {
    let Some(body) = file.get(VERSION_BYTES..) else {
        return Err(VaultError::NotADatabase);
    };

    if !is_kdbx4(file) {
        return Ok(());
    }

    let mut cursor = Cursor::new(body);

    loop {
        let id = cursor.take_u8().ok_or(VaultError::DamagedHeader)?;
        let length = cursor.take_u32().ok_or(VaultError::DamagedHeader)?;

        if length > MAX_FIELD_BYTES {
            return Err(VaultError::DamagedHeader);
        }

        let data = cursor
            .take(length as usize)
            .ok_or(VaultError::DamagedHeader)?;

        if id == KDF_PARAMETERS {
            return check_dictionary(data);
        }

        if id == END_OF_HEADER {
            // A KDBX 4 file with no KDF parameters cannot be opened at all; the
            // parser says so more precisely than this module can.
            return Ok(());
        }
    }
}

fn is_kdbx4(file: &[u8]) -> bool {
    let (Some(minor), Some(major)) = (file.get(8..10), file.get(10..12)) else {
        return false;
    };
    let _ = minor;
    major == [4, 0]
}

fn check_dictionary(data: &[u8]) -> Result<(), VaultError> {
    let mut cursor = Cursor::new(data);
    let _version = cursor.take_u16().ok_or(VaultError::DamagedHeader)?;

    let mut uuid: Option<Vec<u8>> = None;
    let mut memory: Option<u64> = None;
    let mut iterations: Option<u64> = None;
    let mut parallelism: Option<u32> = None;
    let mut rounds: Option<u64> = None;

    loop {
        let tag = cursor.take_u8().ok_or(VaultError::DamagedHeader)?;
        if tag == TAG_END {
            break;
        }

        let key = cursor.take_sized().ok_or(VaultError::DamagedHeader)?;
        let value = cursor.take_sized().ok_or(VaultError::DamagedHeader)?;

        if !known_tag(tag) {
            return Err(VaultError::DamagedHeader);
        }

        match key {
            b"$UUID" => uuid = Some(value.to_vec()),
            b"M" => memory = unsigned(tag, value),
            b"I" => iterations = unsigned(tag, value),
            b"P" => parallelism = unsigned(tag, value).and_then(|v| u32::try_from(v).ok()),
            b"R" => rounds = unsigned(tag, value),
            _ => {}
        }
    }

    let argon2 = uuid
        .as_deref()
        .is_some_and(|id| id == KDF_ARGON2D || id == KDF_ARGON2ID);

    if argon2 {
        let memory = memory.ok_or(VaultError::DamagedHeader)?;
        let iterations = iterations.ok_or(VaultError::DamagedHeader)?;
        let parallelism = parallelism.ok_or(VaultError::DamagedHeader)?;

        if memory > MAX_ARGON2_MEMORY
            || iterations == 0
            || iterations > MAX_ARGON2_ITERATIONS
            || parallelism == 0
            || parallelism > MAX_PARALLELISM
        {
            return Err(VaultError::AbsurdKeyDerivation);
        }
    } else if let Some(rounds) = rounds
        && rounds > MAX_AES_ROUNDS
    {
        return Err(VaultError::AbsurdKeyDerivation);
    }

    Ok(())
}

fn known_tag(tag: u8) -> bool {
    matches!(
        tag,
        TAG_U32 | TAG_U64 | TAG_BOOL | TAG_I32 | TAG_I64 | TAG_STRING | TAG_BYTES
    )
}

/// Reads a dictionary value as an unsigned number, whichever width it was
/// written in. A negative signed value is refused rather than wrapped, because
/// wrapping is how a small number becomes an enormous one.
fn unsigned(tag: u8, value: &[u8]) -> Option<u64> {
    match tag {
        TAG_U32 | TAG_I32 => {
            let bytes: [u8; 4] = value.try_into().ok()?;
            match tag {
                TAG_U32 => Some(u64::from(u32::from_le_bytes(bytes))),
                _ => u64::try_from(i32::from_le_bytes(bytes)).ok(),
            }
        }
        TAG_U64 | TAG_I64 => {
            let bytes: [u8; 8] = value.try_into().ok()?;
            match tag {
                TAG_U64 => Some(u64::from_le_bytes(bytes)),
                _ => u64::try_from(i64::from_le_bytes(bytes)).ok(),
            }
        }
        _ => None,
    }
}

/// A reader over a byte slice that returns `None` instead of panicking.
struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.bytes.split_at_checked(count)?;
        self.bytes = tail;
        Some(head)
    }

    fn take_u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn take_u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn take_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// A length-prefixed run, as the variant dictionary writes its keys and
    /// values.
    fn take_sized(&mut self) -> Option<&'a [u8]> {
        let length = self.take_u32()?;
        if length > MAX_FIELD_BYTES {
            return None;
        }
        self.take(length as usize)
    }
}
