//! Which production code may name the user desktop: a hidden user on this
//! Mac, the kind people know as Second Login.
//!
//! A desktop has a kind, and each thing that differs between kinds has one
//! owner that matches on it. Each kind's own code lives in its module at
//! the root of every target (`user` for the user desktop). Outside those
//! modules, only the items listed in [`ALLOWED`] may name the user desktop,
//! each saying why.
//!
//! The check reads every target's modules as the compiler finds them: from
//! the target's root through `mod` declarations, following `#[path]` and
//! the compiler's rules for where a module's own modules live, and leaving
//! out what a `cfg` compiles for tests alone (`test`, or the `test-support`
//! feature). It then checks each item outside the kinds' modules:
//!
//! - its imports, by their paths: none may reach into a kind's module, and
//!   none may rename or glob the desktop's kind, type or module;
//! - its other code, by names: `helper_uid`; `User`, unless reached through
//!   another module's path (`nix::unistd::User`); and the accessor `user`
//!   through a path or called as a method (`.user()`, `Desktop::user`). A
//!   plain `user` (a variable, a field, a free function) names nothing.
//!
//! It guards against ordinary drift; it is not a proof. What it cannot see:
//! an item that returns the account under another name, a struct that keeps
//! it as `uid`, a name a macro builds from pieces, and an assumption that
//! never names the user desktop at all, such as a file kept beside a socket.
//! [`KEPT`] lists the shared assumptions known to stay; reviews look for the
//! rest.
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use syn::{
    Attribute, ImplItem, Item, ItemUse, TraitItem, UseTree,
    visit::{self, Visit},
    visit_mut::{self, VisitMut},
};

/// Each desktop kind's module, declared at the root of every target that
/// holds code of that kind.
const KINDS: [&str; 1] = ["user"];

/// The identifier that names the user desktop wherever it appears.
const HELPER_UID: &str = "helper_uid";

/// The user desktop's type, which names it unless reached through another
/// module's path.
const TYPE: &str = "User";

/// The user desktop's accessor, which names it when reached through a path
/// or called as a method.
const ACCESSOR: &str = "user";

/// The paths whose `User` is the user desktop's: its kind's variant, the
/// modules that define it, and `Self` inside them.
const OWN_PATHS: [&str; 4] = ["Kind", "Self", "desktop", "user"];

/// The desktop's names that may not be renamed, globbed or aliased.
const GUARDED: [&str; 4] = ["Desktop", "Kind", "User", "desktop"];

/// Why an item outside the kinds' modules may name the user desktop.
const ROLES: [&str; 5] = [
    // It matches on the desktop's kind and hands each kind its operation.
    "owner: ",
    // It declares a kind's module.
    "declares: ",
    // A shared assumption about the user desktop, kept on purpose.
    "exception: ",
    // The user desktop's own code, still to move into its module.
    "pending: ",
    // Code built only for the tests of the crates above.
    "test support: ",
];

/// One flattened token of Rust source. Literals are left out: a name in a
/// string or a comment names nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Identifier(String),
    Punctuation(char),
}

fn flatten(stream: TokenStream, into: &mut Vec<Token>) {
    for tree in stream {
        match tree {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let name = name.strip_prefix("r#").map_or(name.clone(), str::to_owned);
                into.push(Token::Identifier(name));
            }
            TokenTree::Punct(punct) => into.push(Token::Punctuation(punct.as_char())),
            TokenTree::Literal(_) => {}
            TokenTree::Group(group) => {
                let delimiters = match group.delimiter() {
                    Delimiter::Parenthesis => Some(('(', ')')),
                    Delimiter::Brace => Some(('{', '}')),
                    Delimiter::Bracket => Some(('[', ']')),
                    Delimiter::None => None,
                };
                if let Some((open, _)) = delimiters {
                    into.push(Token::Punctuation(open));
                }
                flatten(group.stream(), into);
                if let Some((_, close)) = delimiters {
                    into.push(Token::Punctuation(close));
                }
            }
        }
    }
}

/// The tokens of `source`, which must be well-formed Rust tokens.
fn tokens(source: &str) -> Vec<Token> {
    let stream: TokenStream = source
        .parse()
        .unwrap_or_else(|error| panic!("{source} is not Rust: {error}"));
    let mut found = Vec::new();
    flatten(stream, &mut found);
    found
}

fn tokens_of(item: &impl ToTokens) -> Vec<Token> {
    let mut found = Vec::new();
    flatten(item.to_token_stream(), &mut found);
    found
}

fn identifier(token: &Token) -> Option<&str> {
    match token {
        Token::Identifier(name) => Some(name),
        Token::Punctuation(_) => None,
    }
}

/// Whether the token at `at` follows `::`, with the path segment before
/// that when there is one.
fn after_path(tokens: &[Token], at: usize) -> Option<Option<&str>> {
    let colon = Token::Punctuation(':');
    let before = |count: usize| at.checked_sub(count).and_then(|index| tokens.get(index));
    (before(1) == Some(&colon) && before(2) == Some(&colon)).then(|| before(3).and_then(identifier))
}

/// Whether the token at `at` comes before `::`.
fn before_path(tokens: &[Token], at: usize) -> bool {
    let colon = Token::Punctuation(':');
    tokens.get(at + 1) == Some(&colon) && tokens.get(at + 2) == Some(&colon)
}

/// Whether `tokens` name the user desktop.
fn names_the_user(tokens: &[Token]) -> bool {
    tokens.iter().enumerate().any(|(at, token)| {
        let Some(name) = identifier(token) else {
            return false;
        };
        if name == HELPER_UID {
            return true;
        }
        if name == TYPE {
            // Through a type (`Kind::User`, `Self::User`, an alias of either),
            // one of its own modules, or a macro's metavariable (`$kind::User`);
            // not through another module.
            let metavariable = at.checked_sub(4).and_then(|index| tokens.get(index))
                == Some(&Token::Punctuation('$'));
            return match after_path(tokens, at) {
                Some(Some(before)) => {
                    metavariable
                        || OWN_PATHS.contains(&before)
                        || before.chars().next().is_some_and(char::is_uppercase)
                }
                Some(None) | None => true,
            };
        }
        if name != ACCESSOR {
            return false;
        }
        // Through a path (`Desktop::user`, `user::copies`), or called as a
        // method (`.user()`, `.user::<>()`); a field is followed by neither
        // `(` nor `::`.
        let method = at.checked_sub(1).and_then(|before| tokens.get(before))
            == Some(&Token::Punctuation('.'))
            && (tokens.get(at + 1) == Some(&Token::Punctuation('(')) || before_path(tokens, at));
        after_path(tokens, at).is_some() || before_path(tokens, at) || method
    })
}

/// One name an import brings in: the path to it, what it is called there,
/// and what it is called here.
#[derive(Debug, PartialEq, Eq)]
enum Imported {
    Name { path: Vec<String>, name: String },
    Renamed { path: Vec<String>, name: String },
    Glob { path: Vec<String> },
}

/// An identifier as it is meant: `r#user` is `user`.
fn plain(ident: &proc_macro2::Ident) -> String {
    let name = ident.to_string();
    name.strip_prefix("r#").map_or(name.clone(), str::to_owned)
}

fn imported(tree: &UseTree, path: &mut Vec<String>, found: &mut Vec<Imported>) {
    match tree {
        UseTree::Path(next) => {
            path.push(plain(&next.ident));
            imported(&next.tree, path, found);
            path.pop();
        }
        UseTree::Name(name) => found.push(Imported::Name {
            path: path.clone(),
            name: plain(&name.ident),
        }),
        UseTree::Rename(rename) => found.push(Imported::Renamed {
            path: path.clone(),
            name: plain(&rename.ident),
        }),
        UseTree::Glob(_) => found.push(Imported::Glob { path: path.clone() }),
        UseTree::Group(group) => {
            for tree in &group.items {
                imported(tree, path, found);
            }
        }
    }
}

/// The names `import` brings in.
fn imports(import: &ItemUse) -> Vec<Imported> {
    let mut found = Vec::new();
    imported(&import.tree, &mut Vec::new(), &mut found);
    found
}

/// What is wrong with `import` outside a kind's module, if anything.
fn refused_import(import: &ItemUse) -> Option<&'static str> {
    for name in imports(import) {
        let (path, last) = match &name {
            Imported::Name { path, name } | Imported::Renamed { path, name } => {
                // `Kind::{self}` names `Kind`.
                let last = if name == "self" {
                    path.last().cloned().unwrap_or_default()
                } else {
                    name.clone()
                };
                (path, Some(last))
            }
            Imported::Glob { path } => (path, None),
        };
        let reaches = |segment: &str| KINDS.contains(&segment);
        if path.iter().any(|segment| reaches(segment)) || last.as_deref().is_some_and(reaches) {
            return Some("imports a kind's module from outside it");
        }
        // Through the desktop module, or through its kind or type brought in
        // by an earlier import (`use Kind::User as Login;`).
        let local = path
            .iter()
            .find(|segment| !matches!(segment.as_str(), "crate" | "self" | "super"));
        let desktop_path = path.iter().any(|segment| segment == "desktop")
            || local.is_some_and(|first| matches!(first.as_str(), "Kind" | "Desktop" | "User"));
        // From within Atlas rather than another crate (`nix::unistd::User`).
        let from_atlas = path.first().is_none_or(|first| {
            matches!(first.as_str(), "crate" | "self" | "super") || first.starts_with("atlas_")
        });
        match &name {
            Imported::Renamed { .. }
                if last.as_deref().is_some_and(|last| {
                    GUARDED.contains(&last) && (desktop_path || from_atlas || last == "desktop")
                }) =>
            {
                return Some("renames the desktop's names with `as`");
            }
            Imported::Glob { .. } if desktop_path => {
                return Some("imports the desktop module's items with a glob");
            }
            Imported::Name { .. } | Imported::Renamed { .. } | Imported::Glob { .. } => {}
        }
    }
    None
}

/// The paths by which `import` brings in the user desktop's type or
/// `helper_uid`: a `User` from within Atlas (this crate, a module above, an
/// Atlas crate) or through the desktop's kind or module, not one of another
/// crate's.
fn user_imports(import: &ItemUse) -> Vec<String> {
    let atlas =
        |first: &str| matches!(first, "crate" | "self" | "super") || first.starts_with("atlas_");
    imports(import)
        .iter()
        .filter_map(|name| match name {
            Imported::Name { path, name } | Imported::Renamed { path, name } => {
                let own = path.first().is_none_or(|first| atlas(first))
                    || path
                        .iter()
                        .any(|segment| OWN_PATHS.contains(&segment.as_str()));
                ((name == TYPE && own) || name == HELPER_UID).then(|| {
                    path.iter()
                        .chain(std::iter::once(name))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("::")
                })
            }
            Imported::Glob { .. } => None,
        })
        .collect()
}

/// Whether `import` brings in the user desktop's type or `helper_uid`.
fn imports_the_user(import: &ItemUse) -> bool {
    !user_imports(import).is_empty()
}

