//! Shared machinery: where the fixtures are, how to drive keepassxc-cli, and
//! how to compare two exports of the same database.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use keepass::db::{
    AutoType, AutoTypeAssociation, Color, CustomDataItem, CustomDataValue, DataTransferObfuscation,
    Icon, Value,
};
use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

use vault_core::kind::Kind;
use vault_core::model::{EntryId, GroupId, Project, fields};
use vault_core::{Attached, LockPolicy, MasterKey, NewValue, Vault};

/// Set this to run the suite on a machine with no KeePassXC. Everything that
/// needs an external implementation is skipped, and the round-trip criterion
/// goes unproven, so CI never sets it.
const SKIP_VARIABLE: &str = "COFFER_SKIP_KEEPASSXC";

/// The fixture with the whole feature matrix, and the password every fixture
/// but the key-file one uses.
pub const RICH: &str = "rich-kdbx41.kdbx";
pub const SECRET: &str = "coffer-test";

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn fixture(name: &str) -> PathBuf {
    fixtures().join(name)
}

pub fn password(text: &str) -> MasterKey {
    MasterKey::from_password(Zeroizing::new(text.as_bytes().to_vec()))
}

/// Copies a fixture into a scratch directory so that a test can save over it.
pub fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let target = directory.path().join(name);
    std::fs::copy(fixture(name), &target).expect("the fixture copies");
    (directory, target)
}

pub fn open(path: &Path, secret: &str) -> Vault {
    Vault::open(path, password(secret), LockPolicy::Respect).expect("the database opens")
}

/// The vault saved, let go of, and opened again from the file: what the next
/// launch would find.
pub fn reopened(mut vault: Vault, secret: &str) -> Vault {
    vault.save().expect("the database saves");
    let path = vault.path().to_owned();
    drop(vault);
    open(&path, secret)
}

/// Puts a file on an entry under a name the entry does not use yet.
///
/// Most tests that add a file are about something else, and a name that turned
/// out to be taken adds nothing at all: the test would go on to prove its point
/// about a file that is not there.
pub fn attach(vault: &mut Vault, id: EntryId, name: &str, data: &[u8]) {
    let attached = vault
        .add_attachment(id, name, data)
        .unwrap_or_else(|error| panic!("{name} could not be added: {error}"));
    assert_eq!(attached, Attached::Added, "{name} was already on the entry");
}

/// Every entry's file, by the title of the entry that holds it, with the bytes
/// behind it. Byte for byte, because the failure the pool is prone to is not a
/// file that vanishes but one that comes back on somebody else's entry.
pub fn files(vault: &Vault) -> Vec<(String, String, Vec<u8>)> {
    let mut found = Vec::new();
    for summary in all_entries(vault) {
        let entry = vault.entry(summary.id).expect("the entry is there");
        let title = entry
            .field(vault_core::model::fields::TITLE)
            .and_then(|field| field.value.open())
            .unwrap_or_default()
            .to_owned();

        for attachment in &entry.attachments {
            let bytes = vault
                .attachment(entry.id, &attachment.name)
                .expect("the file is there");
            found.push((
                title.clone(),
                attachment.name.clone(),
                bytes.expose().to_vec(),
            ));
        }
    }
    found.sort();
    found
}

/// Every entry in the database, flattened, with previous versions excluded the
/// way the tree excludes them.
pub fn all_entries(vault: &Vault) -> Vec<vault_core::model::EntrySummary> {
    fn walk(group: &vault_core::model::Project, into: &mut Vec<vault_core::model::EntrySummary>) {
        into.extend(group.entries.iter().cloned());
        for section in &group.sections {
            walk(section, into);
        }
    }

    let mut entries = Vec::new();
    walk(&vault.tree(), &mut entries);
    entries
}

/// The one entry with this title, in full. Panics when there is not exactly
/// one, so that a fixture change cannot quietly make a test assert nothing.
pub fn entry_titled(vault: &Vault, title: &str) -> vault_core::model::Entry {
    let mut found: Vec<_> = all_entries(vault)
        .into_iter()
        .filter(|entry| entry.title.open() == Some(title))
        .collect();

    assert_eq!(
        found.len(),
        1,
        "expected exactly one entry titled {title:?}"
    );
    vault
        .entry(found.remove(0).id)
        .expect("the entry the tree named is in the database")
}

