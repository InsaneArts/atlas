//! The keeper's own program, as it was started: its signing, read once at
//! its start, and the file the kernel mapped its image from. An update of
//! Atlas replaces that program's file, and may move or delete the bundle it
//! was in, while the keeper runs: nothing of its own is read from disk
//! again, and the clients it admits are judged by the requirement it was
//! built with. Replaced, it does nothing on its own: it says so, and waits
//! for the update to hand it over.
use serde_json::{Value, json};
use std::{os::unix::fs::MetadataExt, path::PathBuf};

/// This keeper's program as it started.
#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    /// The code directory hash it is signed with: which build it is.
    code: Option<String>,
    /// Its launch path, and the file its image was mapped from.
    launched: Option<Launched>,
}

/// The path a program was launched by and the file its image was mapped
/// from, by device and inode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Launched {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl Launched {
    pub(crate) fn new(path: PathBuf, device: u64, inode: u64) -> Self {
        Self {
            path,
            device,
            inode,
        }
    }

    /// Whether the launch path now names another file than the image's, or
    /// none. A look that fails otherwise is no replacement.
    pub(crate) fn replaced(&self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(now) => now.dev() != self.device || now.ino() != self.inode,
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        }
    }
}

impl Program {
    /// This program's, read now: at the keeper's start, before an update
    /// may have touched its file.
    pub(crate) fn own() -> Self {
        let code = atlas_native::peer::own_code().map(|code| atlas_core::name::hex(&code));
        let launched = std::env::current_exe().ok().and_then(|path| {
            atlas_native::program::mapped()
                .map(|mapped| Launched::new(path, mapped.device, mapped.inode))
        });
        Self { code, launched }
    }

    /// A signed program started from `path`, as a test stands one in.
    #[cfg(test)]
    pub(crate) fn started_from(path: &std::path::Path) -> Self {
        let started = std::fs::metadata(path).unwrap();
        Self {
            code: Some("00".repeat(20)),
            launched: Some(Launched::new(path.to_owned(), started.dev(), started.ino())),
        }
    }

    /// Whether its signing was read.
    pub(crate) const fn signed(&self) -> bool {
        self.code.is_some()
    }

    /// Whether an update replaced it; none when the kernel did not say which
    /// file it runs from, which is never taken for not replaced.
    pub(crate) fn replaced(&self) -> Option<bool> {
        self.launched.as_ref().map(Launched::replaced)
    }

    /// What its hello says of it.
    pub(crate) fn said(&self) -> Value {
        json!({"code": self.code, "replaced": self.replaced()})
    }
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