/// Whether a type alias, or an associated type, of `target` hides the
/// desktop's names: its type, its module, or `Kind` or `Desktop` as imported
/// or from within Atlas. Another module's `Kind` (`store::Kind`) may be
/// aliased.
fn hides_the_user(target: &syn::Type) -> bool {
    let target = tokens_of(target);
    names_the_user(&target)
        || target
            .iter()
            .enumerate()
            .any(|(at, token)| match identifier(token) {
                Some("desktop") => true,
                Some("Kind" | "Desktop") => match after_path(&target, at) {
                    Some(Some(before)) => {
                        matches!(before, "desktop" | "crate" | "self" | "super")
                            || before.starts_with("atlas_")
                    }
                    Some(None) | None => true,
                },
                _ => false,
            })
}

/// Whether the `cfg` predicate `tokens` holds only when tests are built:
/// `test`, the `test-support` feature, `all(…)` with one such, or `any(…)`
/// of only such. Anything else may hold in a shipped build.
fn holds_for_tests_alone(tokens: &[TokenTree]) -> bool {
    let split = |stream: TokenStream| -> Vec<Vec<TokenTree>> {
        let mut parts = vec![Vec::new()];
        for tree in stream {
            match &tree {
                TokenTree::Punct(punct) if punct.as_char() == ',' => parts.push(Vec::new()),
                _ => parts.last_mut().expect("parts start with one").push(tree),
            }
        }
        parts.retain(|part| !part.is_empty());
        parts
    };
    match tokens {
        [TokenTree::Ident(word)] => word == "test",
        [
            TokenTree::Ident(word),
            TokenTree::Punct(equals),
            TokenTree::Literal(value),
        ] => {
            word == "feature" && equals.as_char() == '=' && value.to_string() == "\"test-support\""
        }
        [TokenTree::Ident(word), TokenTree::Group(group)]
            if group.delimiter() == Delimiter::Parenthesis =>
        {
            let parts = split(group.stream());
            match word.to_string().as_str() {
                "all" => parts.iter().any(|part| holds_for_tests_alone(part)),
                "any" => !parts.is_empty() && parts.iter().all(|part| holds_for_tests_alone(part)),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether `attributes` compile what they sit on for tests alone.
fn for_tests(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        let syn::Meta::List(list) = &attribute.meta else {
            return false;
        };
        list.path.is_ident("cfg")
            && holds_for_tests_alone(&list.tokens.clone().into_iter().collect::<Vec<_>>())
    })
}

/// The `#[path]` a module declaration gives, if any.
fn path_attribute(attributes: &[Attribute]) -> Option<String> {
    attributes.iter().find_map(|attribute| {
        let syn::Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        if !value.path.is_ident("path") {
            return None;
        }
        match &value.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            }) => Some(text.value()),
            _ => None,
        }
    })
}

/// The attributes of `item`.
fn attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

/// How a source is reached: in which kind's module, if any, and whether
/// only tests compile it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reached {
    kind: Option<&'static str>,
    tests: bool,
}

/// Where a module's own modules are found, as the compiler finds them:
/// the folder they start from and, for a module in `name.rs`, the folder
/// of that name beneath it.
#[derive(Clone, Debug)]
struct Folder {
    path: PathBuf,
    relative: Option<String>,
}

impl Folder {
    /// The folder a default declaration's source is found in.
    fn default_base(&self) -> PathBuf {
        match &self.relative {
            Some(name) => self.path.join(name),
            None => self.path.clone(),
        }
    }
}

/// Every source a workspace's targets compile, by its real path from the
/// workspace's root, found as the compiler finds them, and what could not
/// be followed.
#[derive(Debug, Default)]
struct Tree {
    root: PathBuf,
    sources: BTreeMap<PathBuf, Reached>,
    seen: BTreeSet<(PathBuf, bool)>,
    problems: Vec<String>,
}

impl Tree {
    /// The tree of the targets whose roots are `roots`, all under `root`.
    fn of(root: &Path, roots: &[PathBuf]) -> Self {
        let mut tree = Self {
            root: root.canonicalize().unwrap_or_else(|_| root.to_owned()),
            ..Self::default()
        };
        let top = Reached {
            kind: None,
            tests: false,
        };
        for target in roots {
            let folder = Folder {
                path: target.parent().unwrap_or(root).to_owned(),
                relative: None,
            };
            tree.file(target, &folder, top, true);
        }
        tree
    }

    /// `path` as it is named: its real path, from the workspace's root.
    fn named(&self, path: &Path) -> PathBuf {
        let real = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        real.strip_prefix(&self.root).unwrap_or(&real).to_owned()
    }

    /// Read `file`, whose modules are found from `folder`; `top` when it is
    /// a target's root.
    fn file(&mut self, file: &Path, folder: &Folder, reached: Reached, top: bool) {
        let name = self.named(file);
        if let Some(before) = self.sources.get(&name).copied() {
            if before.kind != reached.kind {
                self.problems.push(format!(
                    "{} is reached both inside and outside a kind's module",
                    name.display()
                ));
                return;
            }
            self.sources.insert(
                name.clone(),
                Reached {
                    kind: reached.kind,
                    tests: before.tests && reached.tests,
                },
            );
        } else {
            self.sources.insert(name.clone(), reached);
        }
        // A source is read again only from a new place or as production.
        let place = (folder.default_base().join(&name), reached.tests);
        if !self.seen.insert(place) {
            return;
        }
        let parsed = fs::read_to_string(file)
            .map_err(|error| error.to_string())
            .and_then(|text| syn::parse_file(&text).map_err(|error| error.to_string()));
        match parsed {
            // A source that opens with `#![cfg(test)]` is a test, whoever
            // declares it.
            Ok(parsed) if !reached.tests && for_tests(&parsed.attrs) => {
                let tested = Reached {
                    tests: true,
                    ..reached
                };
                if let Some(entry) = self.sources.get_mut(&name) {
                    entry.tests = true;
                }
                self.items(&parsed.items, folder, tested, top);
            }
            Ok(parsed) => self.items(&parsed.items, folder, reached, top),
            Err(error) => self
                .problems
                .push(format!("{} could not be read: {error}", name.display())),
        }
    }

    fn items(&mut self, items: &[Item], folder: &Folder, reached: Reached, top: bool) {
        for item in items {
            let Item::Mod(module) = item else {
                continue;
            };
            let name = plain(&module.ident);
            if module.attrs.iter().any(|attribute| {
                attribute.path().is_ident("cfg_attr")
                    && tokens_of(&attribute.meta)
                        .iter()
                        .any(|token| identifier(token) == Some("path"))
            }) {
                self.problems.push(format!(
                    "module `{name}` takes its path from `cfg_attr`, which this check cannot follow"
                ));
                continue;
            }
            let kind = reached.kind.or_else(|| {
                top.then(|| KINDS.iter().copied().find(|kind| *kind == name))
                    .flatten()
            });
            let inner = Reached {
                kind,
                tests: reached.tests || for_tests(&module.attrs),
            };
            let path = path_attribute(&module.attrs);
            match (&module.content, path) {
                // An inline module's `#[path]` names the folder its modules
                // are in.
                (Some((_, items)), Some(path)) => {
                    let folder = Folder {
                        path: folder.path.join(path),
                        relative: None,
                    };
                    self.items(items, &folder, inner, false);
                }
                (Some((_, items)), None) => {
                    let folder = Folder {
                        path: folder.default_base().join(&name),
                        relative: None,
                    };
                    self.items(items, &folder, inner, false);
                }
                // A declared module's `#[path]` names its source, from the
                // folder of the module declaring it; its own modules are
                // found beside it.
                (None, Some(path)) => {
                    let file = folder.path.join(path);
                    let beside = Folder {
                        path: file.parent().unwrap_or(&folder.path).to_owned(),
                        relative: None,
                    };
                    self.declared(&name, &file, &beside, inner);
                }
                (None, None) => {
                    let base = folder.default_base();
                    let flat = base.join(format!("{name}.rs"));
                    let nested = base.join(&name).join("mod.rs");
                    match (flat.is_file(), nested.is_file()) {
                        (true, true) => self.problems.push(format!(
                            "module `{name}` has two sources, {} and {}",
                            self.named(&flat).display(),
                            self.named(&nested).display()
                        )),
                        (true, false) => {
                            let own = Folder {
                                path: base,
                                relative: Some(name.clone()),
                            };
                            self.declared(&name, &flat, &own, inner);
                        }
                        (false, _) => {
                            let own = Folder {
                                path: base.join(&name),
                                relative: None,
                            };
                            self.declared(&name, &nested, &own, inner);
                        }
                    }
                }
            }
        }
    }

    /// Follow `mod name;`, whose source is `file`.
    fn declared(&mut self, name: &str, file: &Path, folder: &Folder, reached: Reached) {
        if file.is_file() {
            self.file(file, folder, reached, false);
        } else if !reached.tests {
            self.problems.push(format!(
                "module `{name}` has no source at {}",
                self.named(file).display()
            ));
        }
    }
}