/// The folder with this id, wherever it sits in the tree.
pub fn folder(tree: &Project, id: GroupId) -> Option<&Project> {
    if tree.id == id {
        return Some(tree);
    }
    tree.sections.iter().find_map(|section| folder(section, id))
}

/// The folder holding a folder, wherever the two sit in the tree.
pub fn holder(tree: &Project, id: GroupId) -> Option<GroupId> {
    if tree.sections.iter().any(|section| section.id == id) {
        return Some(tree.id);
    }
    tree.sections.iter().find_map(|section| holder(section, id))
}

/// The file as the library reads it, with nothing of Coffer's in between,
/// which is what another client finds there.
pub fn library(path: &Path, secret: &str) -> Database {
    let mut file = std::fs::File::open(path).expect("the file opens");
    Database::open(&mut file, DatabaseKey::new().with_password(secret))
        .expect("the library reads it")
}

/// An entry with a title, made where it is asked for.
pub fn made(vault: &mut Vault, group: GroupId, title: &str) -> EntryId {
    let id = vault
        .create_entry(group, Kind::Login)
        .expect("the entry is made");
    vault
        .set_field(id, fields::TITLE, NewValue::Open(title.to_owned()))
        .expect("the title is written");
    id
}

/// Everything an entry holds but its files, its place and its dates, as the
/// library reads it: what a restore brings back and a copy carries. Built from
/// the library's own types, so a value compares with its protection.
#[derive(Debug, PartialEq)]
pub struct Holding {
    pub fields: std::collections::HashMap<String, Value<String>>,
    pub tags: Vec<String>,
    pub custom_data: std::collections::HashMap<String, CustomDataItem>,
    pub autotype: Option<AutoType>,
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub override_url: Option<String>,
    pub quality_check: bool,
    pub expires: Option<bool>,
    pub expiry: Option<chrono::NaiveDateTime>,
    pub icon: Option<Icon>,
}

pub fn holding(entry: &keepass::db::Entry) -> Holding {
    Holding {
        fields: entry.fields.clone(),
        tags: entry.tags.clone(),
        custom_data: entry.custom_data.clone(),
        autotype: entry.autotype.clone(),
        foreground_color: entry.foreground_color.clone(),
        background_color: entry.background_color.clone(),
        override_url: entry.override_url.clone(),
        quality_check: entry.quality_check,
        expires: entry.times.expires,
        expiry: entry.times.expiry,
        icon: entry.icon().cloned(),
    }
}

/// Gives an entry one of everything it can hold beside its fields and files,
/// each set away from the library's default, so that one left behind on the
/// way somewhere is missed: custom data, an auto-type association, both
/// colours, an override URL, the quality check off, an expiry, a tag and a
/// custom icon.
pub fn furnish(entry: &mut keepass::db::EntryMut<'_>, word: &str) {
    entry.tags = vec![word.to_owned()];
    entry.custom_data.insert(
        "Coffer.Test".to_owned(),
        CustomDataItem {
            value: Some(CustomDataValue::String(word.to_owned())),
            last_modification_time: None,
        },
    );
    entry.autotype = Some(AutoType {
        enabled: true,
        default_sequence: Some("{USERNAME}{TAB}{PASSWORD}".to_owned()),
        data_transfer_obfuscation: DataTransferObfuscation::UseClipboard,
        associations: vec![AutoTypeAssociation {
            window: format!("{word} - Browser"),
            sequence: "{PASSWORD}".to_owned(),
        }],
    });
    entry.foreground_color = Some("#FF0000".parse().expect("a colour"));
    entry.background_color = Some("#00FF00".parse().expect("a colour"));
    entry.override_url = Some(format!("cmd://open {word}"));
    entry.quality_check = false;
    entry.times.expires = Some(true);
    entry.times.expiry =
        chrono::NaiveDate::from_ymd_opt(2030, 1, 2).and_then(|day| day.and_hms_opt(3, 4, 5));
    entry.set_icon_custom_new(word.as_bytes().to_vec());
}

/// An empty vault of [`built`]'s, in a scratch directory of its own.
pub fn cheap(name: &str) -> (tempfile::TempDir, PathBuf) {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), name, |_| {});
    (scratch, path)
}

