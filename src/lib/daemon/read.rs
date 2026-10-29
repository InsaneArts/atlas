//! Part of a regular file in the helper, for the person's side to collect:
//! checked by its open descriptor (where it really is, its owner, kind,
//! size and that nothing is writing it), sent as base64 `data` events, and
//! never changed.
//!
//! Only files in the helper's home, its own temporary folder and
//! `/private/tmp` are read, and none in the folders macOS guards (all of
//! `~/Library` among them), in the Trash nor in Atlas's own `~/.atlas`:
//! those are refused before anything is opened, by name and by the folder
//! the path's first part is, so reading never makes macOS ask for access in
//! a login nobody watches. No link is followed anywhere in the path. The
//! session is checked again before the file is opened and between parts,
//! so nothing is read once the helper has come on screen.
use super::protocol::{FileIdentity, READ_RANGE_MAX, ReadRequest, failure};
use crate::collect::Refused;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
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
/// apps' data, media libraries or deleted files: refused before anything
/// is opened.
const PROTECTED: [&str; 8] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Library",
    "Pictures",
    "Music",
    "Movies",
    ".Trash",
];
/// Atlas's own folder in the helper's home.
const OWN: &str = ".atlas";
/// The system's shared temporary folder, as it really is.
const SHARED_TEMPORARY: &str = "/private/tmp";
/// Where every account's own temporary folder is, as it really is.
const TEMPORARY_FOLDERS: &str = "/private/var/folders";

/// Where the helper's files may be read from: its home and its own
/// temporary folder, as they really are.
pub struct Places {
    pub home: PathBuf,
    /// The account's own temporary folder; none when it is not known, and
    /// then no account's is read.
    pub temporary: Option<PathBuf>,
}

impl Places {
    /// The places of the daemon's own account: `HOME`, and the temporary
    /// folder macOS keeps for it, each as it really is.
    pub fn here() -> Option<Self> {
        let home = std::env::var_os("HOME").and_then(|path| std::fs::canonicalize(path).ok())?;
        Some(Self {
            home,
            temporary: own_temporary().and_then(|path| std::fs::canonicalize(path).ok()),
        })
    }

    fn roots(&self) -> Vec<&Path> {
        let mut roots = vec![self.home.as_path(), Path::new(SHARED_TEMPORARY)];
        roots.extend(self.temporary.as_deref());
        roots
    }
}

/// This account's own temporary folder, as macOS names it
/// (`_CS_DARWIN_USER_TEMP_DIR`), whatever `TMPDIR` says.
#[allow(
    unsafe_code,
    reason = "FFI: confstr(_CS_DARWIN_USER_TEMP_DIR) has no safe binding"
)]
fn own_temporary() -> Option<PathBuf> {
    // SAFETY: a null buffer of length 0 asks only for the size the value
    // needs, its NUL included; nothing is written.
    let size = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if size == 0 || size > 4096 {
        return None;
    }
    let mut buffer = vec![0_u8; size];
    // SAFETY: the buffer is writable for `size` bytes, the length passed in;
    // confstr writes at most that many, its NUL included, and returns the
    // length the whole value needs, checked below.
    let needed = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if needed == 0 || needed > buffer.len() {
        return None;
    }
    let text = std::ffi::CStr::from_bytes_until_nul(&buffer).ok()?;
    let path = PathBuf::from(<OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(
        text.to_bytes(),
    ));
    path.is_absolute().then_some(path)
}

/// Serve `request` for the account `uid`: its reply, after the part's
/// bytes went to `events` one `data` event at a time. `events` says
/// whether the client still takes them; once it does not, nothing more is
/// read. `guard` says whether the helper may still be read, as before the
/// request was taken: it is asked before the file is opened, before each
/// part and before the digest, and its refusal is the reply.
pub fn read(
    request: &ReadRequest,
    places: &Places,
    uid: u32,
    guard: &dyn Fn() -> Result<(), Value>,
    events: &mut dyn FnMut(Value) -> bool,
) -> Value {
    match served(request, places, uid, guard, events) {
        Ok(reply) => reply,
        Err(Stop::Refused(reason)) => refusal(reason),
        Err(Stop::Guard(refusal)) => refusal,
    }
}

