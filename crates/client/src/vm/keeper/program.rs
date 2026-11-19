//! The keeper's own program, as it was started: its signing and the digest
//! of its executable, read once at its start, and the file the kernel mapped
//! its image from. An update of Atlas replaces that program's file, and may
//! move or delete the bundle it was in, while the keeper runs: nothing of
//! its own is read from disk again, and the clients it admits are judged by
//! the requirement it was built with. Replaced, it does nothing on its own:
//! it says so, and waits for the update to hand it over.
//!
//! Its own bundle is read again for one thing alone: to stage it into the
//! VM's guest, and only while it is still the very image the keeper runs
//! ([`Program::bundle`]).
use atlas_core::stage::{Held, Kind, Snapshot};
use serde_json::{Value, json};
use std::{os::unix::fs::MetadataExt, path::PathBuf};

/// Why a program's own bundle is not staged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unheld {
    /// An update replaced it, or which file it runs from cannot be told.
    Replaced,
    /// The bundle is not the very image it runs.
    NotOwn,
}

/// This keeper's program as it started.
#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    /// The code directory hash it is signed with: which build it is.
    code: Option<String>,
    /// The SHA-256 of the file its image was mapped from, as a guest's
    /// daemon says its own `build`.
    build: Option<String>,
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

    /// The SHA-256 of the file its image was mapped from, read through the
    /// launch path's file only while that is still the image's, checked on
    /// the descriptor read; none otherwise.
    fn digest(&self) -> Option<String> {
        let file = atlas_core::files::open_plain(&std::fs::canonicalize(&self.path).ok()?)?;
        let opened = file.metadata().ok()?;
        if opened.dev() != self.device || opened.ino() != self.inode {
            return None;
        }
        atlas_core::digest::sha256(&file)
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
        let build = launched.as_ref().and_then(Launched::digest);
        Self {
            code,
            build,
            launched,
        }
    }

    /// A signed program started from `path`, as a test stands one in.
    #[cfg(test)]
    pub(crate) fn started_from(path: &std::path::Path) -> Self {
        let started = std::fs::metadata(path).unwrap();
        Self {
            code: Some("00".repeat(20)),
            build: None,
            launched: Some(Launched::new(path.to_owned(), started.dev(), started.ino())),
        }
    }

    /// The program at `executable`, signed as it is on disk, its build that
    /// file's digest, as a keeper started from it reads itself.
    #[cfg(test)]
    pub(crate) fn signed_at(executable: &std::path::Path) -> Self {
        let started = std::fs::metadata(executable).unwrap();
        Self {
            code: atlas_native::peer::file_cdhash(executable)
                .map(|code| atlas_core::name::hex(&code)),
            build: atlas_core::digest::sha256(&std::fs::File::open(executable).unwrap()),
            launched: Some(Launched::new(
                executable.to_owned(),
                started.dev(),
                started.ino(),
            )),
        }
    }

    /// A program of the build `build`, as a test stands one in.
    #[cfg(test)]
    pub(crate) fn built(build: &str) -> Self {
        Self {
            build: Some(build.to_owned()),
            ..Self::default()
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

    /// Which build of Atlas it is: its digest, when it was read, and this
    /// build's version.
    pub(crate) fn build(&self) -> crate::vm::update::Build {
        crate::vm::update::Build::new(self.build.as_deref(), Some(atlas_core::VERSION))
    }

    /// The bundle it was launched from, held by descriptor to be staged,
    /// while it is the very image this program runs: its launch path still
    /// names the file the image was mapped from, that file is the bundle's
    /// executable as held, its contents the build read at the start, and
    /// its signature the code this program runs. Its path, and the bundle
    /// held. It waits on the disk: it runs off the runtime.
    ///
    /// # Errors
    ///
    /// [`Unheld::Replaced`] once an update replaced the program, or when
    /// which file it runs from cannot be told; [`Unheld::NotOwn`] for
    /// anything else that is not its own image.
    pub(crate) fn bundle(&self) -> Result<(PathBuf, Held), Unheld> {
        let launched = self.launched.as_ref().ok_or(Unheld::Replaced)?;
        if launched.replaced() {
            return Err(Unheld::Replaced);
        }
        let (Some(code), Some(build)) = (&self.code, &self.build) else {
            return Err(Unheld::NotOwn);
        };
        let executable = std::fs::canonicalize(&launched.path).map_err(|_| Unheld::NotOwn)?;
        let bundle = atlas_core::app_bundle::app_of(&executable).ok_or(Unheld::NotOwn)?;
        let inside: Vec<String> = executable
            .strip_prefix(&bundle)
            .map_err(|_| Unheld::NotOwn)?
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect();
        let held = Snapshot::held(&bundle).map_err(|_| Unheld::NotOwn)?;
        let own = held.files().any(|(entry, _, file)| {
            entry.path == inside
                && file.is(launched.device, launched.inode)
                && matches!(&entry.kind, Kind::File { sha256, .. } if sha256 == build)
        });
        let signed = atlas_native::peer::file_cdhash(&executable)
            .is_some_and(|signed| atlas_core::name::hex(&signed) == *code);
        // Every path checked still names what is held, the executable whose
        // signature was read through its path included.
        if !own || !signed || held.still(&bundle).is_err() {
            return Err(Unheld::NotOwn);
        }
        Ok((bundle, held))
    }

    /// What its hello says of it.
    pub(crate) fn said(&self) -> Value {
        json!({"code": self.code, "build": self.build, "replaced": self.replaced()})
    }
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