/// The one folder with this name, wherever it sits.
pub fn only_group(vault: &Vault, name: &str) -> vault_core::model::GroupId {
    fn walk(
        group: &vault_core::model::Project,
        name: &str,
        into: &mut Vec<vault_core::model::GroupId>,
    ) {
        if group.name == name {
            into.push(group.id);
        }
        for section in &group.sections {
            walk(section, name, into);
        }
    }

    let mut found = Vec::new();
    walk(&vault.tree(), name, &mut found);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one folder called {name:?}"
    );
    found.remove(0)
}

/// Builds a database in the scratch directory with a deliberately cheap key
/// derivation.
///
/// Key derivation is nearly the whole cost of a save, and the cheapest thing
/// `keepassxc-cli` can produce is a million AES rounds, which is about 370 ms
/// per save in a debug build. A test that saves a thousand times needs a
/// database that costs nothing to open, and only a test that measures fidelity
/// needs a database KeePassXC wrote.
pub fn built(directory: &Path, name: &str, shape: impl FnOnce(&mut Database)) -> PathBuf {
    built_with(directory, name, BUILT_PASSWORD, shape)
}

/// As [`built`], with a password of the caller's choosing.
pub fn built_with(
    directory: &Path,
    name: &str,
    secret: &str,
    shape: impl FnOnce(&mut Database),
) -> PathBuf {
    use keepass::config::KdfConfig;

    let mut database = Database::new();
    database.config.kdf_config = KdfConfig::Aes { rounds: 16 };
    shape(&mut database);

    let path = directory.join(name);
    let mut file = std::fs::File::create(&path).expect("the database file is created");
    database
        .save(&mut file, DatabaseKey::new().with_password(secret))
        .expect("the database saves");

    path
}

/// The password every database `built` produces uses.
pub const BUILT_PASSWORD: &str = "built";

/// Puts an extended attribute on a file, leaving its length, its inode and
/// every byte of its contents alone. That is what the callers need: something a
/// system does to a file behind the user's back, which a vault must not read as
/// another client's edit.
///
/// The syscall rather than a command: `xattr` is a Mac's tool and the engine's
/// suite runs on Linux, where it is not installed under any name. The name is
/// this module's business rather than the caller's, because the two platforms
/// do not accept the same ones - Linux allows only the `user.` namespace on an
/// ordinary filesystem, and rejects everything else with `EOPNOTSUPP`. What the
/// tests need is any attribute at all.
pub fn set_attribute(path: &Path) {
    let path = CString::new(path.as_os_str().as_bytes()).expect("a path with no null in it");

    // On a Mac, one the system itself writes onto a file an application has
    // touched through a save panel; on Linux, the only namespace a test may
    // write to.
    #[cfg(target_os = "macos")]
    let name = c"com.apple.provenance";
    #[cfg(not(target_os = "macos"))]
    let name = c"user.coffer.provenance";

    let value = b"whatever the system says";

    // SAFETY: both pointers are to buffers alive for the call, and the length
    // is the value's own.
    let written = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        }
    };

    // Not a skip. A filesystem that refused the attribute is one where these
    // tests prove nothing, and passing quietly would be worse than stopping.
    assert_eq!(
        written,
        0,
        "the attribute was refused: {}",
        std::io::Error::last_os_error()
    );

    // And read back, because a call that returned zero is not yet an attribute
    // on a file. Asking for the length rather than the value on purpose: macOS
    // owns `com.apple.provenance` and stores eleven bytes of its own whatever
    // it is handed, so the value that lands is not the value written and only
    // its presence is the caller's business.
    //
    // SAFETY: the path outlives the call, and a null destination with a length
    // of zero is how both platforms are asked for the size alone.
    let present = unsafe {
        #[cfg(target_os = "macos")]
        {
            libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0)
        }
    };

    assert!(
        present >= 0,
        "the attribute was accepted and is not on the file: {}",
        std::io::Error::last_os_error()
    );
}