/// A type or path as a key: its tokens without spaces.
fn compact(item: &impl ToTokens) -> String {
    item.to_token_stream()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Every item compiled outside tests, keyed by its name in the source, with
/// its tokens: `name` for a function, type or constant; `Type::name` for an
/// inherent `impl`'s members and `<Type as Trait>::name` for a trait's,
/// with `impl Type` or `impl Trait for Type` for the header; `Trait::name`
/// for a trait's members and `trait Trait` for its header; `extern::name`
/// for a foreign item; `mod name` for a module; `name! #n` for the nth call
/// of a macro, `macro name` for a macro's definition. Imports are keyed
/// apart, by what they bring in (`use path::User`).
#[derive(Default)]
struct Items {
    tokens: BTreeMap<String, Vec<Token>>,
    calls: BTreeMap<String, usize>,
}

impl Items {
    fn of(items: &[Item]) -> Self {
        let mut found = Self::default();
        found.collect(items, "");
        found
    }

    fn add(&mut self, key: String, tokens: Vec<Token>) {
        self.tokens.entry(key).or_default().extend(tokens);
    }

    /// `key`, or `key #n` when an item already has it: two `impl` headers of
    /// one type, or two calls of one macro, are told apart. Functions of one
    /// name stay one item, as only `cfg` lets them both exist.
    fn numbered(&self, key: String) -> String {
        if !self.tokens.contains_key(&key) {
            return key;
        }
        (2..)
            .map(|count| format!("{key} #{count}"))
            .find(|candidate| !self.tokens.contains_key(candidate))
            .expect("some number is free")
    }

    fn collect(&mut self, items: &[Item], prefix: &str) {
        for item in items {
            if for_tests(attributes(item)) {
                continue;
            }
            match item {
                Item::Fn(function) => {
                    self.add(
                        format!("{prefix}{}", function.sig.ident),
                        tokens_of(function),
                    );
                }
                Item::Impl(implementation) => self.implementation(implementation, prefix),
                Item::Trait(definition) => {
                    let name = &definition.ident;
                    let mut header: Vec<Token> =
                        definition.attrs.iter().flat_map(tokens_of).collect();
                    header.extend(tokens_of(&definition.generics));
                    header.extend(tokens_of(&definition.generics.where_clause));
                    for bound in &definition.supertraits {
                        header.extend(tokens_of(bound));
                    }
                    self.add(format!("{prefix}trait {name}"), header);
                    for member in &definition.items {
                        let (member_name, member_attributes) = match member {
                            TraitItem::Fn(item) => (item.sig.ident.to_string(), &item.attrs),
                            TraitItem::Const(item) => (item.ident.to_string(), &item.attrs),
                            TraitItem::Type(item) => (item.ident.to_string(), &item.attrs),
                            TraitItem::Macro(item) => (compact(&item.mac.path) + "!", &item.attrs),
                            other => (compact(other), &Vec::new()),
                        };
                        if !for_tests(member_attributes) {
                            let key = format!("{prefix}{name}::{member_name}");
                            let key = if matches!(member, TraitItem::Macro(_)) {
                                self.numbered(key)
                            } else {
                                key
                            };
                            self.add(key, tokens_of(member));
                        }
                    }
                }
                Item::ForeignMod(foreign) => {
                    for member in &foreign.items {
                        let (name, member_attributes) = match member {
                            syn::ForeignItem::Fn(item) => (item.sig.ident.to_string(), &item.attrs),
                            syn::ForeignItem::Static(item) => (item.ident.to_string(), &item.attrs),
                            syn::ForeignItem::Type(item) => (item.ident.to_string(), &item.attrs),
                            other => (compact(other), &Vec::new()),
                        };
                        if for_tests(member_attributes) {
                            continue;
                        }
                        self.add(format!("{prefix}extern::{name}"), tokens_of(member));
                    }
                }
                Item::Mod(module) => {
                    let declaration = module
                        .attrs
                        .iter()
                        .flat_map(tokens_of)
                        .chain(tokens_of(&module.ident))
                        .collect();
                    self.add(format!("{prefix}mod {}", module.ident), declaration);
                    if let Some((_, inner)) = &module.content {
                        self.collect(inner, &format!("{prefix}{}::", module.ident));
                    }
                }
                // Imports are judged by their paths, wherever they are.
                Item::Use(_) => {}
                Item::Macro(invoked) => {
                    let key = match &invoked.ident {
                        Some(name) => format!("{prefix}macro {name}"),
                        None => {
                            let name = compact(&invoked.mac.path);
                            let count = self.calls.entry(format!("{prefix}{name}")).or_default();
                            *count += 1;
                            format!("{prefix}{name}! #{count}")
                        }
                    };
                    self.add(key, tokens_of(invoked));
                }
                Item::Const(item) => {
                    let key = format!("{prefix}{}", item.ident);
                    // Every `const _` is an item of its own.
                    let key = if item.ident == "_" {
                        self.numbered(key)
                    } else {
                        key
                    };
                    self.add(key, tokens_of(item));
                }
                Item::Static(item) => self.add(format!("{prefix}{}", item.ident), tokens_of(item)),
                Item::Struct(item) => self.add(format!("{prefix}{}", item.ident), tokens_of(item)),
                Item::Enum(item) => self.add(format!("{prefix}{}", item.ident), tokens_of(item)),
                Item::Union(item) => self.add(format!("{prefix}{}", item.ident), tokens_of(item)),
                Item::Type(item) => self.add(format!("{prefix}{}", item.ident), tokens_of(item)),
                Item::TraitAlias(item) => {
                    self.add(format!("{prefix}{}", item.ident), tokens_of(item));
                }
                Item::ExternCrate(item) => {
                    self.add(
                        format!("{prefix}extern crate {}", item.ident),
                        tokens_of(item),
                    );
                }
                other => self.add(format!("{prefix}{}", compact(other)), tokens_of(other)),
            }
        }
    }

    fn implementation(&mut self, implementation: &syn::ItemImpl, prefix: &str) {
        let ty = compact(&implementation.self_ty);
        let (header_key, member) = match &implementation.trait_ {
            Some((_, path, _)) => {
                let trait_name = compact(path);
                (
                    format!("{prefix}impl {trait_name} for {ty}"),
                    format!("{prefix}<{ty} as {trait_name}>::"),
                )
            }
            None => (format!("{prefix}impl {ty}"), format!("{prefix}{ty}::")),
        };
        let mut header: Vec<Token> = implementation.attrs.iter().flat_map(tokens_of).collect();
        header.extend(tokens_of(&implementation.generics));
        header.extend(tokens_of(&implementation.generics.where_clause));
        header.extend(tokens_of(&implementation.self_ty));
        if let Some((_, path, _)) = &implementation.trait_ {
            header.extend(tokens_of(path));
        }
        let header_key = self.numbered(header_key);
        self.add(header_key, header);
        for inner in &implementation.items {
            let (name, inner_attributes) = match inner {
                ImplItem::Fn(item) => (item.sig.ident.to_string(), &item.attrs),
                ImplItem::Const(item) => (item.ident.to_string(), &item.attrs),
                ImplItem::Type(item) => (item.ident.to_string(), &item.attrs),
                ImplItem::Macro(item) => (compact(&item.mac.path) + "!", &item.attrs),
                other => (compact(other), &Vec::new()),
            };
            if !for_tests(inner_attributes) {
                let key = format!("{member}{name}");
                let key = if matches!(inner, ImplItem::Macro(_)) {
                    self.numbered(key)
                } else {
                    key
                };
                self.add(key, tokens_of(inner));
            }
        }
    }
}

/// The attributes `node` is printed with first, as an expression's are.
fn leading_attributes(node: &impl ToTokens) -> Vec<Attribute> {
    let parser = |input: syn::parse::ParseStream<'_>| {
        let attributes = input.call(Attribute::parse_outer)?;
        input.parse::<TokenStream>()?;
        Ok(attributes)
    };
    syn::parse::Parser::parse2(parser, node.to_token_stream()).unwrap_or_default()
}

/// Whether the statement `statement` is compiled for tests alone.
fn statement_for_tests(statement: &syn::Stmt) -> bool {
    match statement {
        syn::Stmt::Item(item) => for_tests(attributes(item)),
        syn::Stmt::Local(local) => for_tests(&local.attrs),
        syn::Stmt::Macro(called) => for_tests(&called.attrs),
        syn::Stmt::Expr(expression, _) => for_tests(&leading_attributes(expression)),
    }
}

/// Takes out of a source, at every depth, what tests alone compile, so it
/// counts toward nothing around it.
struct WithoutTests;

impl VisitMut for WithoutTests {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        file.items.retain(|item| !for_tests(attributes(item)));
        visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut module.content {
            items.retain(|item| !for_tests(attributes(item)));
        }
        visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_item_impl_mut(&mut self, implementation: &mut syn::ItemImpl) {
        implementation.items.retain(|member| match member {
            ImplItem::Fn(item) => !for_tests(&item.attrs),
            ImplItem::Const(item) => !for_tests(&item.attrs),
            ImplItem::Type(item) => !for_tests(&item.attrs),
            ImplItem::Macro(item) => !for_tests(&item.attrs),
            _ => true,
        });
        visit_mut::visit_item_impl_mut(self, implementation);
    }

    fn visit_item_trait_mut(&mut self, definition: &mut syn::ItemTrait) {
        definition.items.retain(|member| match member {
            TraitItem::Fn(item) => !for_tests(&item.attrs),
            TraitItem::Const(item) => !for_tests(&item.attrs),
            TraitItem::Type(item) => !for_tests(&item.attrs),
            TraitItem::Macro(item) => !for_tests(&item.attrs),
            _ => true,
        });
        visit_mut::visit_item_trait_mut(self, definition);
    }

    fn visit_item_foreign_mod_mut(&mut self, foreign: &mut syn::ItemForeignMod) {
        foreign.items.retain(|member| match member {
            syn::ForeignItem::Fn(item) => !for_tests(&item.attrs),
            syn::ForeignItem::Static(item) => !for_tests(&item.attrs),
            syn::ForeignItem::Type(item) => !for_tests(&item.attrs),
            _ => true,
        });
        visit_mut::visit_item_foreign_mod_mut(self, foreign);
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        block
            .stmts
            .retain(|statement| !statement_for_tests(statement));
        visit_mut::visit_block_mut(self, block);
    }

    fn visit_fields_named_mut(&mut self, fields: &mut syn::FieldsNamed) {
        fields.named = std::mem::take(&mut fields.named)
            .into_iter()
            .filter(|field| !for_tests(&field.attrs))
            .collect();
        visit_mut::visit_fields_named_mut(self, fields);
    }

    fn visit_fields_unnamed_mut(&mut self, fields: &mut syn::FieldsUnnamed) {
        fields.unnamed = std::mem::take(&mut fields.unnamed)
            .into_iter()
            .filter(|field| !for_tests(&field.attrs))
            .collect();
        visit_mut::visit_fields_unnamed_mut(self, fields);
    }

    fn visit_item_enum_mut(&mut self, definition: &mut syn::ItemEnum) {
        definition.variants = std::mem::take(&mut definition.variants)
            .into_iter()
            .filter(|variant| !for_tests(&variant.attrs))
            .collect();
        visit_mut::visit_item_enum_mut(self, definition);
    }

    fn visit_expr_match_mut(&mut self, matched: &mut syn::ExprMatch) {
        matched.arms.retain(|arm| !for_tests(&arm.attrs));
        visit_mut::visit_expr_match_mut(self, matched);
    }

    fn visit_expr_struct_mut(&mut self, made: &mut syn::ExprStruct) {
        made.fields = std::mem::take(&mut made.fields)
            .into_iter()
            .filter(|field| !for_tests(&field.attrs))
            .collect();
        visit_mut::visit_expr_struct_mut(self, made);
    }

    fn visit_signature_mut(&mut self, signature: &mut syn::Signature) {
        signature.inputs = std::mem::take(&mut signature.inputs)
            .into_iter()
            .filter(|input| match input {
                syn::FnArg::Receiver(receiver) => !for_tests(&receiver.attrs),
                syn::FnArg::Typed(typed) => !for_tests(&typed.attrs),
            })
            .collect();
        visit_mut::visit_signature_mut(self, signature);
    }

    fn visit_expr_array_mut(&mut self, array: &mut syn::ExprArray) {
        array.elems = without_tests(std::mem::take(&mut array.elems));
        visit_mut::visit_expr_array_mut(self, array);
    }

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        call.args = without_tests(std::mem::take(&mut call.args));
        visit_mut::visit_expr_call_mut(self, call);
    }

    fn visit_expr_method_call_mut(&mut self, call: &mut syn::ExprMethodCall) {
        call.args = without_tests(std::mem::take(&mut call.args));
        visit_mut::visit_expr_method_call_mut(self, call);
    }

    fn visit_expr_tuple_mut(&mut self, tuple: &mut syn::ExprTuple) {
        tuple.elems = without_tests(std::mem::take(&mut tuple.elems));
        visit_mut::visit_expr_tuple_mut(self, tuple);
    }
}

/// `expressions` without those compiled for tests alone.
fn without_tests(
    expressions: syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
) -> syn::punctuated::Punctuated<syn::Expr, syn::Token![,]> {
    expressions
        .into_iter()
        .filter(|expression| !for_tests(&leading_attributes(expression)))
        .collect()
}

/// Takes the imports out of bodies, once they are collected: each is judged
/// by its paths alone, so its words count toward nothing around it.
struct WithoutImports;

