//! Which production sources may name the second login.
//!
//! A desktop has a kind, and each thing that differs between kinds has one
//! owner that matches on it. Code outside those owners, and outside the
//! second login's own implementation, must not name the second login: it
//! would then work for that one kind alone, unseen.
//!
//! This checks names, not types. Three identifiers mean the second login
//! and nothing else in the workspace: `SecondLogin`, `second_login` and
//! `helper_uid`. A source that holds one must be listed in [`ALLOWED`]
//! with why. What it cannot see: an allowed function that returns the
//! account under another name, a struct that keeps it as `uid`, and an
//! assumption that never names the second login at all, such as a file
//! kept beside a socket. Those assumptions are outside this check.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The identifiers that name the second login.
const NAMES: [&str; 3] = ["SecondLogin", "second_login", "helper_uid"];

/// The sources that may name the second login, from the workspace root,
/// each with what it owns (an operation that matches on the desktop's kind)
/// or what of the second login's own implementation it holds.
const ALLOWED: &[(&str, &str)] = &[
    (
        "crates/agents/src/lib.rs",
        "owner: the readiness an agent's brief reports",
    ),
    (
        "crates/cli/src/bin/atlas/dispatch.rs",
        "owner: showing the person this desktop",
    ),
    (
        "crates/cli/src/bin/atlas/doctor.rs",
        "owner: what doctor reports",
    ),
    (
        "crates/cli/src/bin/atlas/init.rs",
        "second login: setting it up from the terminal",
    ),
    (
        "crates/cli/src/bin/atlas/main.rs",
        "owner: why it is silent, and whether setup is complete",
    ),
    (
        "crates/cli/src/bin/atlas/update.rs",
        "second login: finishing an update in it",
    ),
    (
        "crates/cli/src/mcp.rs",
        "owner: showing the person this desktop",
    ),
    (
        "crates/cli/src/onboarding.rs",
        "second login: choosing and preparing its account",
    ),
    (
        "crates/cli/src/onboarding/acceptance.rs",
        "second login: the setup connection and its acceptance",
    ),
    (
        "crates/cli/src/onboarding/status.rs",
        "second login: how far setup has come",
    ),
    ("crates/cli/src/uninstall.rs", "second login: removing it"),
    (
        "crates/cli/src/uninstall/inspection.rs",
        "second login: what removing it would take",
    ),
    (
        "crates/cli/src/uninstall/log_out.rs",
        "second login: logging it out before removal",
    ),
    (
        "crates/client/src/capability.rs",
        "second login: its readiness, and the native-dialog evidence kept for it on this disk",
    ),
    (
        "crates/client/src/capability/canary.rs",
        "second login: establishing native capability evidence",
    ),
    (
        "crates/client/src/client.rs",
        "owner: its connections, and getting an observation of it",
    ),
    (
        "crates/client/src/client/input.rs",
        "owner: the input connection",
    ),
    (
        "crates/client/src/client/service.rs",
        "owner: starting its input service",
    ),
    (
        "crates/client/src/client/session.rs",
        "owner: whether its session has ended",
    ),
    (
        "crates/client/src/environment.rs",
        "owner: syncing, applying, rolling back and running an environment in it",
    ),
    (
        "crates/client/src/environment/discovery.rs",
        "second login: what environment discovery says of its account",
    ),
    (
        "crates/client/src/environment/list.rs",
        "owner: listing the environments copied into it",
    ),
    (
        "crates/client/src/environment/one_off.rs",
        "owner: installing and removing an app in it, and how its installs stand",
    ),
    (
        "crates/client/src/environment/profile.rs",
        "owner: freezing an environment profile for it",
    ),
    (
        "crates/client/src/environment/stage.rs",
        "second login: staging an environment on this disk for its account",
    ),
    (
        "crates/client/src/environment/undo.rs",
        "owner: how its environment plans stand, and undoing one",
    ),
    (
        "crates/client/src/handoff.rs",
        "owner: opening a file in it, and the second login's way of doing so",
    ),
    (
        "crates/client/src/home_privacy.rs",
        "second login: what its account may reach of the person's home",
    ),
    (
        "crates/client/src/jobs.rs",
        "owner: that a started job is the one asked for",
    ),
    (
        "crates/client/src/overview.rs",
        "owner: the report of its readiness",
    ),
    (
        "crates/client/src/setup.rs",
        "second login: its setup, as the controller sees it",
    ),
    (
        "crates/client/src/setup/configuration.rs",
        "second login: the settings setup writes for it",
    ),
    (
        "crates/client/src/setup/job.rs",
        "second login: installing and restarting its job",
    ),
    (
        "crates/client/src/setup/migrate.rs",
        "second login: moving an older setup forward",
    ),
    (
        "crates/client/src/switch.rs",
        "second login: locking this Mac to its login window, and the account to come back to",
    ),
    (
        "crates/control/src/lib.rs",
        "second login: the controller of one, built for the tests of the crates above",
    ),
    (
        "crates/core/src/config.rs",
        "owner: the desktop that the settings name",
    ),
    (
        "crates/core/src/desktop.rs",
        "owner: its identity, proof, input readiness and processes; and the way in for the second login's own setup, update and removal",
    ),
    (
        "crates/daemon/src/helper_setup/first_login.rs",
        "second login: its first login",
    ),
    (
        "crates/daemon/src/helper_setup/pairing.rs",
        "second login: its pairing request",
    ),
];