/// Puts `from`'s modification time onto `to`, which is what a client that
/// wanted to hide a write would do.
///
/// To the nanosecond, because that is the resolution the stamp compares at: a
/// second-granular `touch -r` would leave the two files differing in the one
/// field this is trying to make identical.
pub fn copy_modification_time(from: &Path, to: &Path) {
    let when = std::fs::metadata(from).expect("the file to copy the time from is there");

    // The access time is left where it is. `utimensat` takes both or neither,
    // and `UTIME_OMIT` is how it is told to keep one - which is what the name
    // of this function promises, and one fewer difference between the file a
    // test is describing and the file a client would have left.
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: when.mtime() as _,
            tv_nsec: when.mtime_nsec() as _,
        },
    ];
    let to = CString::new(to.as_os_str().as_bytes()).expect("a path with no null in it");

    // SAFETY: the path and the two times are alive for the call, and the array
    // is the pair the call expects.
    let done = unsafe { libc::utimensat(libc::AT_FDCWD, to.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(
        done,
        0,
        "the modification time was not copied: {}",
        std::io::Error::last_os_error()
    );
}

/// Whether this process can be stopped by file permissions at all. Running as
/// root makes every permission test pass without testing anything.
pub fn permissions_apply() -> bool {
    // SAFETY: geteuid reads a process property and touches nothing.
    unsafe { libc::geteuid() != 0 }
}

/// A directory that will not take a new file, thawed when this is dropped.
///
/// Thawed on the way out rather than by the test, because a test that asserts
/// while the directory is frozen and fails leaves a `TempDir` whose own `Drop`
/// cannot remove it: the failure is then followed by a directory left behind in
/// the system's temporary space, and the assertion nobody sees is the one that
/// mattered.
pub struct Frozen(PathBuf);

impl Frozen {
    pub fn over(directory: &Path) -> Frozen {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o500))
            .expect("the directory is frozen");
        Frozen(directory.to_path_buf())
    }
}

impl Drop for Frozen {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

/// Where `keepassxc-cli` lives, or `None` when the suite is running without it.
///
/// It is on `PATH` on Ubuntu, where CI installs it from apt, and inside the
/// application bundle on macOS, where it ships only as a cask.
pub fn keepassxc_cli() -> Option<PathBuf> {
    if std::env::var_os(SKIP_VARIABLE).is_some() {
        return None;
    }

    if let Ok(output) = Command::new("sh")
        .arg("-c")
        .arg("command -v keepassxc-cli")
        .output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    let bundled = PathBuf::from("/Applications/KeePassXC.app/Contents/MacOS/keepassxc-cli");
    if bundled.is_file() {
        return Some(bundled);
    }

    panic!(
        "keepassxc-cli is not installed, so the round-trip criterion cannot be checked. \
         Install keepassxc, or set {SKIP_VARIABLE}=1 to run the rest of the suite without it."
    );
}

/// Runs a keepassxc-cli subcommand against a database, feeding it the password
/// on stdin the way `-q` expects.
pub fn cli(
    tool: &Path,
    secret: &str,
    key_file: Option<&Path>,
    arguments: &[&str],
) -> Result<Vec<u8>, String> {
    use std::io::Write;

    let mut command = Command::new(tool);
    command.args(arguments);
    if let Some(key_file) = key_file {
        command.arg("--key-file").arg(key_file);
    }

    let mut child = command
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    child
        .stdin
        .as_mut()
        .ok_or("no stdin")?
        .write_all(format!("{secret}\n").as_bytes())
        .map_err(|error| error.to_string())?;

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// The database as keepassxc-cli renders it in XML.
pub fn export(tool: &Path, database: &Path, secret: &str, key_file: Option<&Path>) -> String {
    let database = database.to_string_lossy().into_owned();
    let bytes = cli(
        tool,
        secret,
        key_file,
        &["export", "-q", "-f", "xml", &database],
    )
    .unwrap_or_else(|error| panic!("keepassxc-cli could not export {database}: {error}"));
    String::from_utf8(bytes).expect("the export is UTF-8")
}

/// The `<Entry>` an export holds for the entry with this title, from its start
/// to the first `</Entry>` after it. That is the entry's own end when it has no
/// versions, and the end of its first version when it has: either way the
/// fields before any `<History>` are the entry as it stands, and a `<History>`
/// in it is a version KeePassXC read.
pub fn exported_entry<'a>(xml: &'a str, title: &str) -> Option<&'a str> {
    let at = xml.find(&format!("<Value>{title}</Value>"))?;
    let start = xml.get(..at)?.rfind("<Entry>")?;
    let rest = xml.get(start..)?;
    rest.get(..rest.find("</Entry>")?)
}