impl VisitMut for WithoutImports {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        block
            .stmts
            .retain(|statement| !matches!(statement, syn::Stmt::Item(Item::Use(_))));
        visit_mut::visit_block_mut(self, block);
    }
}

/// What a source holds at every depth, once tests' code is taken out: its
/// imports, its type aliases, the macros it calls or defines, and the items
/// declared inside a body, each checked on its own under the names of what
/// encloses it (`helper inside Report::uid`).
#[derive(Default)]
struct Nested {
    imports: Vec<(String, ItemUse)>,
    aliases: Vec<(String, syn::Type)>,
    macros: Vec<Vec<Token>>,
    inner: BTreeMap<String, Vec<Token>>,
    scope: Vec<String>,
    depth: usize,
}

impl Nested {
    fn of(file: &syn::File) -> Self {
        let mut found = Self::default();
        found.visit_file(file);
        found
    }

    /// Record `item`, declared inside a body, by its own items' names.
    fn enclosed(&mut self, item: &Item) {
        let scope = self.scope.join("::");
        for (key, tokens) in Items::of(std::slice::from_ref(item)).tokens {
            let name = format!("{key} inside {scope}");
            let name = if self.inner.contains_key(&name) {
                (2..)
                    .map(|count| format!("{name} #{count}"))
                    .find(|candidate| !self.inner.contains_key(candidate))
                    .expect("some number is free")
            } else {
                name
            };
            self.inner.insert(name, tokens);
        }
    }

    fn within(&mut self, name: String, visit: impl FnOnce(&mut Self)) {
        self.scope.push(name);
        visit(self);
        self.scope.pop();
    }
}

impl<'ast> Visit<'ast> for Nested {
    fn visit_item(&mut self, item: &'ast Item) {
        if self.depth > 0 {
            self.enclosed(item);
        }
        visit::visit_item(self, item);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.depth += 1;
        visit::visit_block(self, block);
        self.depth -= 1;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        self.within(plain(&function.sig.ident), |nested| {
            visit::visit_item_fn(nested, function);
        });
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.within(plain(&function.sig.ident), |nested| {
            visit::visit_impl_item_fn(nested, function);
        });
    }

    fn visit_trait_item_fn(&mut self, function: &'ast syn::TraitItemFn) {
        self.within(plain(&function.sig.ident), |nested| {
            visit::visit_trait_item_fn(nested, function);
        });
    }

    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        let ty = compact(&implementation.self_ty);
        let name = match &implementation.trait_ {
            Some((_, path, _)) => format!("<{ty} as {}>", compact(path)),
            None => ty,
        };
        self.within(name, |nested| {
            visit::visit_item_impl(nested, implementation);
        });
    }

    fn visit_item_trait(&mut self, definition: &'ast syn::ItemTrait) {
        self.within(plain(&definition.ident), |nested| {
            visit::visit_item_trait(nested, definition);
        });
    }

    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        self.within(plain(&module.ident), |nested| {
            visit::visit_item_mod(nested, module);
        });
    }

    fn visit_item_const(&mut self, constant: &'ast syn::ItemConst) {
        self.within(plain(&constant.ident), |nested| {
            visit::visit_item_const(nested, constant);
        });
    }

    fn visit_item_static(&mut self, variable: &'ast syn::ItemStatic) {
        self.within(plain(&variable.ident), |nested| {
            visit::visit_item_static(nested, variable);
        });
    }

    fn visit_item_use(&mut self, import: &'ast ItemUse) {
        self.imports.push((self.scope.join("::"), import.clone()));
    }

    fn visit_item_type(&mut self, alias: &'ast syn::ItemType) {
        self.aliases
            .push((plain(&alias.ident), (*alias.ty).clone()));
        visit::visit_item_type(self, alias);
    }

    fn visit_impl_item_type(&mut self, alias: &'ast syn::ImplItemType) {
        self.aliases.push((plain(&alias.ident), alias.ty.clone()));
        visit::visit_impl_item_type(self, alias);
    }

    fn visit_macro(&mut self, called: &'ast syn::Macro) {
        let mut found = Vec::new();
        flatten(called.tokens.clone(), &mut found);
        self.macros.push(found);
        visit::visit_macro(self, called);
    }
}

/// Whether the macro payload `tokens` imports a kind or the desktop's names.
fn imports_in_macro(tokens: &[Token]) -> bool {
    let has = |word: &str| tokens.iter().any(|token| identifier(token) == Some(word));
    has("use") && (KINDS.iter().any(|kind| has(kind)) || GUARDED.iter().any(|name| has(name)))
}

/// What in the item `key`, of `tokens`, this check cannot follow: an
/// `include!`, or a module declared anywhere but as an item of its own (in a
/// function's body or a macro).
fn hidden_source(key: &str, tokens: &[Token]) -> Option<&'static str> {
    let has = |word: &str| tokens.iter().any(|token| identifier(token) == Some(word));
    let included = tokens
        .windows(2)
        .any(|pair| identifier(&pair[0]) == Some("include") && pair[1] == Token::Punctuation('!'));
    let own_module = key
        .rsplit("::")
        .next()
        .is_some_and(|last| last.starts_with("mod "));
    if included {
        Some("includes a source with `include!`")
    } else if !own_module && has("mod") {
        Some("declares a module inside a function or a macro")
    } else {
        None
    }
}
/// Items outside the kinds' modules that may name the user desktop: the
/// source, from the workspace root; its items; and why. Sorted by source.
const ALLOWED: &[(&str, &[&str], &str)] = &[
    (
        "crates/agents/src/lib.rs",
        &["brief"],
        "owner: the readiness an agent's brief reports",
    ),
    (
        "crates/cli/src/bin/atlas/dispatch.rs",
        &["switch"],
        "owner: showing the person this desktop",
    ),
    (
        "crates/cli/src/bin/atlas/dispatch.rs",
        &["carry_out", "setup", "setup_helper", "switch_on_this_mac"],
        "pending: switching this Mac to the user, its login timing and its setup worker",
    ),
    (
        "crates/cli/src/bin/atlas/doctor.rs",
        &["report"],
        "owner: what doctor reports",
    ),
    (
        "crates/cli/src/bin/atlas/doctor.rs",
        &["user_report"],
        "pending: what doctor reports of the user",
    ),
    (
        "crates/cli/src/bin/atlas/init.rs",
        &[
            "choose_helper",
            "helper_step",
            "install_job",
            "log_in_once",
            "mark_since_lock",
            "offer_lock",
            "permissions_step",
            "run",
            "selected",
            "start_helper",
            "verify_step",
            "wait_until_back",
        ],
        "pending: setting the user up from the terminal",
    ),
    (
        "crates/cli/src/bin/atlas/main.rs",
        &["or_logged_out"],
        "owner: why the desktop is silent",
    ),
    (
        "crates/cli/src/bin/atlas/main.rs",
        &["main", "open_app"],
        "pending: whether the user's setup is complete, and its first-login opener",
    ),
    (
        "crates/cli/src/bin/atlas/update.rs",
        &["preflight", "update_finish_using"],
        "pending: finishing an update in the user",
    ),
    (
        "crates/cli/src/mcp.rs",
        &["Server::switch_helper"],
        "owner: showing the person this desktop",
    ),
    (
        "crates/cli/src/mcp.rs",
        &["Server::switch_to_user"],
        "pending: switching this Mac to the user",
    ),
    (
        "crates/cli/src/onboarding.rs",
        &[
            "accept",
            "asked",
            "check_choice",
            "choosable",
            "choose",
            "close_all",
            "close_attended",
            "close_helper",
            "close_leftovers",
            "closing_first",
            "closing_first_using",
            "controller_of",
            "create",
            "create_helper",
            "created",
            "expected_agent",
            "files",
            "finish",
            "forget",
            "keep_other_helper",
            "leftover_logins",
            "leftover_targets",
            "orphans_now",
            "prepared",
            "requested",
            "use_helper",
            "window",
        ],
        "pending: choosing and preparing the user's account",
    ),
    (
        "crates/cli/src/onboarding/acceptance.rs",
        &[
            "accept_worker",
            "accepted",
            "accepted_at",
            "allow",
            "complete_setup",
            "drive",
            "observe",
            "reconcile",
            "resume",
            "serve_channel",
            "signal",
            "verify",
        ],
        "pending: the setup connection and its acceptance",
    ),
    (
        "crates/cli/src/onboarding/as_root.rs",
        &["create_in_terminal"],
        "pending: creating the user's account as root",
    ),
    (
        "crates/cli/src/onboarding/status.rs",
        &[
            "candidates",
            "configured",
            "helper_job_here",
            "helper_side",
            "logins",
            "status",
        ],
        "pending: how far the user's setup has come",
    ),
    (
        "crates/cli/src/ui/render/act.rs",
        &["opened"],
        "exception: the link: copies are made on a shared disk, and these words name its folder",
    ),
    (
        "crates/cli/src/uninstall.rs",
        &[
            "<Live<'_> as Teardown>::log_out",
            "<Live<'_> as Teardown>::logged_out",
            "<Live<'_> as Teardown>::remove_helper",
            "<Live<'_> as Teardown>::retire",
            "Live<'_>::forget_requested_opener",
            "Live<'_>::trash",
            "handed_over",
            "home_note",
            "retire_helper_job",
            "retirement_request",
            "run",
            "settle",
        ],
        "pending: removing the user's account and job",
    ),
    (
        "crates/cli/src/uninstall/inspection.rs",
        &["survey"],
        "pending: what removing the user would take",
    ),
    (
        "crates/cli/src/uninstall/log_out.rs",
        &["log_out_helper", "logged_out_twice", "root_log_out"],
        "pending: logging the user out before removal",
    ),
    (
        "crates/cli/src/viewer/reads.rs",
        &["Viewer::status_for"],
        "owner: the name the desktop's account goes by",
    ),
    (
        "crates/client/src/agent/elements.rs",
        &["probe"],
        "owner: the inspection tool the desktop runs",
    ),
    (
        "crates/client/src/agent/elements.rs",
        &["probe_in"],
        "pending: the user's inspection tool",
    ),
    (
        "crates/client/src/app_selection.rs",
        &["atlas_bundle_in", "installed"],
        "owner: which apps the desktop has installed, and which is Atlas",
    ),
    (
        "crates/client/src/client.rs",
        &[
            "Client::session_proof",
            "Client::verified_status",
            "Client::writable",
        ],
        "owner: getting an observation of the desktop, and where builds are staged",
    ),
    (
        "crates/client/src/client.rs",
        &["Client::new"],
        "exception: the link, a local socket to the user",
    ),
    (
        "crates/client/src/client.rs",
        &["Client::session_proof_by_command"],
        "pending: the user's legacy session proof",
    ),
    (
        "crates/client/src/client/input.rs",
        &["Client::input_transport"],
        "exception: the link, a local socket to the user",
    ),
    (
        "crates/client/src/client/install.rs",
        &["Client::install_probe", "Client::installed_build"],
        "owner: installing the shared Atlas command, and which build it is",
    ),
    (
        "crates/client/src/client/install.rs",
        &["Client::build_on_this_disk", "Client::install_on_this_disk"],
        "pending: the user's shared Atlas command on this disk",
    ),
    (
        "crates/client/src/client/service.rs",
        &["Client::spawn_input", "Client::stop_input"],
        "owner: starting and stopping the desktop's input service",
    ),
    (
        "crates/client/src/client/service.rs",
        &["Client::launch_input_in", "Client::spawn_input_in"],
        "pending: starting the user's input service",
    ),
    (
        "crates/client/src/client/service_change.rs",
        &["Client::require_service_idle_for"],
        "owner: whether replacing its services is a first setup",
    ),
    (
        "crates/client/src/client/session.rs",
        &["Client::desktop_gone"],
        "owner: whether the desktop's session has ended",
    ),
    (
        "crates/client/src/client/session.rs",
        &["Client::user_gone"],
        "pending: whether the user's session has ended",
    ),
    (
        "crates/client/src/client/stop.rs",
        &["Client::service_stop_saved"],
        "owner: the Stop the desktop's input service keeps",
    ),
    (
        "crates/client/src/collect.rs",
        &["collect"],
        "owner: which paths are the person's own rather than the desktop's",
    ),
    (
        "crates/client/src/environment.rs",
        &["apply", "rollback", "run", "sync"],
        "owner: applying, rolling back, running and syncing an environment",
    ),
    (
        "crates/client/src/environment/list.rs",
        &["list"],
        "owner: listing environments",
    ),
    (
        "crates/client/src/environment/one_off.rs",
        &["install_app", "install_states", "remove_app"],
        "owner: installing and removing an app, and how installs stand",
    ),
    (
        "crates/client/src/environment/profile.rs",
        &["sync"],
        "owner: freezing an environment profile",
    ),
    (
        "crates/client/src/environment/undo.rs",
        &["plan_states", "undo_plan"],
        "owner: how environment plans stand, and undoing one",
    ),
    (
        "crates/client/src/handoff.rs",
        &[
            "Copies::app",
            "Copies::exists",
            "Copies::identity",
            "Copies::location",
            "Copies::of",
            "Copies::prune",
            "Copies::remove",
            "Copies::remove_of_app",
            "Copies::token_of",
            "hand_over",
        ],
        "owner: where the desktop's copies are, and handing files over",
    ),
    (
        "crates/client/src/handoff.rs",
        &["Copies::in_shared_folder"],
        "exception: records do not say which desktop made the copies they name",
    ),
    (
        "crates/client/src/host_session.rs",
        &["from_env_verified"],
        "owner: who proves which agent host a command came from",
    ),
    (
        "crates/client/src/host_session.rs",
        &["verified_by_user"],
        "pending: proving an agent host to the user",
    ),
    (
        "crates/client/src/jobs.rs",
        &["daemon_listens", "started"],
        "owner: whether the daemon listens, and that a started job is the one asked for",
    ),
    (
        "crates/client/src/lib.rs",
        &["mod user"],
        "declares: the user desktop's module",
    ),
    (
        "crates/client/src/overview.rs",
        &["report"],
        "owner: the report of the desktop's readiness",
    ),
    (
        "crates/client/src/overview.rs",
        &["logins", "user_report"],
        "pending: the report of the user's readiness",
    ),
    (
        "crates/core/src/config.rs",
        &[
            "Config",
            "Config::desktop",
            "Config::parse",
            "Config::same_helper",
        ],
        "exception: the flat settings file, which names one user",
    ),
    (
        "crates/core/src/desktop.rs",
        &[
            "<Desktop as From<crate::user::User>>::from",
            "Desktop::exited",
            "Desktop::id",
            "Desktop::input_current",
            "Desktop::instance",
            "Desktop::left_may_run",
            "Desktop::orphans_run",
            "Desktop::prove",
            "Kind",
            "impl From<crate::user::User> for Desktop",
        ],
        "owner: the kinds of desktop, and the identity, proof, readiness and processes of each",
    ),
    (
        "crates/core/src/desktop.rs",
        &["Desktop::user"],
        "exception: the way in for the user's own setup, update and removal",
    ),
    (
        "crates/core/src/lib.rs",
        &["mod user"],
        "declares: the user desktop's module",
    ),
    (
        "crates/daemon/src/daemon/consent.rs",
        &["arrival_notice", "guided", "just_logged_in", "notice"],
        "exception: the daemon serves a user: its switch-back notice and setup guidance",
    ),
    (
        "crates/daemon/src/daemon/serve.rs",
        &["Daemon::hello", "Daemon::installed"],
        "exception: the daemon serves a user: its login timing and console agent",
    ),
    (
        "crates/daemon/src/lib.rs",
        &["mod user"],
        "declares: the user desktop's module",
    ),
];