/// One token of Rust source, as far as this check reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Identifier(String),
    Punctuation(char),
}

/// The identifiers and punctuation of `source`, with comments and string,
/// byte-string, raw-string and character literals left out. Macro bodies
/// are read as ordinary tokens.
fn tokens(source: &str) -> Vec<Token> {
    let text: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let character = text[at];
        let next = text.get(at + 1).copied();
        if character.is_whitespace() {
            at += 1;
        } else if character == '/' && next == Some('/') {
            at = line_end(&text, at);
        } else if character == '/' && next == Some('*') {
            at = block_comment_end(&text, at);
        } else if character == '"' {
            at = string_end(&text, at + 1);
        } else if character == '\'' {
            at = character_or_lifetime_end(&text, at);
        } else if character == '_' || character.is_alphabetic() {
            let end = identifier_end(&text, at);
            let word: String = text[at..end].iter().collect();
            at = match literal_after_prefix(&text, &word, end) {
                Some(literal_end) => literal_end,
                None if word == "r" && text.get(end) == Some(&'#') => {
                    // A raw identifier: its name is the token.
                    let name_end = identifier_end(&text, end + 1);
                    tokens.push(Token::Identifier(text[end + 1..name_end].iter().collect()));
                    name_end
                }
                None => {
                    tokens.push(Token::Identifier(word));
                    end
                }
            };
        } else {
            tokens.push(Token::Punctuation(character));
            at += 1;
        }
    }
    tokens
}

fn line_end(text: &[char], from: usize) -> usize {
    text[from..]
        .iter()
        .position(|character| *character == '\n')
        .map_or(text.len(), |offset| from + offset)
}

/// The end of the block comment opening at `from`; block comments nest.
fn block_comment_end(text: &[char], from: usize) -> usize {
    let mut depth = 0;
    let mut at = from;
    while at < text.len() {
        match (text[at], text.get(at + 1)) {
            ('/', Some('*')) => {
                depth += 1;
                at += 2;
            }
            ('*', Some('/')) => {
                depth -= 1;
                at += 2;
                if depth == 0 {
                    return at;
                }
            }
            _ => at += 1,
        }
    }
    text.len()
}

/// The end of the string whose body starts at `from`, past its closing
/// quote.
fn string_end(text: &[char], from: usize) -> usize {
    let mut at = from;
    while at < text.len() {
        match text[at] {
            '\\' => at += 2,
            '"' => return at + 1,
            _ => at += 1,
        }
    }
    text.len()
}

