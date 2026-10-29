//! Part of a regular file in the helper, for the person's side to collect:
//! checked by its open descriptor (where it really is, its owner, kind,
//! size and that nothing is writing it), sent as base64 `data` events, and
//! never changed.
//!
//! Only files in the helper's home, its temporary folder and `/private/tmp`
//! are read, and none in the folders macOS guards (all of `~/Library`
//! among them) nor in Atlas's own `~/.atlas`: those are refused by path
//! before anything is opened, so reading never makes macOS ask for access
//! in a login nobody watches. No link is followed anywhere in the path.
use super::protocol::{FileIdentity, READ_RANGE_MAX, ReadRequest, failure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata, OpenOptions},
    os::{
        macos::fs::MetadataExt as _,
        unix::fs::{FileExt, MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Bytes per `data` event.
pub const CHUNK: usize = 256 * 1024;
/// A file changed more recently than this is still being written.
const STILL: Duration = Duration::from_secs(2);
/// `st_flags` of a file whose contents are not on this Mac yet
/// (`SF_DATALESS`, `sys/stat.h`): reading it would download it.
const SF_DATALESS: u32 = 0x4000_0000;
/// The folders of the helper's home that macOS guards, or that hold other
/// apps' data: refused by path, in any case, before anything is opened.
const PROTECTED: [&str; 4] = ["Desktop", "Documents", "Downloads", "Library"];
/// Atlas's own folder in the helper's home.
const OWN: &str = ".atlas";
/// The system's shared temporary folder, as it really is.
const SHARED_TEMPORARY: &str = "/private/tmp";

/// Where the helper's files may be read from: its home and its own
/// temporary folder, as they really are.
pub struct Places {
    pub home: PathBuf,
    pub temporary: Option<PathBuf>,
}

impl Places {
    /// The places of the daemon's own account: `HOME` and `TMPDIR`, each as
    /// it really is.
    pub fn here() -> Option<Self> {
        let real =
            |name: &str| std::env::var_os(name).and_then(|path| std::fs::canonicalize(path).ok());
        Some(Self {
            home: real("HOME")?,
            temporary: real("TMPDIR"),
        })
    }

    fn roots(&self) -> Vec<&Path> {
        let mut roots = vec![self.home.as_path(), Path::new(SHARED_TEMPORARY)];
        roots.extend(self.temporary.as_deref());
        roots
    }
}

/// Serve `request` for the account `uid`: its reply, after the part's
/// bytes went to `events` one `data` event at a time. `events` says
/// whether the client still takes them; once it does not, nothing more is
/// read.
pub fn read(
    request: &ReadRequest,
    places: &Places,
    uid: u32,
    events: &mut dyn FnMut(Value) -> bool,
) -> Value {
    match served(request, places, uid, events) {
        Ok(reply) => reply,
        Err(reason) => refusal(reason),
    }
}

fn served(
    request: &ReadRequest,
    places: &Places,
    uid: u32,
    events: &mut dyn FnMut(Value) -> bool,
) -> Result<Value, &'static str> {
    let path = locate(&request.path, &places.home)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|error| opened(&error))?;
    let mut real = PathBuf::new();
    nix::fcntl::fcntl(&file, nix::fcntl::FcntlArg::F_GETPATH(&mut real))
        .map_err(|_| "unreadable")?;
    if within(&real, &places.home).is_some_and(guarded) {
        return Err("protected");
    }
    let metadata = file.metadata().map_err(|_| "unreadable")?;
    let identity = readable(
        &metadata,
        &real,
        &places.roots(),
        uid,
        request.expect.as_ref(),
        request.max_size,
        SystemTime::now(),
    )?;

    let mut reply = json!({
        "ok": true,
        "op": "read",
        "file": {
            "path": real,
            "inHome": within(&real, &places.home),
            "dev": identity.dev,
            "ino": identity.ino,
            "size": identity.size,
            "mtime": identity.mtime,
            "ctime": identity.ctime,
        },
        "offset": request.offset,
    });

    if request.digest {
        reply["sha256"] = json!(digest(&file, identity.size)?);
        reply["length"] = json!(0);
    } else {
        let length = request
            .length
            .min(READ_RANGE_MAX)
            .min(identity.size.saturating_sub(request.offset));
        send(&file, request.offset, length, events)?;
        reply["length"] = json!(length);
    }

    // Unchanged to the nanosecond while it was read, or it was being written.
    let after = file.metadata().map_err(|_| "unreadable")?;
    if identity_of(&after) != identity {
        return Err("changing");
    }
    Ok(reply)
}

/// Where `path` points inside the helper, or why it may not be read: the
/// two fixed system links (`/tmp`, `/var/folders`) are read as their
/// `/private` paths, a relative path (or one starting `~/`) starts at
/// `home`, and `.` and `..` components, the folders macOS guards and
/// Atlas's own are refused. Decided from the text alone.
fn locate(path: &str, home: &Path) -> Result<PathBuf, &'static str> {
    let rewritten = if let Some(rest) = path.strip_prefix("/tmp/") {
        format!("/private/tmp/{rest}")
    } else if let Some(rest) = path.strip_prefix("/var/folders/") {
        format!("/private/var/folders/{rest}")
    } else {
        path.to_owned()
    };
    let given = Path::new(&rewritten);
    if given
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err("outside");
    }
    let located = if given.is_absolute() {
        given.to_owned()
    } else if let Ok(rest) = given.strip_prefix("~") {
        home.join(rest)
    } else {
        home.join(given)
    };

    if let Some(inside) = within(&located, home) {
        if guarded(inside) {
            return Err("protected");
        }
        if inside
            .components()
            .next()
            .is_some_and(|first| first.as_os_str().eq_ignore_ascii_case(OWN))
        {
            return Err("outside");
        }
        return Ok(located);
    }
    let elsewhere = [SHARED_TEMPORARY, "/private/var/folders"]
        .iter()
        .any(|root| located.starts_with(root) && located != Path::new(root));
    if elsewhere {
        Ok(located)
    } else {
        Err("outside")
    }
}