/// Why a read stopped: a reason of its own, or the guard's refusal.
enum Stop {
    Refused(Refused),
    Guard(Value),
}

impl From<Refused> for Stop {
    fn from(reason: Refused) -> Self {
        Self::Refused(reason)
    }
}

fn served(
    request: &ReadRequest,
    places: &Places,
    uid: u32,
    guard: &dyn Fn() -> Result<(), Value>,
    events: &mut dyn FnMut(Value) -> bool,
) -> Result<Value, Stop> {
    let path = locate(&request.path, places)?;
    guard().map_err(Stop::Guard)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|error| opened(&error))?;
    let mut real = PathBuf::new();
    nix::fcntl::fcntl(&file, nix::fcntl::FcntlArg::F_GETPATH(&mut real))
        .map_err(|_| Refused::Unreadable)?;
    if let Some(inside) = within(&real, &places.home) {
        refuse_inside(inside, &places.home)?;
    }
    let metadata = file.metadata().map_err(|_| Refused::Unreadable)?;
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
        guard().map_err(Stop::Guard)?;
        reply["sha256"] = json!(digest(&file, identity.size, guard)?);
        reply["length"] = json!(0);
    } else {
        let length = request
            .length
            .min(READ_RANGE_MAX)
            .min(identity.size.saturating_sub(request.offset));
        send(&file, request.offset, length, guard, events)?;
        reply["length"] = json!(length);
    }

    // Unchanged to the nanosecond while it was read, or it was being written.
    let after = file.metadata().map_err(|_| Refused::Unreadable)?;
    if identity_of(&after) != identity {
        return Err(Refused::Changing.into());
    }
    guard().map_err(Stop::Guard)?;
    Ok(reply)
}

/// Where `path` points inside the helper, or why it may not be read: the
/// two fixed system links (`/tmp`, `/var/folders`) are read as their
/// `/private` paths, a relative path (or one starting `~/`) starts at the
/// helper's home, and `.` and `..` components, the folders macOS guards,
/// Atlas's own and every temporary folder but the helper's own are
/// refused. Nothing is opened: only the first part of a path in the home
/// is looked at, without following it.
fn locate(path: &str, places: &Places) -> Result<PathBuf, Refused> {
    let home = &places.home;
    let rewritten = if let Some(rest) = path.strip_prefix("/tmp/") {
        format!("{SHARED_TEMPORARY}/{rest}")
    } else if let Some(rest) = path.strip_prefix("/var/folders/") {
        format!("{TEMPORARY_FOLDERS}/{rest}")
    } else {
        path.to_owned()
    };
    let given = Path::new(&rewritten);
    if given
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(Refused::Outside);
    }
    let located = if given.is_absolute() {
        given.to_owned()
    } else if let Ok(rest) = given.strip_prefix("~") {
        home.join(rest)
    } else {
        home.join(given)
    };

    if let Some(inside) = within(&located, home) {
        refuse_inside(inside, home)?;
        return Ok(located);
    }
    let shared = located.starts_with(SHARED_TEMPORARY) && located != Path::new(SHARED_TEMPORARY);
    let own = places
        .temporary
        .as_deref()
        .is_some_and(|temporary| located.starts_with(temporary) && located != temporary);
    if shared || own {
        Ok(located)
    } else {
        Err(Refused::Outside)
    }
}

/// Refuse a path inside the helper's `home` (`inside`) whose first part is
/// a folder macOS guards or Atlas's own: by its name in any ASCII case, and
/// by what the first part is on the disk, so a spelling the file system
/// takes for one of them (another Unicode case, say) is refused too.
fn refuse_inside(inside: &Path, home: &Path) -> Result<(), Refused> {
    let Some(first) = inside.components().next().map(Component::as_os_str) else {
        return Err(Refused::Outside);
    };
    let named = |name: &str| first.eq_ignore_ascii_case(name);
    if PROTECTED.iter().any(|folder| named(folder)) {
        return Err(Refused::Protected);
    }
    if named(OWN) {
        return Err(Refused::Outside);
    }
    // A spelling APFS takes for one of them in another Unicode case.
    if !first.is_ascii() && differs_only_in_unicode_case(first) {
        return Err(Refused::Protected);
    }

    let Some(found) = node(&home.join(first)) else {
        return Ok(());
    };
    let is = |name: &str| node(&home.join(name)) == Some(found);
    if PROTECTED.iter().any(|folder| is(folder)) {
        return Err(Refused::Protected);
    }
    if is(OWN) {
        return Err(Refused::Outside);
    }
    Ok(())
}

