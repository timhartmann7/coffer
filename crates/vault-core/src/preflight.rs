//! Reading the outer header before handing the file to the parser.
//!
//! KDBX puts the key derivation parameters in the clear, outside everything the
//! header signature covers, and the library derives the key from them before it
//! checks that signature. A file claiming four terabytes of Argon2 memory
//! therefore takes the process down before anybody has proved the file is
//! genuine, and a header field one byte short of what the parser reads panics
//! it in the same window.
//!
//! This module reads the same bytes first, with no indexing and no arithmetic
//! that can wrap, and refuses anything it cannot vouch for. It is deliberately
//! stricter than the parser: a header this module accepts is one the parser can
//! walk without panicking, and a header it rejects never reaches the parser at
//! all.

use crate::error::VaultError;
use crate::kdf::Work;

/// The four bytes every KeePass file begins with.
const SIGNATURE: [u8; 4] = [0x03, 0xd9, 0xa2, 0x9a];

/// The size of the version block, which the signature is part of.
const VERSION_BYTES: usize = 12;

/// Header field identifiers. KDBX 3 and KDBX 4 number them differently, so each
/// format has its own table.
mod kdbx4 {
    pub(super) const END: u8 = 0;
    pub(super) const COMPRESSION_ID: u8 = 3;
    pub(super) const KDF_PARAMETERS: u8 = 11;
    pub(super) const PUBLIC_CUSTOM_DATA: u8 = 12;
    pub(super) const HIGHEST_KNOWN: u8 = 12;
}

mod kdbx3 {
    pub(super) const END: u8 = 0;
    pub(super) const COMPRESSION_ID: u8 = 3;
    pub(super) const TRANSFORM_ROUNDS: u8 = 6;
    pub(super) const INNER_RANDOM_STREAM_ID: u8 = 10;
    pub(super) const HIGHEST_KNOWN: u8 = 10;
}

/// The largest a single header field may claim to be. The whole outer header of
/// a real database is a few hundred bytes; the key derivation seed is the
/// biggest thing in it.
const MAX_FIELD_BYTES: u32 = 64 * 1024;

/// Ceilings on what a database may ask Coffer to compute before it has proved
/// it is genuine. They are far above anything a real database carries and far
/// below anything that would take the process down.
/// A gigabyte, which is where KeePassXC's own settings screen stops. Real
/// databases ask for sixty-four megabytes.
const MAX_ARGON2_MEMORY: u64 = 1024 * 1024 * 1024;
const MAX_ARGON2_ITERATIONS: u64 = 100_000;
const MAX_PARALLELISM: u32 = 1024;
const MAX_AES_ROUNDS: u64 = 1_000_000_000;

/// Memory in bytes multiplied by iterations. Each ceiling on its own leaves a
/// corner where both are legal and the derivation still takes days.
const MAX_ARGON2_WORK: u64 = 64 * 1024 * 1024 * 1024;

/// Variant dictionary value tags, from the KDBX 4 specification, with the width
/// the parser reads for each. A `None` width is a run of bytes of any length.
const TAG_END: u8 = 0x00;
const TAGS: [(u8, Option<usize>); 7] = [
    (0x04, Some(4)), // u32
    (0x05, Some(8)), // u64
    (0x08, Some(1)), // bool
    (0x0c, Some(4)), // i32
    (0x0d, Some(8)), // i64
    (0x18, None),    // string
    (0x42, None),    // byte array
];

const TAG_U32: u8 = 0x04;
const TAG_U64: u8 = 0x05;
const TAG_I32: u8 = 0x0c;
const TAG_I64: u8 = 0x0d;

/// Argon2 key derivation identifiers. Anything else is AES, or unknown and left
/// to the parser to reject.
const KDF_ARGON2D: [u8; 16] = [
    0xef, 0x63, 0x6d, 0xdf, 0x8c, 0x29, 0x44, 0x4b, 0x91, 0xf7, 0xa9, 0xa4, 0x03, 0xe3, 0x0a, 0x0c,
];
const KDF_ARGON2ID: [u8; 16] = [
    0x9e, 0x29, 0x8b, 0x19, 0x56, 0xdb, 0x47, 0x73, 0xb2, 0x3d, 0xfc, 0x3e, 0xc6, 0xf0, 0xa1, 0xe6,
];