/// The value an export gives a field, and whether it marks it to be kept
/// protected: the first field of that name in `xml`, which in an entry is the
/// entry's own before any of its versions'. Read off the text rather than
/// parsed: the export is the other implementation's own words, and this asks
/// it two things. An empty value is written as an element with nothing in it.
pub fn exported_field(xml: &str, name: &str) -> Option<(String, bool)> {
    let key = format!("<Key>{name}</Key>");
    let after = xml.get(xml.find(&key)? + key.len()..)?;
    let opens = after.find("<Value")?;
    let tag = after.get(opens..opens + after.get(opens..)?.find('>')? + 1)?;
    let hidden = tag.contains("ProtectInMemory=\"True\"");
    if tag.ends_with("/>") {
        return Some((String::new(), hidden));
    }
    let rest = after.get(opens + tag.len()..)?;
    let value = rest.get(..rest.find("</Value>")?)?;
    Some((value.to_owned(), hidden))
}

/// Every entry path in the database, in the order keepassxc-cli lists them.
/// Groups are dropped: only entries can carry attachments.
pub fn entry_paths(
    tool: &Path,
    database: &Path,
    secret: &str,
    key_file: Option<&Path>,
) -> Vec<String> {
    let path = database.to_string_lossy().into_owned();
    let bytes = cli(tool, secret, key_file, &["ls", "-q", "-R", "-f", &path])
        .unwrap_or_else(|error| panic!("keepassxc-cli could not list {path}: {error}"));

    String::from_utf8_lossy(&bytes)
        .lines()
        // A trailing slash marks a group, and "[empty]" is what keepassxc-cli
        // prints in place of the children a group has none of.
        .filter(|line| !line.is_empty() && !line.ends_with('/'))
        .filter(|line| line.rsplit('/').next() != Some("[empty]"))
        .map(str::to_owned)
        .collect()
}

/// The attachment names on one entry, as keepassxc-cli reports them.
pub fn attachment_names(
    tool: &Path,
    database: &Path,
    secret: &str,
    key_file: Option<&Path>,
    entry: &str,
) -> Vec<String> {
    let path = database.to_string_lossy().into_owned();
    let bytes = cli(
        tool,
        secret,
        key_file,
        &["show", "-q", "--show-attachments", &path, entry],
    )
    .unwrap_or_else(|error| panic!("keepassxc-cli could not show {entry}: {error}"));

    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Some((_, listing)) = text.split_once("Attachments:\n") else {
        return Vec::new();
    };

    listing
        .lines()
        .filter(|line| line.starts_with("  "))
        .filter_map(|line| {
            // "  name.txt (17 B)" - the size is appended in a bracket, and a
            // name may itself contain brackets, so only the last one goes.
            let trimmed = line.trim_start();
            let cut = trimmed.rfind(" (")?;
            Some(trimmed.get(..cut)?.to_owned())
        })
        .collect()
}

/// One attachment's bytes, pulled out through keepassxc-cli so that the
/// comparison is against an external reader rather than our own.
pub fn attachment_bytes(
    tool: &Path,
    database: &Path,
    secret: &str,
    key_file: Option<&Path>,
    entry: &str,
    name: &str,
    into: &Path,
) -> Vec<u8> {
    let path = database.to_string_lossy().into_owned();
    let target = into.to_string_lossy().into_owned();
    cli(
        tool,
        secret,
        key_file,
        &["attachment-export", "-q", &path, entry, name, &target],
    )
    .unwrap_or_else(|error| panic!("keepassxc-cli could not export {name} from {entry}: {error}"));

    std::fs::read(into).expect("the exported attachment reads back")
}

/// Builds a database out of XML, the way the golden fixtures are built.
///
/// The one shape Coffer cannot produce for itself is a file that several
/// entries name: nothing in the library adds a second name for a file that is
/// already in the pool, and KeePass 2 writes them all the time, because it
/// stores identical binaries once. Only an external writer can make one, so the
/// tests that need one ask the same tool the fixtures came from.
pub fn imported(tool: &Path, directory: &Path, name: &str, xml: &str, secret: &str) -> PathBuf {
    let source = directory.join(format!("{name}.xml"));
    std::fs::write(&source, xml).expect("the source is written");

    let target = directory.join(format!("{name}.kdbx"));
    let source = source.to_string_lossy().into_owned();
    let written = target.to_string_lossy().into_owned();

    // `import` asks for the password twice, the way it asks when a database is
    // created.
    cli(
        tool,
        &format!("{secret}\n{secret}"),
        None,
        &["import", "-p", &source, &written],
    )
    .unwrap_or_else(|error| panic!("keepassxc-cli could not import {name}: {error}"));

    target
}