/// `path` relative to `home`, when it lies inside it.
fn within<'a>(path: &'a Path, home: &Path) -> Option<&'a Path> {
    path.strip_prefix(home).ok()
}

/// Whether a path inside the helper's home lies in a folder macOS guards,
/// its name compared in any case, as the file system compares it.
fn guarded(inside: &Path) -> bool {
    inside.components().next().is_some_and(|first| {
        PROTECTED
            .iter()
            .any(|folder| first.as_os_str().eq_ignore_ascii_case(folder))
    })
}

/// Why an open failed, as a reason.
fn opened(error: &std::io::Error) -> &'static str {
    match error.raw_os_error() {
        Some(libc::ELOOP) => "symlink",
        Some(libc::ENOENT | libc::ENOTDIR) => "missing",
        _ => "unreadable",
    }
}

/// Whether a file opened at `real`, as `metadata` says it is, may be read
/// now, and what it is: a regular file whose contents are here, of the
/// account `uid`, in one of `roots`, no larger than `max_size`, and either
/// what an earlier part found (`expect`) or untouched for two seconds.
fn readable(
    metadata: &Metadata,
    real: &Path,
    roots: &[&Path],
    uid: u32,
    expect: Option<&FileIdentity>,
    max_size: u64,
    now: SystemTime,
) -> Result<FileIdentity, &'static str> {
    if !roots
        .iter()
        .any(|root| real.starts_with(root) && real != *root)
    {
        return Err("outside");
    }
    if !metadata.is_file() {
        return Err("not-regular");
    }
    if metadata.st_flags() & SF_DATALESS != 0 {
        return Err("dataless");
    }
    if metadata.uid() != uid {
        return Err("owner");
    }
    if metadata.len() > max_size {
        return Err("too-large");
    }
    let identity = identity_of(metadata);
    match expect {
        Some(expect) if *expect != identity => Err("changing"),
        Some(_) => Ok(identity),
        None => {
            let now = now.duration_since(UNIX_EPOCH).unwrap_or_default();
            let still = |[seconds, nanos]: [i64; 2]| {
                let at = Duration::new(seconds.max(0) as u64, nanos.clamp(0, 999_999_999) as u32);
                at + STILL <= now
            };
            if still(identity.mtime) && still(identity.ctime) {
                Ok(identity)
            } else {
                Err("changing")
            }
        }
    }
}

/// What `metadata` says the file is.
fn identity_of(metadata: &Metadata) -> FileIdentity {
    FileIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        size: metadata.len(),
        mtime: [metadata.mtime(), metadata.mtime_nsec()],
        ctime: [metadata.ctime(), metadata.ctime_nsec()],
    }
}

/// Send `length` bytes of `file` from `offset` as `data` events.
fn send(
    file: &File,
    offset: u64,
    length: u64,
    events: &mut dyn FnMut(Value) -> bool,
) -> Result<(), &'static str> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut sent = 0;
    while sent < length {
        let want = usize::try_from(length - sent).map_or(CHUNK, |left| left.min(CHUNK));
        let mut filled = 0;
        while filled < want {
            let count = file
                .read_at(&mut buffer[filled..want], offset + sent + filled as u64)
                .map_err(|_| "unreadable")?;
            if count == 0 {
                // Shorter than it said it was: written meanwhile.
                return Err("changing");
            }
            filled += count;
        }
        if !events(json!({"event": "data", "data": STANDARD.encode(&buffer[..filled])})) {
            return Err("gone");
        }
        sent += filled as u64;
    }
    Ok(())
}

/// SHA-256 of the first `size` bytes of `file`, which must be all of it.
fn digest(file: &File, size: u64) -> Result<String, &'static str> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut read = 0;
    while read < size {
        let count = file.read_at(&mut buffer, read).map_err(|_| "unreadable")?;
        if count == 0 {
            return Err("changing");
        }
        let count = usize::try_from(size - read).map_or(count, |left| count.min(left));
        hasher.update(&buffer[..count]);
        read += count as u64;
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The refusal for `reason`: nothing in the helper changed.
fn refusal(reason: &'static str) -> Value {
    let error = match reason {
        "outside" => "Only files in the helper's home, its temporary folder or /tmp are collected",
        "protected" => {
            "Files in a folder macOS guards, such as Documents or Library, are never collected"
        }
        "symlink" => "A link anywhere in the path is never followed",
        "not-regular" => "Only regular files are collected, not folders, devices or pipes",
        "dataless" => "The file's contents are not on this Mac yet",
        "owner" => "The file does not belong to the helper",
        "too-large" => "The file is larger than allowed",
        "changing" => "The file changed in the last 2 seconds, or while it was read",
        "missing" => "No such file in the helper",
        "gone" => "The client went away",
        _ => "The file could not be read",
    };
    let mut reply = failure("collect", error);
    reply["reason"] = json!(reason);
    reply
}

#[cfg(test)]
mod tests;
