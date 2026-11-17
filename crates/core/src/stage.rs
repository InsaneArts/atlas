//! A file or a folder sent to a desktop whole, as one snapshot: every
//! entry's path, kind, mode, size, SHA-256 and extended attributes, and a
//! digest of them all. Both ends check it the same way, so what one
//! refuses the other refuses too.
use crate::{
    error::{Code, Error, Result},
    xattr::{self, Attribute},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod held;
mod snapshot;

pub use held::{CHUNK, Held, HeldFile};
pub use snapshot::open_file;

/// The most entries one snapshot holds.
pub const MOST_ENTRIES: usize = 200_000;
/// The most bytes of file contents one snapshot holds.
pub const MOST_CONTENTS: u64 = 64 << 30;
/// The longest manifest, as sent.
pub const MOST_MANIFEST: usize = 64 << 20;
/// The most files a snapshot held to be sent from holds open at once.
pub const MOST_HELD: usize = 4096;
/// The deepest an entry lies below the root.
pub const DEEPEST: usize = 64;
/// The longest one name in a path.
pub const LONGEST_NAME: usize = 255;
/// The longest symlink target.
pub const LONGEST_TARGET: usize = 1023;
/// How much of one entry's extended attributes is carried.
pub const ATTRIBUTES: xattr::Bounds = xattr::Bounds {
    count: 128,
    value: 1 << 20,
    total: 4 << 20,
};
/// The attribute macOS keeps itself: setting or removing it is ignored, so
/// it is never carried or compared.
pub const PROVENANCE: &str = "com.apple.provenance";

/// What one snapshot holds, and the digest of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// The staged item's name: one path component.
    pub name: String,
    /// The root first, then every entry below it, parents first, sorted.
    pub entries: Vec<Entry>,
    /// SHA-256 of `name` and `entries`.
    pub digest: String,
}

/// One entry of a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Its names below the root; the root's is empty.
    pub path: Vec<String>,
    pub kind: Kind,
    /// Its permission bits; none for a symlink.
    pub mode: u32,
    /// Its extended attributes, sorted by name, the system's own left out.
    pub attributes: Vec<Attribute>,
}

/// What an entry is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Kind {
    File { size: u64, sha256: String },
    Folder,
    Symlink { target: String },
}

/// The refusal of what a snapshot cannot hold.
pub(crate) fn refused(why: impl std::fmt::Display) -> Error {
    Error::new(Code::BadRequest, format!("This cannot be staged: {why}"))
}

impl Snapshot {
    /// Make a snapshot of `name` and `entries`, with their digest.
    ///
    /// # Errors
    ///
    /// Fails when they cannot be written down.
    pub fn new(name: String, entries: Vec<Entry>) -> Result<Self> {
        let digest = digest_of(&name, &entries)?;
        Ok(Self {
            name,
            entries,
            digest,
        })
    }

    /// The bytes of file contents it holds.
    pub fn size(&self) -> u64 {
        self.files().map(|(_, size, _)| size).sum()
    }

    /// The bytes of attribute values it holds.
    pub fn attribute_bytes(&self) -> u64 {
        self.entries
            .iter()
            .flat_map(|entry| &entry.attributes)
            .map(|attribute| attribute.value.len() as u64)
            .sum()
    }

    /// Every file, in order: its entry, size and SHA-256.
    pub fn files(&self) -> impl Iterator<Item = (&Entry, u64, &str)> {
        self.entries.iter().filter_map(|entry| match &entry.kind {
            Kind::File { size, sha256 } => Some((entry, *size, sha256.as_str())),
            Kind::Folder | Kind::Symlink { .. } => None,
        })
    }