/// Shared assumptions about the user desktop kept on purpose until another
/// kind is built: the source, the item, and what it assumes. Many never
/// name the user desktop, so each is only checked to still exist.
const KEPT: &[(&str, &str, &str)] = &[
    (
        "crates/cli/src/ui/render/act.rs",
        "opened",
        "the link: copies are made on a shared disk, and these words name its folder",
    ),
    (
        "crates/client/src/client.rs",
        "Client::new",
        "the link: commands go over a local socket",
    ),
    (
        "crates/client/src/client/input.rs",
        "Client::input_transport",
        "the link: input goes over a local socket",
    ),
    (
        "crates/client/src/client/install.rs",
        "Client::installed_build",
        "installed builds are read from this disk, cached by file identity",
    ),
    (
        "crates/client/src/console_user.rs",
        "Named",
        "what a login switch does to the person's Mac",
    ),
    (
        "crates/client/src/environment/discovery.rs",
        "inspect",
        "environment references are the same paths on both sides",
    ),
    (
        "crates/client/src/host_session/store.rs",
        "Store::at",
        "records do not say which desktop made them",
    ),
    (
        "crates/client/src/idle_time.rs",
        "IdleTime",
        "what a login switch does to the person's Mac",
    ),
    (
        "crates/client/src/setup/installed_app.rs",
        "link_installed_command",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/client/src/update.rs",
        "install_in",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/client/src/update.rs",
        "resume",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/client/src/update.rs",
        "swap_using",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/core/src/app_bundle.rs",
        "app_executable",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/core/src/app_bundle.rs",
        "require_helper_access",
        "this Mac's install must be readable by another account",
    ),
    (
        "crates/core/src/config.rs",
        "Config::desktop",
        "the settings name one user",
    ),
    (
        "crates/core/src/desktop.rs",
        "Desktop::user",
        "every desktop is a user: the way in for its setup, update and removal",
    ),
    (
        "crates/core/src/host/verification.rs",
        "verify",
        "agents are verified in the same kernel",
    ),
    (
        "crates/core/src/session.rs",
        "Observation::require_hidden",
        "the hidden-session proof and its guards",
    ),
    (
        "crates/core/src/session.rs",
        "validate_target_uid",
        "the link: the controller is another account here",
    ),
    (
        "crates/daemon/src/daemon/consent.rs",
        "arrival_notice",
        "the daemon serves a user: its switch-back notice",
    ),
    (
        "crates/daemon/src/daemon/consent.rs",
        "guided",
        "the daemon serves a user: its setup guidance",
    ),
    (
        "crates/daemon/src/daemon/console_agent.rs",
        "ConsoleAgent",
        "the daemon serves a user: calming what it disturbs on the person's Mac",
    ),
    (
        "crates/daemon/src/daemon/serve.rs",
        "Daemon::hello",
        "the daemon serves a user: it reports the console agent",
    ),
    (
        "crates/daemon/src/daemon/serve.rs",
        "handle",
        "agents are verified in the same kernel",
    ),
    (
        "crates/daemon/src/daemon/serve.rs",
        "serve",
        "the daemon serves a user: two accounts at launch",
    ),
    (
        "crates/native/src/native/endpoint.rs",
        "Directory",
        "the link: the controller account may reach the socket's folder",
    ),
    (
        "crates/native/src/native/service.rs",
        "admitted",
        "the link: only the controller account is admitted",
    ),
];

/// How many pending entries are left. Each move lowers it; none raises it.
const PENDING_LIMIT: usize = 20;

/// The roots of a member's targets: its library, its main binary and the
/// binaries Cargo finds under `src/bin`.
fn target_roots(member: &Path) -> Vec<PathBuf> {
    let source = member.join("src");
    let mut roots: Vec<PathBuf> = ["lib.rs", "main.rs"]
        .iter()
        .map(|name| source.join(name))
        .filter(|path| path.is_file())
        .collect();
    if let Ok(entries) = fs::read_dir(source.join("bin")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let root = if path.is_dir() {
                path.join("main.rs")
            } else {
                path
            };
            if root.extension().is_some_and(|extension| extension == "rs") && root.is_file() {
                roots.push(root);
            }
        }
    }
    roots.sort();
    roots
}