/// The end of the raw string whose hashes or quote start at `from`; none
/// when what starts there is not a raw string.
fn raw_string_end(text: &[char], from: usize) -> Option<usize> {
    let hashes = text[from..]
        .iter()
        .take_while(|character| **character == '#')
        .count();
    let body = from + hashes;
    if text.get(body) != Some(&'"') {
        return None;
    }
    let mut at = body + 1;
    while at < text.len() {
        if text[at] == '"'
            && text[at + 1..]
                .iter()
                .take(hashes)
                .filter(|character| **character == '#')
                .count()
                == hashes
        {
            return Some(at + 1 + hashes);
        }
        at += 1;
    }
    Some(text.len())
}

/// The end of the string literal that the prefix `word`, ending at `end`,
/// opens: `b"…"`, `c"…"`, `r"…"`, `r#"…"#`, `br"…"`, `cr"…"`. None when
/// `word` is an identifier of its own.
fn literal_after_prefix(text: &[char], word: &str, end: usize) -> Option<usize> {
    match word {
        "b" | "c" if text.get(end) == Some(&'"') => Some(string_end(text, end + 1)),
        "b" if text.get(end) == Some(&'\'') => Some(character_or_lifetime_end(text, end)),
        "r" | "br" | "cr" => raw_string_end(text, end),
        _ => None,
    }
}

/// Past the character literal at `from`, or past the quote alone when it
/// opens a lifetime, whose name is then read as an identifier.
fn character_or_lifetime_end(text: &[char], from: usize) -> usize {
    match (text.get(from + 1), text.get(from + 2)) {
        (Some('\\'), _) => text
            .get(from + 3..)
            .unwrap_or_default()
            .iter()
            .position(|character| *character == '\'')
            .map_or(text.len(), |offset| from + 3 + offset + 1),
        (Some(_), Some('\'')) => from + 3,
        _ => from + 1,
    }
}

fn identifier_end(text: &[char], from: usize) -> usize {
    text[from..]
        .iter()
        .position(|character| !(*character == '_' || character.is_alphanumeric()))
        .map_or(text.len(), |offset| from + offset)
}

fn identifier(token: &Token) -> Option<&str> {
    match token {
        Token::Identifier(name) => Some(name),
        Token::Punctuation(_) => None,
    }
}

/// Whether `tokens` name the second login.
fn names_second_login(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .filter_map(identifier)
        .any(|name| NAMES.contains(&name))
}

/// A way `tokens` would let the second login's type leave under another
/// name, which no source may use: renamed with `as`, behind a type alias,
/// or through a glob of the desktop module.
fn renames_second_login(tokens: &[Token]) -> Option<&'static str> {
    let named = |token: &Token| matches!(identifier(token), Some("SecondLogin" | "second_login"));
    let word = |token: &Token, expected: &str| identifier(token) == Some(expected);
    let punctuation = |token: &Token, expected: char| *token == Token::Punctuation(expected);

    if tokens
        .windows(2)
        .any(|pair| named(&pair[0]) && word(&pair[1], "as"))
    {
        return Some("renames it with `as`");
    }
    // What follows `desktop::` brings in items it does not name when it is a
    // glob, of the module or of one of its items (`Kind::*`), or a group
    // holding one at any depth. The desktop module has no modules of its
    // own, so every glob under it is one of these.
    let globbed = |from: usize| {
        // Along a path such as `Kind::`, to where it ends.
        let mut at = from;
        while matches!(
            (tokens.get(at), tokens.get(at + 1), tokens.get(at + 2)),
            (
                Some(Token::Identifier(_)),
                Some(Token::Punctuation(':')),
                Some(Token::Punctuation(':'))
            )
        ) {
            at += 3;
        }
        match tokens.get(at) {
            Some(Token::Punctuation('*')) => true,
            Some(Token::Punctuation('{')) => {
                let mut depth = 0_usize;
                for token in &tokens[at..] {
                    match token {
                        Token::Punctuation('*') => return true,
                        Token::Punctuation('{') => depth += 1,
                        Token::Punctuation('}') => match depth.checked_sub(1) {
                            Some(0) | None => return false,
                            Some(outer) => depth = outer,
                        },
                        Token::Identifier(_) | Token::Punctuation(_) => {}
                    }
                }
                false
            }
            _ => false,
        }
    };
    if tokens.windows(3).enumerate().any(|(at, path)| {
        word(&path[0], "desktop")
            && punctuation(&path[1], ':')
            && punctuation(&path[2], ':')
            && globbed(at + 3)
    }) {
        return Some("imports the desktop module's items with a glob");
    }
    let mut rest = tokens;
    while let Some(start) = rest.iter().position(|token| word(token, "type")) {
        let alias = &rest[start..];
        let end = alias
            .iter()
            .position(|token| punctuation(token, ';'))
            .unwrap_or(alias.len());
        let defined = alias[..end]
            .iter()
            .position(|token| punctuation(token, '='));
        if defined.is_some_and(|equals| alias[equals..end].iter().any(named)) {
            return Some("hides it behind a type alias");
        }
        rest = &alias[1..];
    }
    None
}

