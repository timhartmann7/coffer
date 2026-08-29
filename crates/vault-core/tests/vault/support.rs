//! Shared machinery: where the fixtures are, how to drive keepassxc-cli, and
//! how to compare two exports of the same database.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

use vault_core::{LockPolicy, MasterKey, Vault};

/// Set this to run the suite on a machine with no KeePassXC. Everything that
/// needs an external implementation is skipped, and the round-trip criterion
/// goes unproven, so CI never sets it.
const SKIP_VARIABLE: &str = "COFFER_SKIP_KEEPASSXC";

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

/// Every entry in the database, flattened, with previous versions excluded the
/// way the tree excludes them.
pub fn all_entries(vault: &Vault) -> Vec<vault_core::model::Entry> {
    fn walk(group: &vault_core::model::Project, into: &mut Vec<vault_core::model::Entry>) {
        into.extend(group.entries.iter().cloned());
        for section in &group.sections {
            walk(section, into);
        }
    }

    let mut entries = Vec::new();
    walk(&vault.tree(), &mut entries);
    entries
}

/// The one entry with this title. Panics when there is not exactly one, so that
/// a fixture change cannot quietly make a test assert nothing.
pub fn entry_titled(vault: &Vault, title: &str) -> vault_core::model::Entry {
    let mut found: Vec<_> = all_entries(vault)
        .into_iter()
        .filter(|entry| entry.title() == title)
        .collect();

    assert_eq!(
        found.len(),
        1,
        "expected exactly one entry titled {title:?}"
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

/// Whether this process can be stopped by file permissions at all. Running as
/// root makes every permission test pass without testing anything.
pub fn permissions_apply() -> bool {
    // SAFETY: geteuid reads a process property and touches nothing.
    unsafe { libc::geteuid() != 0 }
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