/// Every Rust source under `folder`.
fn sources_under(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// What the check found in a workspace, each a list of what is wrong.
#[derive(Debug, Default)]
struct Findings {
    /// Sources it could not follow, read or place.
    problems: Vec<String>,
    /// Items that name the user desktop without being listed.
    unlisted: Vec<String>,
    /// Imports it refuses, and type aliases that hide the desktop's names.
    refused: Vec<String>,
    /// Listed items that no longer name the user desktop, or are listed
    /// under a role they cannot have.
    stale: Vec<String>,
    /// Kept assumptions whose item is gone.
    gone: Vec<String>,
}

impl Findings {
    /// Everything found, each line led by what kind of finding it is.
    fn all(&self) -> Vec<String> {
        [
            ("problem", &self.problems),
            ("unlisted", &self.unlisted),
            ("refused", &self.refused),
            ("stale", &self.stale),
            ("gone", &self.gone),
        ]
        .into_iter()
        .flat_map(|(what, lines)| lines.iter().map(move |line| format!("{what}: {line}")))
        .collect()
    }
}

/// Check the members under `root` against `allowed` and `kept`.
fn findings(
    root: &Path,
    members: &[PathBuf],
    allowed: &[(&str, &[&str], &str)],
    kept: &[(&str, &str, &str)],
) -> Findings {
    let roots: Vec<PathBuf> = members
        .iter()
        .flat_map(|member| target_roots(member))
        .collect();
    let tree = Tree::of(root, &roots);
    let mut found = Findings {
        problems: tree.problems.clone(),
        ..Findings::default()
    };

    let mut on_disk = Vec::new();
    for member in members {
        sources_under(&member.join("src"), &mut on_disk);
    }
    on_disk.sort();
    for path in on_disk {
        let name = tree.named(&path);
        if !tree.sources.contains_key(&name) {
            found.problems.push(format!(
                "{} is compiled by no target this check can follow",
                name.display()
            ));
        }
    }

    let mut naming: BTreeSet<(String, String)> = BTreeSet::new();
    let mut existing: BTreeSet<(String, String)> = BTreeSet::new();
    for (source, reached) in &tree.sources {
        if reached.tests {
            continue;
        }
        let path = source.to_string_lossy().into_owned();
        let Ok(mut parsed) = fs::read_to_string(tree.root.join(source))
            .map_err(|error| error.to_string())
            .and_then(|text| syn::parse_file(&text).map_err(|error| error.to_string()))
        else {
            continue;
        };
        WithoutTests.visit_file_mut(&mut parsed);
        let nested = Nested::of(&parsed);
        WithoutImports.visit_file_mut(&mut parsed);
        let items = Items::of(&parsed.items);
        let inner = Nested::of(&parsed).inner;
        for (key, tokens) in &items.tokens {
            existing.insert((path.clone(), key.clone()));
            if let Some(how) = hidden_source(key, tokens) {
                found.problems.push(format!(
                    "{path}: {key} {how}, which this check cannot follow"
                ));
            }
            if reached.kind.is_some() {
                continue;
            }
            let declares_a_kind = key
                .strip_prefix("mod ")
                .is_some_and(|module| KINDS.contains(&module));
            if declares_a_kind || names_the_user(tokens) {
                naming.insert((path.clone(), key.clone()));
            }
        }
        for (scope, import) in &nested.imports {
            for imported in user_imports(import) {
                let key = if scope.is_empty() {
                    format!("use {imported}")
                } else {
                    format!("use {imported} inside {scope}")
                };
                existing.insert((path.clone(), key.clone()));
                if reached.kind.is_none() {
                    naming.insert((path.clone(), key));
                }
            }
        }
        for (key, tokens) in &inner {
            if reached.kind.is_none() && names_the_user(tokens) {
                naming.insert((path.clone(), key.clone()));
            }
        }
        if reached.kind.is_some() {
            continue;
        }
        if nested.macros.iter().any(|tokens| imports_in_macro(tokens)) {
            found.problems.push(format!(
                "{path}: a macro imports a kind or the desktop's names, which this check cannot follow"
            ));
        }
        for (_, import) in &nested.imports {
            if let Some(why) = refused_import(import) {
                found
                    .refused
                    .push(format!("{path}: `{}` {why}", compact(import)));
            }
        }
        for (name, target) in &nested.aliases {
            if hides_the_user(target) {
                found.refused.push(format!(
                    "{path}: type `{name}` hides the desktop's names behind a type alias"
                ));
            }
        }
    }

    for (file, key) in &naming {
        let listed = allowed
            .iter()
            .any(|(listed, items, _)| listed == file && items.contains(&key.as_str()));
        if !listed {
            found.unlisted.push(format!("{file}: {key}"));
        }
    }
    for (file, items, role) in allowed {
        for item in *items {
            if !naming.contains(&((*file).to_owned(), (*item).to_owned())) {
                found.stale.push(format!(
                    "{file}: {item} is listed and does not name the user desktop"
                ));
            }
            if role.starts_with("declares: ") && !item.starts_with("mod ") {
                found.stale.push(format!(
                    "{file}: {item} is listed as a declaration and is not one"
                ));
            }
        }
    }
    for (file, item, _) in kept {
        if !existing.contains(&((*file).to_owned(), (*item).to_owned())) {
            found
                .gone
                .push(format!("{file}: {item} is kept and no longer exists"));
        }
    }
    found
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two folders above this crate")
}

/// Every member of the workspace: each crate, and `xtask`.
fn members(root: &Path) -> Vec<PathBuf> {
    let mut members: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .expect("the crates folder can be listed")
        .map(|entry| entry.expect("a crate folder can be read").path())
        .filter(|path| path.is_dir())
        .collect();
    members.push(root.join("xtask"));
    members.sort();
    members
}

#[test]
fn only_listed_items_name_the_user_desktop() {
    let root = workspace_root();
    let found = findings(&root, &members(&root), ALLOWED, KEPT);
    assert!(
        found.problems.is_empty(),
        "the check could not follow these sources: {:#?}",
        found.problems
    );
    assert!(
        found.refused.is_empty(),
        "write a kind's full path in its owner instead of importing it, and keep the \
         desktop's names as they are: {:#?}",
        found.refused
    );
    assert!(
        found.unlisted.is_empty(),
        "these items name the user desktop and are not listed; call the operation's \
         owner instead: {:#?}",
        found.unlisted
    );
    assert!(
        found.stale.is_empty(),
        "remove or correct these entries: {:#?}",
        found.stale
    );
    assert!(
        found.gone.is_empty(),
        "these kept assumptions are gone: {:#?}",
        found.gone
    );
}

#[test]
fn every_listed_item_says_why() {
    let mut seen = BTreeSet::new();
    for (file, items, role) in ALLOWED {
        assert!(
            ROLES.iter().any(|prefix| role.starts_with(prefix)),
            "{file} says neither what it owns nor why it may name the user desktop: {role}"
        );
        assert!(!items.is_empty(), "{file} lists no items");
        for item in *items {
            assert!(
                seen.insert((*file, *item)),
                "{file}: {item} is listed twice"
            );
        }
    }
    let files: Vec<&str> = ALLOWED.iter().map(|(file, _, _)| *file).collect();
    assert!(files.is_sorted(), "the list is kept in order of source");
    let kept: Vec<(&str, &str)> = KEPT.iter().map(|(file, item, _)| (*file, *item)).collect();
    assert!(kept.is_sorted(), "the kept list is kept in order");
}

#[test]
fn pending_entries_only_shrink() {
    // Pending entries mark the user desktop's own code still to move. Each
    // move removes its own and lowers the limit, until both reach zero.
    let pending = ALLOWED
        .iter()
        .filter(|(_, _, role)| role.starts_with("pending: "))
        .count();
    assert!(
        pending <= PENDING_LIMIT,
        "{pending} pending entries; no move may add one (limit {PENDING_LIMIT})"
    );
}

#[test]
fn comments_and_literals_do_not_name_the_user_desktop() {
    for source in [
        "// User and .user() in a line comment",
        "/// helper_uid in a doc comment\nfn f() {}",
        "/* User /* nested helper_uid */ still a comment */ fn f() {}",
        r#"let key = "helper_uid";"#,
        r#"let key = config["helper_uid"];"#,
        r#"let escaped = "a \" helper_uid \" b";"#,
        r##"let raw = r#"User "quoted" helper_uid"#;"##,
        r#"let bytes = b"helper_uid"; let text = c"user";"#,
        r#"json!({"helper_uid": 1, "User": 2})"#,
        r#"#[serde(rename = "helper_uid")] struct Kept;"#,
        "let quote = '\"'; let other = '\\''; let tick = 'h';",
    ] {
        assert!(!names_the_user(&tokens(source)), "{source}");
    }
}

#[test]
fn every_identifier_use_names_the_user_desktop() {
    for source in [
        "let login = desktop.user();",
        "let uid = client\n    .desktop()\n    .user()\n    .uid;",
        "match desktop.kind() { Kind::User(login) => login.uid }",
        "Desktop::from(User { uid })",
        "struct Pairing { helper_uid: u32 }",
        "let Config { helper_uid, .. } = config;",
        "json!({\"helperUID\": pairing.helper_uid})",
        "fn held<'a>(login: &'a User) {}",
        "let text = \"closed\"; let uid = config.helper_uid;",
        "let r#type = desktop.r#user();",
        "let account = Desktop::user(&desktop);",
        "let account = desktop . user ( );",
        "let accessor = Desktop::user; let account = accessor(&desktop);",
        "let account = (Desktop::r#user)(&desktop);",
        "let account = desktop.user::<>();",
        "let account = desktop.r#user::<>();",
        "let made = atlas_core::desktop::User { uid };",
        "let made = crate::user::User { uid };",
        "user::copies::run();",
    ] {
        assert!(names_the_user(&tokens(source)), "{source}");
    }
}

#[test]
fn a_plain_user_does_not_name_the_user_desktop() {
    for source in [
        "let user = account; let uid = user.uid;",
        "fn user() -> Value { json!({}) }",
        r#"json!({"user": user(), "session": session()})"#,
        "let named = facts.user; let field = report.user.name;",
        "fn of(user: &Account) -> u32 { user.uid }",
        "let account = nix::unistd::User::from_uid(uid);",
    ] {
        assert!(!names_the_user(&tokens(source)), "{source}");
    }
}

fn import(source: &str) -> ItemUse {
    syn::parse_str(source).unwrap_or_else(|error| panic!("{source}: {error}"))
}

#[test]
fn imports_are_judged_by_their_paths() {
    for (source, why) in [
        (
            "use crate::user::copies;",
            "imports a kind's module from outside it",
        ),
        (
            "use crate::{user as u};",
            "imports a kind's module from outside it",
        ),
        (
            "use crate::{user::copies, handoff::Copies};",
            "imports a kind's module from outside it",
        ),
        (
            "pub use super::user::*;",
            "imports a kind's module from outside it",
        ),
        (
            "use atlas_core::desktop::User as Login;",
            "renames the desktop's names with `as`",
        ),
        (
            "use atlas_core::desktop::Kind::{self as K};",
            "renames the desktop's names with `as`",
        ),
        (
            "use atlas_core::{desktop as place};",
            "renames the desktop's names with `as`",
        ),
        (
            "use atlas_core::desktop::*;",
            "imports the desktop module's items with a glob",
        ),
        (
            "use atlas_core::{desktop::{Desktop, Kind::*}, error::Result};",
            "imports the desktop module's items with a glob",
        ),
    ] {
        assert_eq!(refused_import(&import(source)), Some(why), "{source}");
    }
    for source in [
        "use atlas_core::desktop::{Desktop, Kind, User};",
        "use crate::handoff::Copies;",
        "use nix::unistd::{User};",
        "use nix::unistd::User as UnixUser;",
        "use crate::account::user_name;",
        "use atlas_core::{desktop::Desktop, session::*};",
    ] {
        assert_eq!(refused_import(&import(source)), None, "{source}");
    }
    assert!(imports_the_user(&import(
        "use atlas_core::desktop::{Desktop, User};"
    )));
    assert!(imports_the_user(&import("use super::{Kind, User};")));
    assert!(!imports_the_user(&import("use nix::unistd::{User};")));
}

#[test]
fn only_a_build_for_tests_alone_counts_as_tests() {
    for (source, alone) in [
        ("#[cfg(test)] fn f() {}", true),
        (
            "#[cfg(any(test, feature = \"test-support\"))] fn f() {}",
            true,
        ),
        ("#[cfg(all(test, unix))] fn f() {}", true),
        ("#[cfg(any(test, unix))] fn f() {}", false),
        ("#[cfg(not(test))] fn f() {}", false),
        ("#[cfg(feature = \"other\")] fn f() {}", false),
        ("#[cfg(target_os = \"macos\")] fn f() {}", false),
    ] {
        let parsed: syn::ItemFn = syn::parse_str(source).expect("the item parses");
        assert_eq!(for_tests(&parsed.attrs), alone, "{source}");
    }
}

/// A throwaway workspace with one member, `crates/a`, holding `files`.
fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let member = folder.path().join("crates/a");
    for (path, text) in files {
        let file = member.join(path);
        fs::create_dir_all(file.parent().expect("a source has a folder"))
            .expect("the folder can be made");
        fs::write(&file, text).expect("the source can be written");
    }
    (folder, member)
}

/// Everything the check finds in a workspace of `files`.
fn check(files: &[(&str, &str)], allowed: &[(&str, &[&str], &str)]) -> Vec<String> {
    let (folder, member) = workspace(files);
    findings(folder.path(), &[member], allowed, &[]).all()
}