/// Whether key derivation parameters are ones Coffer is willing to run.
///
/// The one rule, called from both sides of it: the check every file passes on
/// its way in, and the check the calibration's answer passes on its way out. A
/// database Coffer wrote and then refused to open would be a vault nobody could
/// get back into, and it is this function being shared that makes that
/// impossible rather than unlikely.
pub(crate) fn acceptable(work: Work) -> Result<(), VaultError> {
    let total = work.memory.saturating_mul(work.iterations);

    if work.memory > MAX_ARGON2_MEMORY
        || work.iterations == 0
        || work.iterations > MAX_ARGON2_ITERATIONS
        || work.parallelism == 0
        || work.parallelism > MAX_PARALLELISM
        || total > MAX_ARGON2_WORK
    {
        return Err(VaultError::AbsurdKeyDerivation);
    }

    Ok(())
}

/// The most passes Coffer will accept at this much memory.
///
/// The product ceiling binds long before the count ceiling does: at sixty-four
/// megabytes it is a thousand and twenty-four passes, and at a gigabyte it is
/// sixty-four.
pub(crate) fn max_iterations(memory: u64) -> u64 {
    MAX_ARGON2_ITERATIONS.min(MAX_ARGON2_WORK / memory.max(1))
}

/// Checks that a file's outer header is one the parser can walk safely, and
/// that its key derivation parameters are ones Coffer is willing to run before
/// it has authenticated anything.
pub(crate) fn check(file: &[u8]) -> Result<(), VaultError> {
    if file.get(..SIGNATURE.len()) != Some(&SIGNATURE[..]) {
        // Not a KeePass file at all. The parser says so more precisely.
        return Ok(());
    }

    let Some(body) = file.get(VERSION_BYTES..) else {
        return Ok(());
    };

    match file.get(10..12) {
        Some([4, 0]) => walk(body, Format::Kdbx4),
        Some([3, 0]) => walk(body, Format::Kdbx3),
        // KDBX 1 and the KeePass 2 pre-release layout have no header of this
        // shape, and the parser refuses both.
        _ => Ok(()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Kdbx3,
    Kdbx4,
}

impl Format {
    /// KDBX 3 states a field's length in two bytes and KDBX 4 in four. Reading
    /// one as the other walks the header into nonsense.
    fn take_length(self, cursor: &mut Cursor<'_>) -> Option<u32> {
        match self {
            Format::Kdbx3 => cursor.take_u16().map(u32::from),
            Format::Kdbx4 => cursor.take_u32(),
        }
    }

    fn end(self) -> u8 {
        match self {
            Format::Kdbx3 => kdbx3::END,
            Format::Kdbx4 => kdbx4::END,
        }
    }

    fn highest_known(self) -> u8 {
        match self {
            Format::Kdbx3 => kdbx3::HIGHEST_KNOWN,
            Format::Kdbx4 => kdbx4::HIGHEST_KNOWN,
        }
    }

    /// The width the parser reads for a field, when it reads a fixed one. A
    /// field narrower than this panics the parser before authentication.
    fn fixed_width(self, id: u8) -> Option<usize> {
        match (self, id) {
            (Format::Kdbx4, kdbx4::COMPRESSION_ID) => Some(4),
            (Format::Kdbx3, kdbx3::COMPRESSION_ID) => Some(4),
            (Format::Kdbx3, kdbx3::TRANSFORM_ROUNDS) => Some(8),
            (Format::Kdbx3, kdbx3::INNER_RANDOM_STREAM_ID) => Some(4),
            _ => None,
        }
    }
}

/// Walks every field of an outer header.
///
/// Every field is checked, not just the ones with a ceiling. The parser reads
/// the whole header before it derives anything, so a field it cannot read is a
/// crash whatever else the file says. And every key derivation dictionary is
/// checked rather than the first: a second one overwrites the first inside the
/// parser, so checking only the first would let a file state safe parameters
/// and then state the real ones.
fn walk(body: &[u8], format: Format) -> Result<(), VaultError> {
    let mut cursor = Cursor::new(body);

    loop {
        let id = cursor.take_u8().ok_or(VaultError::DamagedHeader)?;
        let length = format
            .take_length(&mut cursor)
            .ok_or(VaultError::DamagedHeader)?;

        if length > MAX_FIELD_BYTES {
            return Err(VaultError::DamagedHeader);
        }

        let data = cursor
            .take(usize::try_from(length).map_err(|_| VaultError::DamagedHeader)?)
            .ok_or(VaultError::DamagedHeader)?;

        if id > format.highest_known() {
            // The parser refuses an unknown field, so there is nothing here to
            // protect it from.
            return Ok(());
        }

        if let Some(width) = format.fixed_width(id)
            && data.len() < width
        {
            return Err(VaultError::DamagedHeader);
        }

        if format == Format::Kdbx3 && id == kdbx3::TRANSFORM_ROUNDS {
            let rounds = u64::from_le_bytes(
                data.get(..8)
                    .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
                    .ok_or(VaultError::DamagedHeader)?,
            );
            if rounds > MAX_AES_ROUNDS {
                return Err(VaultError::AbsurdKeyDerivation);
            }
        }

        if format == Format::Kdbx4
            && matches!(id, kdbx4::KDF_PARAMETERS | kdbx4::PUBLIC_CUSTOM_DATA)
        {
            let dictionary = read_dictionary(data)?;
            if id == kdbx4::KDF_PARAMETERS {
                check_key_derivation(&dictionary)?;
            }
        }

        if id == format.end() {
            return Ok(());
        }
    }
}

/// The entries of a variant dictionary, checked for everything the parser reads
/// with a fixed width.
struct Dictionary<'a> {
    entries: Vec<(&'a [u8], u8, &'a [u8])>,
}

fn read_dictionary(data: &[u8]) -> Result<Dictionary<'_>, VaultError> {
    let mut cursor = Cursor::new(data);
    let _version = cursor.take_u16().ok_or(VaultError::DamagedHeader)?;

    let mut entries = Vec::new();

    loop {
        let tag = cursor.take_u8().ok_or(VaultError::DamagedHeader)?;
        if tag == TAG_END {
            return Ok(Dictionary { entries });
        }

        let key = cursor.take_sized().ok_or(VaultError::DamagedHeader)?;
        let value = cursor.take_sized().ok_or(VaultError::DamagedHeader)?;

        let Some((_, width)) = TAGS.iter().find(|(known, _)| *known == tag) else {
            return Err(VaultError::DamagedHeader);
        };

        // A numeric entry narrower than its type panics the parser.
        if let Some(width) = width
            && value.len() < *width
        {
            return Err(VaultError::DamagedHeader);
        }

        entries.push((key, tag, value));
    }
}

