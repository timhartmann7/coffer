//! The scheme allowlist, given everything a URL field can hold.

use vault_core::url::openable;

use crate::support::{self, RICH, SECRET, entry_titled, open};

#[test]
fn the_four_allowed_schemes_open_however_they_are_written() {
    for (written, expected) in [
        ("https://example.com", "https://example.com"),
        ("http://example.com/a?b=c#d", "http://example.com/a?b=c#d"),
        ("HTTPS://EXAMPLE.COM", "HTTPS://EXAMPLE.COM"),
        ("HtTpS://example.com", "HtTpS://example.com"),
        ("mailto:someone@example.com", "mailto:someone@example.com"),
        (
            "ftp://files.example.com/key.pub",
            "ftp://files.example.com/key.pub",
        ),
        // Whitespace around a value is common in a file written by hand, and
        // what comes back is what gets opened.
        ("  https://example.com  ", "https://example.com"),
        ("\thttps://example.com\n", "https://example.com"),
    ] {
        assert_eq!(openable(written), Some(expected), "{written:?}");
    }
}

#[test]
fn everything_that_could_run_code_is_refused() {
    for written in [
        "javascript:alert(document.domain)",
        "JaVaScRiPt:alert(1)",
        "  javascript:alert(1)",
        "\u{a0}javascript:alert(1)",
        // A tab inside the scheme: browsers and NSURL drop it before they act,
        // so the string that gets opened is not the string that was checked.
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        "java\rscript:alert(1)",
        "java\u{0}script:alert(1)",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "file:///etc/passwd",
        "vbscript:msgbox(1)",
        "x-apple-shortcuts://run-shortcut?name=wipe",
        "shell:cmd",
        // A right-to-left override cannot smuggle a scheme past the list: it is
        // not whitespace, so it stays in the scheme and the scheme stops
        // matching.
        "\u{202e}javascript:alert(1)",
        "jav\u{202e}ascript:alert(1)",
    ] {
        assert_eq!(openable(written), None, "{written:?}");
    }
}

#[test]
fn nothing_that_is_not_an_address_is_opened() {
    for written in [
        "",
        "   ",
        "example.com",
        "//example.com",
        "/etc/passwd",
        "https:",
        "https",
        ":",
        ":https://example.com",
        // An interior control character is refused whatever the scheme is.
        "https://exam\u{7f}ple.com",
    ] {
        assert_eq!(openable(written), None, "{written:?}");
    }
}

#[test]
fn a_million_characters_of_url_do_not_hang_the_check() {
    let long = format!("https://example.com/{}", "a".repeat(1_000_000));
    assert_eq!(openable(&long), Some(long.as_str()));

    let long_scheme = format!("{}:x", "a".repeat(1_000_000));
    assert_eq!(openable(&long_scheme), None);
}

/// The fixture carries the same hostile values a real database can, and the
/// list has to hold against the ones that arrive through the reader rather than
/// through a string literal.
#[test]
fn the_url_fields_of_the_fixture_are_judged_the_same_way() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "dangerous urls");

    assert_eq!(openable(entry.url()), None);
    for name in [
        "url-file",
        "url-vbscript",
        "url-mixed-case",
        "url-whitespace",
    ] {
        let field = entry.field(name).expect("the field is there");
        let value = field.value.open().expect("the field is not protected");
        assert_eq!(openable(value), None, "{name}");
    }

    let ordinary = entry_titled(&vault, "basic");
    assert_eq!(openable(ordinary.url()), Some(ordinary.url()));
}