#[test]
fn an_owner_names_the_user_desktop_and_its_neighbour_may_not() {
    let files = [(
        "src/lib.rs",
        "fn owner(desktop: &Desktop) { match desktop.kind() { Kind::User(u) => go(u) } }\n\
         fn neighbour(desktop: &Desktop) -> u32 { desktop.user().uid }",
    )];
    let found = check(
        &files,
        &[("crates/a/src/lib.rs", &["owner"], "owner: going")],
    );
    assert_eq!(found, ["unlisted: crates/a/src/lib.rs: neighbour"]);
}

#[test]
fn a_kind_module_is_free_and_reaching_into_it_from_elsewhere_is_refused() {
    let files = [
        ("src/lib.rs", "mod user;\nmod shared;"),
        (
            "src/user.rs",
            "pub fn copy(user: &User) -> u32 { user.uid }",
        ),
        (
            "src/shared.rs",
            "use crate::user::copy;\n\
             fn reach(account: &Account) { copy(account) }\n\
             fn inside() { use crate::{user as u}; u::copy(&account()) }",
        ),
    ];
    let found = check(
        &files,
        &[(
            "crates/a/src/lib.rs",
            &["mod user"],
            "declares: the user module",
        )],
    );
    assert_eq!(
        found,
        [
            "refused: crates/a/src/shared.rs: `usecrate::user::copy;` imports a kind's module from outside it",
            "refused: crates/a/src/shared.rs: `usecrate::{userasu};` imports a kind's module from outside it",
        ]
    );
}

#[test]
fn a_kind_module_is_declared_only_where_listed() {
    let files = [("src/lib.rs", "mod user;"), ("src/user.rs", "")];
    assert_eq!(
        check(&files, &[]),
        ["unlisted: crates/a/src/lib.rs: mod user"]
    );
    assert!(
        check(
            &files,
            &[(
                "crates/a/src/lib.rs",
                &["mod user"],
                "declares: the user module"
            )]
        )
        .is_empty()
    );
}

#[test]
fn a_kind_renamed_and_matched_elsewhere_is_refused() {
    let files = [(
        "src/lib.rs",
        "use atlas_core::desktop::Kind as K;\n\
         fn which(desktop: &Desktop) -> u32 { match desktop.kind() { K::User(u) => u.uid } }",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "unlisted: crates/a/src/lib.rs: which",
            "refused: crates/a/src/lib.rs: `useatlas_core::desktop::KindasK;` renames the desktop's names with `as`",
        ]
    );
}

#[test]
fn a_module_named_like_tests_without_its_gate_is_production() {
    let files = [
        (
            "src/lib.rs",
            "mod tests;\n#[cfg(test)] mod gated;\n#[cfg(any(test, unix))] mod shipped;",
        ),
        ("src/tests.rs", "fn made() -> User { todo!() }"),
        ("src/gated.rs", "fn made() -> User { todo!() }"),
        ("src/shipped.rs", "fn made() -> User { todo!() }"),
    ];
    assert_eq!(
        check(&files, &[]),
        [
            "unlisted: crates/a/src/shipped.rs: made",
            "unlisted: crates/a/src/tests.rs: made",
        ]
    );
}

#[test]
fn a_test_gate_hides_an_item_and_a_not_test_gate_does_not() {
    let files = [(
        "src/lib.rs",
        "#[cfg(test)] fn fixture() -> User { todo!() }\n\
         #[cfg(any(test, feature = \"test-support\"))] fn support() -> User { todo!() }\n\
         #[cfg(not(test))] fn shipped() -> User { todo!() }",
    )];
    assert_eq!(
        check(&files, &[]),
        ["unlisted: crates/a/src/lib.rs: shipped"]
    );
}

#[test]
fn a_path_into_a_kind_from_outside_it_is_refused() {
    let files = [
        (
            "src/lib.rs",
            "mod user;\n#[path = \"user/copies.rs\"] mod borrowed;",
        ),
        ("src/user.rs", "mod copies;"),
        ("src/user/copies.rs", "pub fn copy(user: &User) {}"),
    ];
    let found = check(
        &files,
        &[(
            "crates/a/src/lib.rs",
            &["mod user"],
            "declares: the user module",
        )],
    );
    assert_eq!(
        found,
        ["problem: crates/a/src/user/copies.rs is reached both inside and outside a kind's module"]
    );
}

#[test]
fn a_path_out_of_a_kind_into_shared_code_is_refused() {
    let files = [
        ("src/lib.rs", "mod user;\nmod shared;"),
        ("src/user.rs", "#[path = \"shared.rs\"] mod borrowed;"),
        ("src/shared.rs", "pub fn copy() {}"),
    ];
    let found = check(
        &files,
        &[(
            "crates/a/src/lib.rs",
            &["mod user"],
            "declares: the user module",
        )],
    );
    assert_eq!(
        found,
        ["problem: crates/a/src/shared.rs is reached both inside and outside a kind's module"]
    );
}

#[test]
fn a_path_is_followed_as_the_compiler_follows_it() {
    let files = [
        ("src/lib.rs", "mod user;\nmod shared;"),
        // A `#[path]` source's own modules are found beside it.
        (
            "src/user.rs",
            "#[path = \"user/moved_copies.rs\"] mod copies;",
        ),
        ("src/user/moved_copies.rs", "mod helper;"),
        ("src/user/helper.rs", "pub fn copy(user: &User) {}"),
        // A shared item reached through `#[path]` is checked like any other.
        ("src/shared.rs", "#[path = \"listed.rs\"] mod listed;"),
        (
            "src/listed.rs",
            "pub fn owner(desktop: &Desktop) -> u32 { desktop.user().uid }",
        ),
    ];
    let allowed: &[(&str, &[&str], &str)] = &[
        (
            "crates/a/src/lib.rs",
            &["mod user"],
            "declares: the user module",
        ),
        ("crates/a/src/listed.rs", &["owner"], "owner: reaching it"),
    ];
    assert_eq!(check(&files, allowed), Vec::<String>::new());
}

#[test]
fn a_spelling_through_a_kind_still_names_the_same_source() {
    let files = [
        ("src/lib.rs", "mod user;\nmod shared;"),
        ("src/user.rs", "#[path = \"../src/shared.rs\"] mod again;"),
        ("src/shared.rs", "pub fn copy() {}"),
    ];
    let found = check(
        &files,
        &[(
            "crates/a/src/lib.rs",
            &["mod user"],
            "declares: the user module",
        )],
    );
    assert_eq!(
        found,
        ["problem: crates/a/src/shared.rs is reached both inside and outside a kind's module"]
    );
}

#[test]
fn a_declaration_entry_covers_only_a_declaration() {
    let files = [
        (
            "src/lib.rs",
            "mod user;\nfn owner(desktop: &Desktop) -> u32 { desktop.user().uid }",
        ),
        ("src/user.rs", ""),
    ];
    let found = check(
        &files,
        &[(
            "crates/a/src/lib.rs",
            &["mod user", "owner"],
            "declares: the user module",
        )],
    );
    assert_eq!(
        found,
        ["stale: crates/a/src/lib.rs: owner is listed as a declaration and is not one"]
    );
}

#[test]
fn a_listed_item_that_stopped_naming_the_user_desktop_is_stale() {
    let files = [("src/lib.rs", "fn owner() {}")];
    let found = check(
        &files,
        &[("crates/a/src/lib.rs", &["owner"], "owner: nothing")],
    );
    assert_eq!(
        found,
        ["stale: crates/a/src/lib.rs: owner is listed and does not name the user desktop"]
    );
}

#[test]
fn a_source_no_target_compiles_is_reported() {
    let files = [
        ("src/lib.rs", "fn shared() {}"),
        ("src/stray.rs", "fn lost() {}"),
    ];
    assert_eq!(
        check(&files, &[]),
        ["problem: crates/a/src/stray.rs is compiled by no target this check can follow"]
    );
}

#[test]
fn an_include_or_a_macro_made_module_is_refused() {
    let files = [(
        "src/lib.rs",
        "fn made() { include!(\"made.rs\"); }\n\
         make! { mod hidden; }\n\
         #[cfg(test)] fn fixture() { include!(\"fixture.rs\"); }",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "problem: crates/a/src/lib.rs: made includes a source with `include!`, which this check cannot follow",
            "problem: crates/a/src/lib.rs: make! #1 declares a module inside a function or a macro, which this check cannot follow",
        ]
    );
}

#[test]
fn items_are_named_by_where_they_are() {
    let parsed = syn::parse_file(
        "use crate::desktop::User;\n\
         impl Client { fn new(user: User) -> Self { todo!() } #[cfg(test)] fn fake() {} }\n\
         impl From<User> for Desktop { fn from(user: User) -> Self { todo!() } }\n\
         impl<T> Wrapper<T> where T: Into<User> { fn wrap() {} }\n\
         trait Teardown { fn log_out(&self); }\n\
         mod inner { fn deep() {} }\n\
         mod declared;\n\
         run! {}\n\
         run! {}",
    )
    .expect("the source parses");
    let items = Items::of(&parsed.items);
    let keys: Vec<&str> = items.tokens.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "<Desktop as From<User>>::from",
            "Client::new",
            "Teardown::log_out",
            "Wrapper<T>::wrap",
            "impl Client",
            "impl From<User> for Desktop",
            "impl Wrapper<T>",
            "inner::deep",
            "mod declared",
            "mod inner",
            "run! #1",
            "run! #2",
            "trait Teardown",
        ]
    );
    assert!(names_the_user(&items.tokens["impl Wrapper<T>"]));
}

#[test]
fn a_module_hidden_in_a_body_a_macro_or_a_conditional_path_is_refused() {
    let files = [
        (
            "src/lib.rs",
            "fn run() { #[path = \"shared.rs\"] mod hidden; }\n\
             macro_rules! make { ($name:ident) => { mod $name; }; }\n\
             #[cfg_attr(not(test), path = \"prod.rs\")] mod normal;",
        ),
        ("src/normal.rs", ""),
        ("src/prod.rs", ""),
        ("src/shared.rs", ""),
    ];
    assert_eq!(
        check(&files, &[]),
        [
            "problem: module `normal` takes its path from `cfg_attr`, which this check cannot follow",
            "problem: crates/a/src/normal.rs is compiled by no target this check can follow",
            "problem: crates/a/src/prod.rs is compiled by no target this check can follow",
            "problem: crates/a/src/shared.rs is compiled by no target this check can follow",
            "problem: crates/a/src/lib.rs: macro make declares a module inside a function or a macro, which this check cannot follow",
            "problem: crates/a/src/lib.rs: run declares a module inside a function or a macro, which this check cannot follow",
        ]
    );
}

#[test]
fn an_import_inside_a_macro_that_reaches_the_desktop_is_refused() {
    let files = [(
        "src/lib.rs",
        "macro_rules! bring { () => { use atlas_core::desktop::Kind as K; }; }",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "problem: crates/a/src/lib.rs: a macro imports a kind or the desktop's names, which this check cannot follow"
        ]
    );
}