/// Whether `name` folds, by Unicode's lowercase, to the name of a folder
/// macOS guards or Atlas's own.
fn differs_only_in_unicode_case(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let folded = fold(name);
    PROTECTED
        .iter()
        .chain(std::iter::once(&OWN))
        .any(|folder| fold(folder) == folded)
}

/// `name` in a form two spellings that APFS takes for one share: lowercase,
/// with the long s and the Kelvin sign read as the letters they fold to.
fn fold(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            '\u{017F}' => 's',
            '\u{212A}' => 'k',
            '\u{212B}' => 'å',
            other => other,
        })
        .collect::<String>()
        .to_lowercase()
}

/// The device and node of `path` itself, not following a link.
fn node(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

/// `path` relative to `home`, when it lies inside it.
fn within<'a>(path: &'a Path, home: &Path) -> Option<&'a Path> {
    path.strip_prefix(home).ok()
}

/// Why an open failed, as a reason.
fn opened(error: &std::io::Error) -> Refused {
    match error.raw_os_error() {
        Some(libc::ELOOP) => Refused::Symlink,
        Some(libc::ENOENT | libc::ENOTDIR) => Refused::Missing,
        _ => Refused::Unreadable,
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
) -> Result<FileIdentity, Refused> {
    if !roots
        .iter()
        .any(|root| real.starts_with(root) && real != *root)
    {
        return Err(Refused::Outside);
    }
    if !metadata.is_file() {
        return Err(Refused::NotRegular);
    }
    if metadata.st_flags() & SF_DATALESS != 0 {
        return Err(Refused::Dataless);
    }
    if metadata.uid() != uid {
        return Err(Refused::Owner);
    }
    if metadata.len() > max_size {
        return Err(Refused::TooLarge);
    }
    let identity = identity_of(metadata);
    match expect {
        Some(expect) if *expect != identity => Err(Refused::Changing),
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
                Err(Refused::Changing)
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

/// Send `length` bytes of `file` from `offset` as `data` events, asking
/// `guard` before each one.
fn send(
    file: &File,
    offset: u64,
    length: u64,
    guard: &dyn Fn() -> Result<(), Value>,
    events: &mut dyn FnMut(Value) -> bool,
) -> Result<(), Stop> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut sent = 0;
    while sent < length {
        guard().map_err(Stop::Guard)?;
        let want = usize::try_from(length - sent).map_or(CHUNK, |left| left.min(CHUNK));
        let mut filled = 0;
        while filled < want {
            let count = file
                .read_at(&mut buffer[filled..want], offset + sent + filled as u64)
                .map_err(|_| Refused::Unreadable)?;
            if count == 0 {
                // Shorter than it said it was: written meanwhile.
                return Err(Refused::Changing.into());
            }
            filled += count;
        }
        if !events(json!({"event": "data", "data": STANDARD.encode(&buffer[..filled])})) {
            return Err(Refused::Gone.into());
        }
        sent += filled as u64;
    }
    Ok(())
}

/// SHA-256 of the first `size` bytes of `file`, which must be all of it,
/// asking `guard` before each mebibyte, so a helper that came on screen is
/// not read on to the end.
fn digest(file: &File, size: u64, guard: &dyn Fn() -> Result<(), Value>) -> Result<String, Stop> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut read = 0;
    while read < size {
        guard().map_err(Stop::Guard)?;
        let count = file
            .read_at(&mut buffer, read)
            .map_err(|_| Refused::Unreadable)?;
        if count == 0 {
            return Err(Refused::Changing.into());
        }
        let count = usize::try_from(size - read).map_or(count, |left| count.min(left));
        hasher.update(&buffer[..count]);
        read += count as u64;
    }
    Ok(crate::name::hex(&hasher.finalize()))
}

/// The refusal for `reason`: nothing in the helper changed.
fn refusal(reason: Refused) -> Value {
    let mut reply = failure("collect", reason.said());
    reply["reason"] = json!(reason);
    reply
}

#[cfg(test)]
mod tests;