/// Whether `path`, from the workspace root, holds tests alone.
fn is_test_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.ends_with("_tests.rs")
        || name == "tests.rs"
        || path.components().any(|component| {
            let part = component.as_os_str().to_string_lossy();
            part == "tests" || part.starts_with("test_support")
        })
}

/// Every Rust source under `folder`.
fn sources_under(folder: &Path, found: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{} could not be listed: {error}", folder.display()));
    for entry in entries {
        let path = entry.expect("a folder entry can be read").path();
        if path.is_dir() {
            sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two folders above this crate")
}

/// The production sources of every workspace member, from the root: the
/// `src` folder of each crate and of `xtask`, without their tests.
fn production_sources(root: &Path) -> Vec<PathBuf> {
    let mut members: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .expect("the crates folder can be listed")
        .map(|entry| entry.expect("a crate folder can be read").path())
        .filter(|path| path.is_dir())
        .collect();
    members.push(root.join("xtask"));

    let mut sources = Vec::new();
    for member in members {
        sources_under(&member.join("src"), &mut sources);
    }
    let mut sources: Vec<PathBuf> = sources
        .into_iter()
        .map(|path| {
            path.strip_prefix(root)
                .expect("a source lies under the root")
                .to_owned()
        })
        .filter(|path| !is_test_source(path))
        .collect();
    sources.sort();
    sources
}

#[test]
fn only_listed_sources_name_the_second_login() {
    let root = workspace_root();
    let mut unlisted = Vec::new();
    let mut renamed = Vec::new();
    let mut naming = Vec::new();
    for source in production_sources(&root) {
        let text = fs::read_to_string(root.join(&source))
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", source.display()));
        let tokens = tokens(&text);
        let path = source.to_string_lossy().into_owned();
        if let Some(how) = renames_second_login(&tokens) {
            renamed.push(format!("{path} {how}"));
        }
        if names_second_login(&tokens) {
            if !ALLOWED.iter().any(|(allowed, _)| *allowed == path) {
                unlisted.push(path.clone());
            }
            naming.push(path);
        }
    }
    let stale: Vec<&str> = ALLOWED
        .iter()
        .map(|(allowed, _)| *allowed)
        .filter(|allowed| !naming.iter().any(|path| path == allowed))
        .collect();

    assert!(
        renamed.is_empty(),
        "the second login may not leave under another name: {renamed:#?}"
    );
    assert!(
        unlisted.is_empty(),
        "these sources name the second login and are not its owners or its own code; \
         call the operation's owner instead: {unlisted:#?}"
    );
    assert!(
        stale.is_empty(),
        "these sources are listed and no longer name the second login; remove them: {stale:#?}"
    );
}

#[test]
fn every_listed_source_says_what_it_owns_or_implements() {
    for (path, role) in ALLOWED {
        assert!(
            role.starts_with("owner: ") || role.starts_with("second login: "),
            "{path} says neither what it owns nor what it implements: {role}"
        );
    }
    let mut paths: Vec<&str> = ALLOWED.iter().map(|(path, _)| *path).collect();
    paths.dedup();
    assert_eq!(paths.len(), ALLOWED.len(), "a source is listed once");
    assert!(paths.is_sorted(), "the list is kept in order");
}

#[test]
fn comments_and_literals_do_not_name_the_second_login() {
    for source in [
        "// second_login in a line comment",
        "/// helper_uid in a doc comment\nfn f() {}",
        "/* SecondLogin /* nested helper_uid */ still a comment */ fn f() {}",
        r#"let key = "helper_uid";"#,
        r#"let key = config["helper_uid"];"#,
        r#"let escaped = "a \" helper_uid \" b";"#,
        r##"let raw = r#"SecondLogin "quoted" helper_uid"#;"##,
        r#"let bytes = b"helper_uid"; let text = c"second_login";"#,
        r#"json!({"helper_uid": 1, "SecondLogin": 2})"#,
        r#"#[serde(rename = "helper_uid")] struct Kept;"#,
        "let quote = '\"'; let other = '\\''; let tick = 'h';",
    ] {
        assert!(!names_second_login(&tokens(source)), "{source}");
    }
}

#[test]
fn every_identifier_use_names_the_second_login() {
    for source in [
        "let login = desktop.second_login();",
        "let uid = client\n    .desktop()\n    .second_login()\n    .uid;",
        "use atlas_core::desktop::{Desktop, SecondLogin};",
        "match desktop.kind() { Kind::SecondLogin(login) => login.uid }",
        "Desktop::from(SecondLogin { uid })",
        "struct Pairing { helper_uid: u32 }",
        "let Config { helper_uid, .. } = config;",
        "json!({\"helperUID\": pairing.helper_uid})",
        "fn held<'a>(login: &'a SecondLogin) {}",
        "let text = \"closed\"; let uid = config.helper_uid;",
        "let r#type = desktop.r#second_login();",
        "#[cfg(test)] fn fixture() -> SecondLogin { todo!() }",
    ] {
        assert!(names_second_login(&tokens(source)), "{source}");
    }
}

#[test]
fn the_second_login_cannot_leave_under_another_name() {
    for (source, how) in [
        (
            "use atlas_core::desktop::SecondLogin as Login;",
            "renames it with `as`",
        ),
        (
            "pub use atlas_core::desktop::{Desktop, SecondLogin as Account};",
            "renames it with `as`",
        ),
        (
            "use atlas_core::desktop::*;",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::desktop::{*};",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::{desktop::{Desktop, *}, error::Result};",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::desktop::{Kind::{self}, *};",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::desktop::Kind::*;",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::desktop::{Desktop, Kind::{*}};",
            "imports the desktop module's items with a glob",
        ),
        (
            "type Login = atlas_core::desktop::SecondLogin;",
            "hides it behind a type alias",
        ),
        (
            "pub(crate) type Logins<'a> = Vec<&'a SecondLogin>;",
            "hides it behind a type alias",
        ),
    ] {
        assert_eq!(renames_second_login(&tokens(source)), Some(how), "{source}");
    }
    for source in [
        "use atlas_core::desktop::{Desktop, SecondLogin};",
        "let uid = login.uid as u64; let login: SecondLogin = made;",
        "type Reply = Result<Value>; fn of(login: &SecondLogin) {}",
        "use crate::desktop::Desktop;",
        "use atlas_core::{desktop::{Desktop, Kind}, error::{*}};",
        "use atlas_core::{desktop::Desktop, session::*};",
        "let area = desktop::width * 2;",
        "let type_of = kind; let login = SecondLogin { uid };",
    ] {
        assert_eq!(renames_second_login(&tokens(source)), None, "{source}");
    }
}

#[test]
fn test_sources_are_told_by_their_path() {
    for path in [
        "crates/core/src/desktop_tests.rs",
        "crates/control/src/tests.rs",
        "crates/control/src/run_groups/tests.rs",
        "crates/client/src/asking/tests/grants.rs",
        "crates/client/src/test_support.rs",
        "crates/client/src/test_support/helper.rs",
    ] {
        assert!(is_test_source(Path::new(path)), "{path}");
    }
    for path in [
        "crates/core/src/desktop.rs",
        "crates/client/src/setup/job.rs",
        "crates/cli/src/onboarding/acceptance.rs",
        "xtask/src/main.rs",
    ] {
        assert!(!is_test_source(Path::new(path)), "{path}");
    }
}
