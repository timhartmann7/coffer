//! Reading this crate's own source, for the checks nothing else can make.
//!
//! Some rules are about where a thing is written rather than about what it
//! does: which thread a command is answered on, which function puts a vault
//! into the session. There is no way to ask the running program either
//! question, and a new command or a new way in added the obvious way is exactly
//! the one that breaks the rule.

/// Everything above a file's test module. A test names the calls it is looking
/// for, and would otherwise count itself.
pub fn shipped(file: &str) -> &str {
    file.split("#[cfg(test)]").next().unwrap_or_default()
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
