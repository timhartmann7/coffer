//! Reading this crate's own source, for the checks nothing else can make.
//!
//! Some rules are about where a thing is written rather than about what it
//! does: which thread a command is answered on, which function puts a vault
//! into the session. There is no way to ask the running program either
//! question, and a new command or a new way in added the obvious way is exactly
//! the one that breaks the rule.

/// Everything above a file's test module. A test names the calls it is looking
/// for, and would otherwise count itself.
///
/// The module and not the first `#[cfg(test)]`: a file can hold a test-only
/// item above shipping code (`lib.rs` declares this module before `run`), and
/// cutting there would hide everything after it from the check.
pub fn shipped(file: &str) -> &str {
    file.split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or_default()
}

/// Every function in a file, cut at each `fn` that begins a line.
///
/// Coarse on purpose. A helper that several functions share is a function of
/// its own here, which is what the checks need: two ways in going through one
/// door, and the door being where the rule is kept.
pub fn functions(source: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();

    for line in source.lines() {
        let head = line.trim_start();
        let starts = ["fn ", "pub fn ", "async fn ", "pub async fn "]
            .iter()
            .any(|shape| head.starts_with(shape));

        if starts || found.is_empty() {
            found.push(String::new());
        }
        if let Some(body) = found.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }

    found
}

/// Every file of this crate's source, with where it is.
///
/// For a rule about a call that no file may make, or that one file alone may:
/// `include_str!` reads only the files a check names, and the file that breaks
/// the rule is the one nobody thought to name.
pub fn every_file() -> Vec<(std::path::PathBuf, String)> {
    let mut directories = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for found in std::fs::read_dir(&directory).expect("the source is there") {
            let path = found.expect("the source is readable").path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            let file = std::fs::read_to_string(&path).expect("the source is text");
            files.push((path, file));
        }
    }
    assert!(
        files.len() > 10,
        "only {} files of this crate were read",
        files.len()
    );
    files
}