fn check_key_derivation(dictionary: &Dictionary<'_>) -> Result<(), VaultError> {
    let mut uuid: Option<&[u8]> = None;
    let mut memory: Option<u64> = None;
    let mut iterations: Option<u64> = None;
    let mut parallelism: Option<u32> = None;
    let mut rounds: Option<u64> = None;

    for (key, tag, value) in &dictionary.entries {
        match *key {
            b"$UUID" => uuid = Some(value),
            b"M" => memory = unsigned(*tag, value),
            b"I" => iterations = unsigned(*tag, value),
            b"P" => parallelism = unsigned(*tag, value).and_then(|v| u32::try_from(v).ok()),
            b"R" => rounds = unsigned(*tag, value),
            _ => {}
        }
    }

    let argon2 = uuid.is_some_and(|id| id == KDF_ARGON2D || id == KDF_ARGON2ID);

    if argon2 {
        acceptable(Work {
            memory: memory.ok_or(VaultError::DamagedHeader)?,
            iterations: iterations.ok_or(VaultError::DamagedHeader)?,
            parallelism: parallelism.ok_or(VaultError::DamagedHeader)?,
        })?;
    } else if let Some(rounds) = rounds
        && rounds > MAX_AES_ROUNDS
    {
        return Err(VaultError::AbsurdKeyDerivation);
    }

    Ok(())
}

/// Reads a dictionary value as an unsigned number, whichever width it was
/// written in. A negative signed value is refused rather than wrapped, because
/// wrapping is how a small number becomes an enormous one.
fn unsigned(tag: u8, value: &[u8]) -> Option<u64> {
    match tag {
        TAG_U32 => Some(u64::from(u32::from_le_bytes(
            value.get(..4)?.try_into().ok()?,
        ))),
        TAG_I32 => u64::try_from(i32::from_le_bytes(value.get(..4)?.try_into().ok()?)).ok(),
        TAG_U64 => Some(u64::from_le_bytes(value.get(..8)?.try_into().ok()?)),
        TAG_I64 => u64::try_from(i64::from_le_bytes(value.get(..8)?.try_into().ok()?)).ok(),
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
        self.take(usize::try_from(length).ok()?)
    }
}
