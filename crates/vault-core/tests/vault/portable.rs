//! The rules that keep this suite running on the machine CI runs it on.
//!
//! `vault-core` is the half of Coffer that is not macOS, and the engine job
//! builds and runs it on Ubuntu. A test written on a Mac can pass here and fail
//! there for reasons that have nothing to do with what it is testing, and three
//! of them reached CI one after another: an import that only one platform's
//! block used, and twice a helper that shelled out to a tool a Mac has and
//! Ubuntu does not.
//!
//! Compiling for the other target catches the first kind. Nothing caught the
//! other two, because a path to a missing program is a perfectly good string
//! until it is run. This is what catches them: a rule about the source, checked
//! on every platform, in the same spirit as the frontend's `tokens.test.ts`.

use std::path::Path;

/// Every `.rs` file in this suite, with its name.
fn sources() -> Vec<(String, String)> {
    fn walk(directory: &Path, found: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(directory).expect("the suite is readable") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push((
                    path.display().to_string(),
                    std::fs::read_to_string(&path).expect("a source file"),
                ));
            }
        }
    }

    let mut found = Vec::new();
    walk(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .as_path(),
        &mut found,
    );
    assert!(found.len() > 10, "the suite was not found where it lives");
    found
}

/// A tool named by its absolute path is a tool one of the two machines does not
/// have. Both times this happened the failure was a panic in CI on a Mac-only
/// binary - `/usr/bin/xattr`, then `/usr/bin/touch` - and both were replaced by
/// the syscall underneath, which needs no path and exists on both.
///
/// `/proc` and the KeePassXC bundle are not tools and are not caught: one is
/// read behind a `cfg` for the kernel that has it, and the other is looked for
/// and given up on.
#[test]
fn no_test_reaches_for_a_tool_by_absolute_path() {
    for (name, text) in sources() {
        if name.ends_with("portable.rs") {
            continue;
        }
        for (at, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            for root in [
                "/usr/bin/",
                "/bin/",
                "/sbin/",
                "/usr/sbin/",
                "/opt/homebrew/",
            ] {
                assert!(
                    !code.contains(root),
                    "{name}:{} names {root}, which is a path one of the two machines does not \
                     have. Call the syscall underneath, or look the tool up on PATH and say \
                     what happens when it is not there.",
                    at + 1
                );
            }
        }
    }
}

/// A program named by a literal is a program that has to exist under that name
/// on both machines, and `sh` is the only one POSIX promises.
///
/// Everything else this suite runs is computed: `keepassxc_cli()` looks the tool
/// up and answers `None` when it is missing, and two tests re-run this very
/// binary as a child. Neither can name something that is not there.
#[test]
fn the_only_program_this_suite_names_is_the_one_posix_promises() {
    for (name, text) in sources() {
        if name.ends_with("portable.rs") {
            continue;
        }
        for (at, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            let Some(rest) = code.split("Command::new(").nth(1) else {
                continue;
            };
            // A computed program - a variable, or this binary's own path - is
            // not a name and cannot be missing under the wrong one.
            if !rest.trim_start().starts_with('"') {
                continue;
            }
            let named = rest.trim_start().trim_start_matches('"');
            let named = named.split('"').next().unwrap_or(named);
            assert_eq!(
                named,
                "sh",
                "{name}:{} runs `{named}` by name. Only `sh` is on every machine this suite \
                 runs on; anything else has to be looked for first.",
                at + 1
            );
        }
    }
}