    /// Check that it is one a desktop may write: its digest, its order, its
    /// names, its kinds, its modes and its bounds.
    ///
    /// # Errors
    ///
    /// Refuses the first thing that does not hold.
    pub fn check(&self) -> Result<()> {
        name(&self.name)?;
        let Some(root) = self.entries.first() else {
            return Err(refused("it holds nothing"));
        };
        if !root.path.is_empty() || matches!(root.kind, Kind::Symlink { .. }) {
            return Err(refused("its root is not a file or a folder"));
        }
        if self.entries.len() > MOST_ENTRIES {
            return Err(refused("it holds too many entries"));
        }
        let mut contents: u64 = 0;
        let mut folders = std::collections::HashSet::new();
        for (at, entry) in self.entries.iter().enumerate() {
            if at > 0 {
                let before = &self.entries[at - 1].path;
                if entry.path <= *before {
                    return Err(refused("its entries are out of order"));
                }
                let parent = &entry.path[..entry.path.len().saturating_sub(1)];
                if entry.path.is_empty() || !folders.contains(parent) {
                    return Err(refused("an entry lies outside a folder it holds"));
                }
            }
            if entry.path.len() > DEEPEST {
                return Err(refused("an entry lies too deep"));
            }
            for part in &entry.path {
                name(part)?;
            }
            check_entry(entry)?;
            match &entry.kind {
                Kind::File { size, .. } => {
                    contents = contents
                        .checked_add(*size)
                        .filter(|contents| *contents <= MOST_CONTENTS)
                        .ok_or_else(|| refused("its files are too large"))?;
                }
                Kind::Folder => {
                    folders.insert(entry.path.as_slice());
                }
                Kind::Symlink { .. } => {}
            }
        }
        if digest_of(&self.name, &self.entries)? != self.digest {
            return Err(refused("its digest does not match what it holds"));
        }
        Ok(())
    }
}

/// Check one entry's kind, mode and attributes.
fn check_entry(entry: &Entry) -> Result<()> {
    match &entry.kind {
        Kind::File { sha256, .. } => {
            let hex = sha256.len() == 64
                && sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !hex {
                return Err(refused("a file's digest is not SHA-256"));
            }
        }
        Kind::Folder => {}
        Kind::Symlink { target } => {
            if target.is_empty() || target.len() > LONGEST_TARGET || target.contains('\0') {
                return Err(refused("a link's target cannot be kept"));
            }
            if entry.mode != 0 || !entry.attributes.is_empty() {
                return Err(refused("a link carries a mode or attributes"));
            }
        }
    }
    if entry.mode & !0o777 != 0 {
        return Err(refused("an entry is setuid, setgid or sticky"));
    }
    let mut total = 0_usize;
    for (at, attribute) in entry.attributes.iter().enumerate() {
        let sorted = at == 0 || entry.attributes[at - 1].name < attribute.name;
        let named = !attribute.name.is_empty()
            && attribute.name.len() <= xattr::NAME
            && !attribute.name.contains('\0')
            && attribute.name != PROVENANCE;
        total = total
            .saturating_add(attribute.name.len())
            .saturating_add(attribute.value.len());
        if !sorted || !named {
            return Err(refused(
                "an entry's attributes are misnamed or out of order",
            ));
        }
        if attribute.value.len() > ATTRIBUTES.value
            || total > ATTRIBUTES.total
            || at >= ATTRIBUTES.count
        {
            return Err(refused("an entry's attributes are too large"));
        }
    }
    Ok(())
}

/// Check one name in a path: neither empty, `.` nor `..`, holding no `/`
/// or NUL, and within bounds.
fn name(part: &str) -> Result<()> {
    let plain = !part.is_empty()
        && part != "."
        && part != ".."
        && part.len() <= LONGEST_NAME
        && !part.contains(['/', '\0']);
    if plain {
        Ok(())
    } else {
        Err(refused(format!("the name {part:?} cannot be kept")))
    }
}

/// SHA-256 of `name` and `entries`, as JSON, written straight into the
/// digest and never past the manifest's bound, so nothing that large is
/// ever held to be refused.
fn digest_of(name: &str, entries: &[Entry]) -> Result<String> {
    let mut bounded = Bounded {
        hash: Sha256::new(),
        written: 0,
    };
    serde_json::to_writer(&mut bounded, &(name, entries))
        .map_err(|_| refused("its manifest is too large"))?;
    Ok(format!("{:x}", bounded.hash.finalize()))
}

/// Room in a manifest for its name and entries: the rest is its digest.
const NAMES_AND_ENTRIES: usize = MOST_MANIFEST - 1024;

/// JSON going into a digest, refused once past [`NAMES_AND_ENTRIES`].
struct Bounded {
    hash: Sha256,
    written: usize,
}

impl std::io::Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.written = self.written.saturating_add(bytes.len());
        if self.written > NAMES_AND_ENTRIES {
            return Err(std::io::Error::other("past the manifest's bound"));
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