#[test]
fn a_chained_or_raw_import_is_judged_like_any_other() {
    for (source, why) in [
        (
            "use Kind::User as Login;",
            "renames the desktop's names with `as`",
        ),
        (
            "use Kind::*;",
            "imports the desktop module's items with a glob",
        ),
        (
            "use crate::r#user::copy;",
            "imports a kind's module from outside it",
        ),
    ] {
        assert_eq!(refused_import(&import(source)), Some(why), "{source}");
    }
    assert!(imports_the_user(&import(
        "use crate::config::helper_uid as uid;"
    )));
}

#[test]
fn a_metavariable_path_to_user_names_the_user_desktop() {
    assert!(names_the_user(&tokens(
        "match kind { $kind::User(u) => u.uid }"
    )));
    assert!(names_the_user(&tokens(
        "impl Kind { fn of(made: Made) -> Self { Self::User(made) } }"
    )));
}

#[test]
fn an_item_inside_an_owner_is_checked_on_its_own() {
    let files = [(
        "src/lib.rs",
        "fn owner(desktop: &Desktop) {\n\
             match desktop.kind() { Kind::User(u) => go(u) }\n\
             fn neighbour(d: &Desktop) -> u32 { d.user().uid }\n\
         }",
    )];
    assert_eq!(
        check(
            &files,
            &[("crates/a/src/lib.rs", &["owner"], "owner: going")]
        ),
        ["unlisted: crates/a/src/lib.rs: neighbour inside owner"]
    );
}

#[test]
fn two_headers_of_one_type_are_told_apart() {
    let files = [(
        "src/lib.rs",
        "impl<T: Into<User>> Wrapper<T> { fn into_user() {} }\n\
         impl<T: AsRef<User>> Wrapper<T> { fn as_user() {} }",
    )];
    assert_eq!(
        check(
            &files,
            &[(
                "crates/a/src/lib.rs",
                &["impl Wrapper<T>"],
                "owner: wrapping"
            )]
        ),
        ["unlisted: crates/a/src/lib.rs: impl Wrapper<T> #2"]
    );
}

#[test]
fn an_inline_module_path_names_the_folder_of_its_modules() {
    let files = [
        (
            "src/lib.rs",
            "#[path = \"elsewhere\"] mod inner { mod deep; }",
        ),
        ("src/elsewhere/deep.rs", "fn made() -> User { todo!() }"),
    ];
    assert_eq!(
        check(&files, &[]),
        ["unlisted: crates/a/src/elsewhere/deep.rs: made"]
    );
}

#[test]
fn a_module_with_two_sources_is_refused() {
    let files = [
        ("src/lib.rs", "mod twice;"),
        ("src/twice.rs", ""),
        ("src/twice/mod.rs", ""),
    ];
    assert_eq!(
        check(&files, &[]),
        [
            "problem: module `twice` has two sources, crates/a/src/twice.rs and crates/a/src/twice/mod.rs",
            "problem: crates/a/src/twice/mod.rs is compiled by no target this check can follow",
            "problem: crates/a/src/twice.rs is compiled by no target this check can follow",
        ]
    );
}

#[test]
fn a_relative_import_of_the_kind_is_judged_like_any_other() {
    for source in [
        "use self::Kind::User as Login;",
        "use super::Kind::*;",
        "use crate::Desktop as Place;",
    ] {
        assert!(refused_import(&import(source)).is_some(), "{source}");
    }
    let files = [(
        "src/lib.rs",
        "use crate::desktop::Kind;\n\
         use self::Kind::User as Login;\n\
         fn which(kind: &Kind) -> u32 { match kind { Login(account) => account.uid } }",
    )];
    assert_eq!(
        check(
            &files,
            &[(
                "crates/a/src/lib.rs",
                &["use self::Kind::User"],
                "pending: importing it"
            )]
        ),
        [
            "refused: crates/a/src/lib.rs: `useself::Kind::UserasLogin;` renames the desktop's names with `as`"
        ]
    );
}

#[test]
fn an_import_in_a_macro_inside_a_function_is_refused() {
    let files = [(
        "src/lib.rs",
        "fn run() { cfg_if! { if #[cfg(unix)] { use crate::{user as desktop_user}; } } desktop_user::copy(); }",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "problem: crates/a/src/lib.rs: a macro imports a kind or the desktop's names, which this check cannot follow"
        ]
    );
}

#[test]
fn items_inside_bodies_are_named_by_all_that_encloses_them() {
    let files = [(
        "src/lib.rs",
        "struct A; struct B;\n\
         impl A { fn report(d: &Desktop) { fn uid(d: &Desktop) -> u32 { d.user().uid } } }\n\
         impl B { fn report(d: &Desktop) { fn uid(d: &Desktop) -> u32 { d.user().uid } } }\n\
         fn outer() { struct Local; impl Local { fn reach(d: &Desktop) -> u32 { d.user().uid } } }\n\
         const READY: u32 = { fn early(d: &Desktop) -> u32 { d.user().uid } 0 };",
    )];
    let allowed: &[(&str, &[&str], &str)] = &[(
        "crates/a/src/lib.rs",
        &[
            "A::report",
            "B::report",
            "READY",
            "outer",
            "uid inside A::report",
        ],
        "owner: reporting",
    )];
    assert_eq!(
        check(&files, allowed),
        [
            "unlisted: crates/a/src/lib.rs: Local::reach inside outer",
            "unlisted: crates/a/src/lib.rs: early inside READY",
            "unlisted: crates/a/src/lib.rs: uid inside B::report",
        ]
    );
}

#[test]
fn a_test_helper_inside_production_code_counts_for_nothing() {
    let files = [
        (
            "src/lib.rs",
            "fn ordinary() -> u32 {\n\
                 #[cfg(test)] fn fixture() -> User { todo!() }\n\
                 #[cfg(test)] mod checks { }\n\
                 1\n\
             }\n\
             mod fixtures;",
        ),
        (
            "src/fixtures.rs",
            "#![cfg(test)]\nfn made() -> User { todo!() }\nmod deeper;",
        ),
        ("src/fixtures/deeper.rs", "fn also() -> User { todo!() }"),
    ];
    assert_eq!(check(&files, &[]), Vec::<String>::new());
}

#[test]
fn an_import_through_the_kind_or_the_desktop_module_names_the_user_desktop() {
    for source in [
        "use Kind::User;",
        "use desktop::User;",
        "use self::Kind::{User};",
    ] {
        assert!(imports_the_user(&import(source)), "{source}");
    }
    let files = [(
        "src/lib.rs",
        "use atlas_core::desktop::Kind;\nuse Kind::User;\nfn shared() {}",
    )];
    assert_eq!(
        check(&files, &[]),
        ["unlisted: crates/a/src/lib.rs: use Kind::User"]
    );
}

#[test]
fn an_alias_of_the_desktop_names_is_refused_and_of_another_kind_is_not() {
    let files = [(
        "src/lib.rs",
        "use atlas_core::desktop::{Desktop, Kind};\n\
         type K = Kind;\n\
         type D = Desktop;\n\
         type Stored = crate::host_session::store::Kind;",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "refused: crates/a/src/lib.rs: type `K` hides the desktop's names behind a type alias",
            "refused: crates/a/src/lib.rs: type `D` hides the desktop's names behind a type alias",
        ]
    );
}

#[test]
fn test_only_fields_variants_arms_and_blocks_count_for_nothing() {
    let files = [(
        "src/lib.rs",
        "struct State { kept: u32, #[cfg(test)] helper_uid: u32 }\n\
         enum Seen { Plain, #[cfg(test)] Made(User) }\n\
         fn ordinary(d: &Desktop, seen: Seen) -> u32 {\n\
             #[cfg(test)] { d.user(); }\n\
             let state = State { kept: 1, #[cfg(test)] helper_uid: 2 };\n\
             match seen { Seen::Plain => state.kept, #[cfg(test)] Seen::Made(u) => u.uid }\n\
         }",
    )];
    assert_eq!(check(&files, &[]), Vec::<String>::new());
}

#[test]
fn another_crates_user_imported_inside_a_body_names_nothing() {
    let files = [(
        "src/lib.rs",
        "fn ordinary(uid: u32) -> bool {\n\
             use nix::unistd::{User as UnixUser};\n\
             UnixUser::from_uid(uid.into()).is_ok()\n\
         }",
    )];
    assert_eq!(check(&files, &[]), Vec::<String>::new());
}

#[test]
fn each_import_is_listed_by_what_it_brings_in() {
    let files = [(
        "src/lib.rs",
        "use atlas_core::desktop::User;\n\
         use crate::config::helper_uid;\n\
         fn shared() { use crate::config::helper_uid as uid; consume(uid()); }",
    )];
    let allowed: &[(&str, &[&str], &str)] = &[(
        "crates/a/src/lib.rs",
        &["use atlas_core::desktop::User"],
        "pending: importing it",
    )];
    assert_eq!(
        check(&files, allowed),
        [
            "unlisted: crates/a/src/lib.rs: use crate::config::helper_uid",
            "unlisted: crates/a/src/lib.rs: use crate::config::helper_uid inside shared",
        ]
    );
}

#[test]
fn an_associated_type_is_an_alias_like_any_other() {
    let files = [(
        "src/lib.rs",
        "impl Provider for Local {\n\
             type Place = atlas_core::desktop::Desktop;\n\
             type Which = atlas_core::desktop::Kind;\n\
             type Stored = crate::host_session::store::Kind;\n\
         }",
    )];
    assert_eq!(
        check(&files, &[]),
        [
            "refused: crates/a/src/lib.rs: type `Place` hides the desktop's names behind a type alias",
            "refused: crates/a/src/lib.rs: type `Which` hides the desktop's names behind a type alias",
        ]
    );
}

#[test]
fn test_only_parameters_and_elements_count_for_nothing() {
    let files = [(
        "src/lib.rs",
        "fn shared(kept: u32, #[cfg(test)] _account: &User) -> [u32; 1] {\n\
             let pair = (kept, #[cfg(test)] desktop.user().uid);\n\
             consume(kept, #[cfg(test)] helper_uid);\n\
             [kept, #[cfg(test)] desktop.user().uid]\n\
         }",
    )];
    assert_eq!(check(&files, &[]), Vec::<String>::new());
}

#[test]
fn a_macro_inside_a_kind_may_import_its_own_kind() {
    let files = [
        ("src/lib.rs", "mod user;"),
        (
            "src/user.rs",
            "macro_rules! own { () => { use crate::user::User; }; }\nown!();",
        ),
    ];
    assert_eq!(
        check(
            &files,
            &[(
                "crates/a/src/lib.rs",
                &["mod user"],
                "declares: the user module"
            )]
        ),
        Vec::<String>::new()
    );
}

#[test]
fn imports_in_two_trait_implementations_are_told_apart() {
    let files = [(
        "src/lib.rs",
        "impl A for Report { fn read() -> u32 { use crate::config::helper_uid as uid; uid() } }\n\
         impl B for Report { fn read() -> u32 { use crate::config::helper_uid as uid; uid() } }",
    )];
    let allowed: &[(&str, &[&str], &str)] = &[(
        "crates/a/src/lib.rs",
        &["use crate::config::helper_uid inside <Report as A>::read"],
        "pending: reading it",
    )];
    assert_eq!(
        check(&files, allowed),
        ["unlisted: crates/a/src/lib.rs: use crate::config::helper_uid inside <Report as B>::read"]
    );
}
