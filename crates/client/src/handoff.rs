//! Copies of the person's apps and files for the helper to open, when the
//! helper cannot read the originals. Each copy lives in a folder of its own
//! under ROOT, private to this account apart from an inherited read-only
//! grant to the helper, and nothing there is ever replaced: a new copy gets
//! a new folder, so a running app's files never change under it.
use crate::{
    client::{Client, RUN_TIMEOUT, Run},
    host_session::{HostSession, Key},
    task::{Claims, Owned, State},
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    name::same_text,
    transport::run_output,
};
use nix::{
    errno::Errno,
    fcntl::{OFlag, openat},
    sys::stat::Mode,
};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Where the copies live.
pub const ROOT: &str = "/Users/Shared/Atlas/Open";

/// How long a copy is kept once nothing of its name runs in the helper.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Io, message)
}

/// Whether `path` is a real folder (never a link) owned by this account and
/// writable by nobody else.
fn private_folder(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| private_metadata(&metadata))
}

fn private_metadata(metadata: &fs::Metadata) -> bool {
    metadata.is_dir()
        && metadata.uid() == nix::unistd::getuid().as_raw()
        && metadata.mode() & 0o022 == 0
}

/// ROOT, made and checked before every use: its parent and itself must be
/// this account's own folders, and only the helper is let in, read-only.
fn root(helper: u32) -> Result<PathBuf> {
    let root = PathBuf::from(ROOT);
    let parent = root.parent().unwrap_or(Path::new("/"));
    if !private_folder(parent) {
        return Err(problem(format!(
            "{} is not Atlas's own folder. Run atlas init.",
            parent.display()
        )));
    }

    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Error::writing(&root, error)),
    }

    if !private_folder(&root) {
        return Err(problem(format!(
            "{ROOT} is not a private folder of this account; remove it and try again"
        )));
    }

    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;

    let not_found = || Error::new(Code::Identity, "The helper account was not found");
    let account = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(helper))
        .map_err(|cause| not_found().with_source(cause))?
        .ok_or_else(not_found)?;

    // The grant is set once; it is replaced only when it names someone else,
    // so the helper is never shut out of copies it is using.
    if !granted(&root, &account.name) {
        let grant = format!(
            "{} allow list,search,readattr,readextattr,readsecurity,read,execute,file_inherit,directory_inherit",
            account.name
        );
        for args in [vec!["-N"], vec!["+a", grant.as_str()]] {
            quiet("/bin/chmod", &args, &root, atlas_core::bounded::FILE).map_err(|cause| {
                problem(format!("Could not let the helper into {ROOT}")).with_source(cause)
            })?;
        }
    }

    Ok(root)
}

/// Whether `folder`'s only access entry is the helper's inherited read grant.
fn granted(folder: &Path, account: &str) -> bool {
    let Ok(output) = atlas_core::bounded::output(
        Command::new("/bin/ls").arg("-lde").arg(folder),
        atlas_core::bounded::FILE,
    ) else {
        return false;
    };

    let listing = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<&str> = listing
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .collect();

    // Exactly the read-only grant: on a folder, read and execute are listed
    // as list and search. Any other right (writing, deleting) disqualifies it.
    let expected = [
        "directory_inherit",
        "file_inherit",
        "list",
        "readattr",
        "readextattr",
        "readsecurity",
        "search",
    ];

    let Some(rights) = entries
        .first()
        .and_then(|entry| entry.split_once(&format!("user:{account} allow ")))
        .map(|(_, rights)| rights.trim())
    else {
        return false;
    };

    let mut rights: Vec<&str> = rights.split(',').map(str::trim).collect();
    rights.sort_unstable();

    entries.len() == 1 && rights == expected
}

/// Run a system tool on `path`, quietly, for `limit` at most, failing when
/// the tool does or takes longer.
fn quiet(tool: &str, args: &[&str], path: &Path, limit: Duration) -> Result<()> {
    let output = atlas_core::bounded::output(Command::new(tool).args(args).arg(path), limit)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(problem(format!("{tool} failed on {}", path.display())))
    }
}

/// Which of `paths` the helper itself can open: readable, a folder it can
/// enter, and for an app, its executable too. The helper answers, so its
/// groups and access lists count, not just permission bits. It runs with a
/// run's usual limit under the ownership the caller holds.
async fn readable(client: &Client, _ownership: &Ownership, paths: &[PathBuf]) -> Result<Vec<bool>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    const CHECK: &str = r#"for p; do
  ok=1
  test -r "$p" || ok=0
  if [ -d "$p" ]; then
    test -x "$p" || ok=0
    case "$p" in *.app)
      exe=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$p/Contents/Info.plist" 2>/dev/null) || ok=0
      if [ -n "$exe" ]; then test -r "$p/Contents/MacOS/$exe" && test -x "$p/Contents/MacOS/$exe" || ok=0; else ok=0; fi
    esac
  fi
  echo "$ok"
done"#;

    let mut argv = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        CHECK.to_owned(),
        "sh".to_owned(),
    ];
    argv.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));

    // Only reads: a failed check leaves nothing pending.
    let reply = client.run(argv, Run::inspect().within(RUN_TIMEOUT)).await?;
    let output = run_output(&reply, 65536).unwrap_or_default();
    let answers: Vec<bool> = String::from_utf8_lossy(&output)
        .lines()
        .map(|line| line.trim() == "1")
        .collect();
    if reply["exitCode"] != 0 || answers.len() != paths.len() {
        return Err(Error::new(
            Code::Input,
            "The helper could not check what it can read",
        ));
    }

    Ok(answers)
}

/// Something `open` hands the helper: a name or URL, or a path here.
pub(crate) enum Target {
    /// An app name or URL, passed through.
    Name(String),
    /// A file or folder here, made absolute.
    Path(PathBuf),
}

/// `target` as given: a path here when one exists (made absolute, since the
/// helper has its own working directory), otherwise a name or URL.
pub(crate) fn resolve(target: &str) -> Target {
    match fs::canonicalize(target) {
        Ok(path) => Target::Path(path),
        Err(_) => Target::Name(target.to_owned()),
    }
}

/// A running app, as the helper lists it.
pub(crate) struct Running {
    pub(crate) name: String,
    pub(crate) executable: String,
}

impl Running {
    /// The running apps of `apps`, an app list the helper gave; a name or
    /// executable it does not know is empty.
    pub(crate) fn listed(apps: &crate::client::AppList) -> Vec<Self> {
        apps.rows()
            .iter()
            .map(|app| Self {
                name: app.name().unwrap_or("").to_owned(),
                executable: app.executable().unwrap_or("").to_owned(),
            })
            .collect()
    }
}

/// Which app the copies are opened with, by the names it may run as.
const OPENED_WITH: &str = ".opened-with";

/// The host session a copy folder was made for: its key, alone on a line.
const SESSION: &str = ".session";

/// What [`hand_over`] gives the helper to open.
#[derive(Debug)]
pub(crate) struct Handed {
    /// The targets as the helper can open them, in order.
    pub(crate) paths: Vec<String>,
    /// The copies made for it.
    pub(crate) copied: Vec<Copied>,
}

/// One copy made for the helper: what was copied, where to, and the token
/// its folder is named with. Replies name only where from and to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Copied {
    pub(crate) from: PathBuf,
    pub(crate) to: PathBuf,
    #[serde(skip)]
    pub(crate) token: Token,
}

/// The paths among `targets` as the helper can open them: unchanged when it
/// can read them (or they are copies already), otherwise copied first. Every
/// target is checked before anything is copied; when a copy fails, the
/// copies already made for the others go.
pub(crate) async fn hand_over(
    client: &Client,
    ownership: &Ownership,
    targets: Vec<Target>,
    running: &[Running],
) -> Result<Handed> {
    // The app the targets open with: its copies go when it is closed.
    let opened_with: Vec<String> = match targets.first() {
        Some(Target::Name(name)) if targets.len() > 1 => vec![name.clone()],
        Some(Target::Path(path))
            if targets.len() > 1
                && path.extension().is_some_and(|extension| extension == "app") =>
        {
            names(path)
        }
        Some(Target::Name(_) | Target::Path(_)) | None => Vec::new(),
    };

    let root_path = PathBuf::from(ROOT);
    let checks: Vec<PathBuf> = targets
        .iter()
        .filter_map(|target| match target {
            Target::Path(path) if !path.starts_with(&root_path) => Some(path.clone()),
            Target::Name(_) | Target::Path(_) => None,
        })
        .collect();
    let answers = readable(client, ownership, &checks).await?;
    let placed = placement(targets, answers, running)?;

    let root = if placed
        .iter()
        .any(|placed| matches!(placed, Placed::Copy { .. }))
    {
        self::root(client.config.helper_uid)?
    } else {
        root_path
    };
    let claims = Claims::of(client);
    let owner = Owner {
        session: client.host_session.as_ref().map(HostSession::key),
        claims: &claims,
        opened_with: &opened_with,
    };
    handed_in(&root, placed, &owner, copy)
}

/// What one target becomes for the helper.
enum Placed {
    /// Opened as it is: a name, a URL, a path the helper can read or one of
    /// Atlas's copies.
    Given(String),
    /// Copied under the copy root first.
    Copy { path: PathBuf, token: Token },
}

/// [`hand_over`]'s targets in order, given `answers` (whether the helper
/// can read each path outside ROOT): each one as it is given, or the copy
/// it needs. A folder the helper cannot read, or a file whose earlier copy
/// still runs in the helper, refuses them all before anything is copied.
fn placement(targets: Vec<Target>, answers: Vec<bool>, running: &[Running]) -> Result<Vec<Placed>> {
    let root_path = PathBuf::from(ROOT);
    let mut answers = answers.into_iter();
    let mut placed = Vec::new();

    for target in targets {
        let path = match target {
            Target::Name(name) => {
                placed.push(Placed::Given(name));
                continue;
            }
            Target::Path(path) if path.starts_with(&root_path) => path,
            Target::Path(path) if answers.next().unwrap_or(false) => path,
            Target::Path(path) => {
                if path.is_dir() && path.extension().is_none_or(|extension| extension != "app") {
                    // A whole folder could be any size; only apps and files are copied.
                    return Err(Error::new(
                        Code::Input,
                        format!(
                            "The helper cannot read {}. Atlas copies apps and files, not folders; share a project with atlas environment.",
                            path.display()
                        ),
                    ));
                }

                // A new copy is a new app to macOS: a second one would run
                // beside the first instead of bringing it forward.
                if let Some(earlier) = running.iter().find(|app| {
                    app.executable.starts_with(ROOT) && names(&path).contains(&app.name)
                }) {
                    return Err(Error::new(
                        Code::Input,
                        format!(
                            "An earlier copy of {0} is still running in the helper. Close it first: atlas close --app '{0}'",
                            earlier.name
                        ),
                    ));
                }

                placed.push(Placed::Copy {
                    path,
                    token: Token::new(),
                });
                continue;
            }
        };
        placed.push(Placed::Given(path.to_string_lossy().into_owned()));
    }

    Ok(placed)
}

/// Who a copy is made for: the host session its label names, if any, the
/// task whose record claims it, and the app its files are opened with.
struct Owner<'a> {
    session: Option<&'a Key>,
    claims: &'a Claims,
    opened_with: &'a [String],
}

/// What `placed` gives the helper to open, its copies made under `root`
/// with `copy`. Every copy's token is claimed for the command's task in one
/// write before any folder is made, so a crash on the way leaves each one
/// findable, and all are settled in one more once it is known how each
/// went: `made` when it is there, `gone` only once its folder is seen to be
/// absent. When a copy fails, those already made go; one whose folder
/// stays anyway is left claimed and named in the error as `leftover`.
fn handed_in(
    root: &Path,
    placed: Vec<Placed>,
    owner: &Owner,
    mut copy: impl FnMut(&Path, &Path, &Token) -> Result<PathBuf>,
) -> Result<Handed> {
    let tokens: Vec<Token> = placed
        .iter()
        .filter_map(|placed| match placed {
            Placed::Copy { token, .. } => Some(token.clone()),
            Placed::Given(_) => None,
        })
        .collect();
    let owned: Vec<Owned> = tokens
        .iter()
        .map(|id| Owned::Copy { id: id.clone() })
        .collect();
    let _ = owner.claims.claim_all(&owned, State::Intent);

    let mut made = Vec::new();
    let mut paths = Vec::new();
    let result = placed.into_iter().try_for_each(|placed| -> Result<()> {
        match placed {
            Placed::Given(given) => paths.push(given),
            Placed::Copy { path, token } => {
                let copied = copy(&path, root, &token)?;
                let marked = owner.mark(&path, &copied);
                paths.push(copied.to_string_lossy().into_owned());
                made.push(Copied {
                    from: path,
                    to: copied,
                    token,
                });
                marked?;
            }
        }
        Ok(())
    });

    if result.is_err() {
        // Nothing has opened these copies yet: they go with the failure.
        for folder in made.iter().filter_map(|copy| copy.to.parent()) {
            let _ = fs::remove_dir_all(folder);
        }
    }

    let mut settled = Vec::new();
    let mut leftover = Vec::new();
    for token in tokens {
        let folder = root.join(token.as_str());
        let owned = Owned::Copy { id: token };
        match (fs::symlink_metadata(&folder).is_ok(), result.is_err()) {
            (false, _) => settled.push((owned, State::Gone)),
            (true, false) => settled.push((owned, State::Made)),
            (true, true) => leftover.push(folder.to_string_lossy().into_owned()),
        }
    }
    let _ = owner.claims.settle_all(&settled);

    match result {
        Ok(()) => Ok(Handed {
            paths,
            copied: made,
        }),
        Err(error) if leftover.is_empty() => Err(error),
        Err(mut error) => {
            error.error.push_str(&format!(
                " Atlas could not remove what it began copying at {}; it stays claimed for this task.",
                leftover.join(", ")
            ));
            Err(error.with("leftover", serde_json::json!(leftover)))
        }
    }
}

impl Owner<'_> {
    /// Label `copied`, a copy of `path` just made, with the host session it
    /// was made for and the app its files are opened with. A lost session
    /// label leaves the copy everyone's, like a copy made with no session.
    fn mark(&self, path: &Path, copied: &Path) -> Result<()> {
        let Some(folder) = copied.parent() else {
            return Ok(());
        };
        if let Some(session) = self.session {
            let _ = label(folder, session);
        }
        if path.extension().is_none_or(|extension| extension != "app")
            && !self.opened_with.is_empty()
        {
            fs::write(folder.join(OPENED_WITH), self.opened_with.join("\n"))?;
        }
        Ok(())
    }
}

/// Copy `path` into a new folder of its own under `root`, named `token`,
/// read-only for the helper: the copy keeps execute bits but nobody else
/// may write to it.
fn copy(path: &Path, root: &Path, token: &Token) -> Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::new(Code::Input, format!("Cannot open {}", path.display())))?;
    let folder = root.join(token.as_str());
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .map_err(|error| Error::writing(&folder, error))?;

    let copy = folder.join(name);
    let made = quiet(
        "/usr/bin/ditto",
        &["--noacl", &path.to_string_lossy()],
        &copy,
        atlas_core::bounded::COPY,
    )
    .and_then(|()| {
        quiet(
            "/bin/chmod",
            &["-R", "go-w"],
            &copy,
            atlas_core::bounded::COPY,
        )
    });
    if let Err(cause) = made {
        let _ = fs::remove_dir_all(&folder);
        return Err(
            problem(format!("Could not copy {} for the helper", path.display())).with_source(cause),
        );
    }

    Ok(copy)
}

/// Note in `folder` that host session `session` made it, readable only by
/// this account and, through the folder's grant, the helper.
fn label(folder: &Path, session: &Key) -> std::io::Result<()> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(folder.join(SESSION))?
        .write_all(format!("{session}\n").as_bytes())
}

/// Whose a copy folder is, by its [`SESSION`] label.
#[derive(Debug, PartialEq, Eq)]
enum Label {
    /// No label: made before sessions were recorded, or by a command
    /// without one.
    None,
    Session(Key),
    /// A label that is not a plain file of this account holding one key
    /// on one line: taken for another session's.
    Unreadable,
}

/// The label of the copy folder `folder`, read as every removal reads it:
/// relative to the folder held open.
#[cfg(test)]
fn labelled(folder: &Path) -> Label {
    let folder = open_folder(folder).unwrap();
    let metadata = folder.metadata().unwrap();
    OpenCopy { folder, metadata }.label()
}

/// The token of the copy folder `path` lies in, when it lies in one under
/// ROOT: the folder's own name, never the copy's.
pub(crate) fn token_of(path: &str) -> Option<Token> {
    let rest = Path::new(path).strip_prefix(ROOT).ok()?;
    Token::parse(rest.components().next()?.as_os_str().to_str()?)
}

/// The token a copy folder is named with, wherever it is.
fn folder_token(folder: &Path) -> Option<Token> {
    Token::parse(folder.file_name()?.to_str()?)
}

/// Whether the copy folder `token` names is still there, under `root`.
pub(crate) fn copy_exists(root: &Path, token: &Token) -> bool {
    private_folder(&root.join(token.as_str()))
}

/// The app the copy folder `token` names is for: the app its files were
/// opened with, else the app it holds. Never a copied file's name.
pub fn copy_app(root: &Path, token: &Token) -> Option<String> {
    let folder = root.join(token.as_str());
    if !private_folder(&folder) {
        return None;
    }
    let with = fs::read_to_string(folder.join(OPENED_WITH)).unwrap_or_default();
    with.lines()
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            entries(&folder).iter().find_map(|copy| {
                copy.extension()
                    .is_some_and(|extension| extension == "app")
                    .then(|| copy.file_stem()?.to_str().map(str::to_owned))?
            })
        })
        .map(|app| atlas_core::name::bounded(&app))
}

/// Every name a copy folder's app may run as: its copies' names, and the
/// app its files were opened with.
fn folder_names(folder: &Path) -> Vec<String> {
    let with = fs::read_to_string(folder.join(OPENED_WITH)).unwrap_or_default();
    with.lines()
        .map(str::to_owned)
        .chain(entries(folder).iter().flat_map(|copy| names(copy)))
        .collect()
}

/// Whether a copy folder may be in use: an app of one of its names is among
/// `running` (the helper's apps, from a list it gave), or one runs from an
/// executable inside it, whatever it is called. Such a folder is never
/// removed.
fn in_use(folder: &Path, running: &[Running]) -> bool {
    let names = folder_names(folder);
    running.iter().any(|app| {
        runs_from(&app.executable, folder) || names.iter().any(|name| same_text(&app.name, name))
    })
}

/// The copies of `app` under `root`: those named so, or whose bundle calls
/// itself so, or whose files it opened.
fn copies_of(root: &Path, app: &str) -> Vec<PathBuf> {
    folders(root)
        .into_iter()
        .filter(|folder| folder_names(folder).iter().any(|name| same_text(name, app)))
        .collect()
}

/// The copy folders under `root`, when it is Atlas's own private folder.
fn folders(root: &Path) -> Vec<PathBuf> {
    if root_if_owned_in(root).is_none() {
        return Vec::new();
    }

    entries(root)
        .into_iter()
        .filter(|folder| private_folder(folder))
        .collect()
}

/// A folder's copies (its own records, starting with a dot, left out).
fn entries(folder: &Path) -> Vec<PathBuf> {
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default()
}

/// What a copy may be called while it runs: its file name, and for an app
/// the names its bundle gives itself.
fn names(copy: &Path) -> Vec<String> {
    let mut names: Vec<String> = copy
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .into_iter()
        .collect();

    if copy.extension().is_some_and(|extension| extension == "app") {
        names.extend(
            ["CFBundleDisplayName", "CFBundleName", "CFBundleExecutable"]
                .iter()
                .filter_map(|key| bundle_value(copy, key)),
        );
    }

    names
}

/// One string from a bundle's Info.plist.
fn bundle_value(bundle: &Path, key: &str) -> Option<String> {
    let output = atlas_core::bounded::output(
        Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg(format!("Print :{key}"))
            .arg(bundle.join("Contents/Info.plist")),
        atlas_core::bounded::FILE,
    )
    .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// What closing an app did with its copies.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Removed {
    /// The folders removed.
    pub(crate) folders: Vec<String>,
    /// How many copies were kept because another host session made them,
    /// or their label could not be read.
    pub(crate) kept_others: usize,
    /// The copies that could not be removed.
    pub(crate) failed: Vec<Failed>,
}

/// A copy folder that could not be removed, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Failed {
    pub(crate) path: String,
    pub(crate) error: String,
}

impl Removed {
    /// The tokens of the folders removed.
    pub(crate) fn tokens(&self) -> Vec<Token> {
        self.folders
            .iter()
            .filter_map(|folder| folder_token(Path::new(folder)))
            .collect()
    }
}

/// Remove the copies of `app` under `root` (ROOT, but in tests), which has
/// quit, by the rule every removal of a copy follows ([`remove_copy`]):
/// except any in use by an app in `running` (from a list the helper gave
/// once `app` was gone) and, for a command of host `session`, any another
/// session made or whose label cannot be read. Without a session, every
/// copy not in use goes: a session only ever keeps more.
pub(crate) fn remove_copies_in(
    root: &Path,
    app: &str,
    running: &[Running],
    session: Option<&Key>,
) -> Removed {
    let mut removed = Removed::default();
    for folder in copies_of(root, app) {
        let Some(name) = folder.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let path = folder.to_string_lossy().into_owned();
        match take_copy(root, name, session, running) {
            CopyRemoval::Removed => removed.folders.push(path),
            CopyRemoval::AnotherSession | CopyRemoval::Unreadable => removed.kept_others += 1,
            CopyRemoval::Failed(error) => removed.failed.push(Failed { path, error }),
            CopyRemoval::AlreadyGone | CopyRemoval::InUse => {}
        }
    }

    removed
}

/// What removing one copy folder by its token came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CopyRemoval {
    /// Nothing is there under that token.
    AlreadyGone,
    Removed,
    /// Kept: another host session made it.
    AnotherSession,
    /// Kept: what is there is not a private folder of this account, its
    /// label cannot be read, or it was swapped while it was looked at.
    Unreadable,
    /// Kept for now: an app in the list runs from it or by one of its
    /// names.
    InUse,
    /// It could not be removed, and why; trying again may.
    Failed(String),
}

/// A copy folder held open once, with what that one descriptor says of
/// it: whatever is judged of it is judged of the same folder.
struct OpenCopy {
    folder: fs::File,
    metadata: fs::Metadata,
}

impl OpenCopy {
    /// The copy folder called `name` in the copy root held open as
    /// `root`, opened without following a link; none when nothing is
    /// there.
    fn open(root: &fs::File, name: &str) -> std::result::Result<Option<Self>, CopyRemoval> {
        let opened = openat(
            root,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        );
        let folder = match opened {
            Ok(fd) => fs::File::from(fd),
            Err(Errno::ENOENT) => return Ok(None),
            Err(_) => return Err(CopyRemoval::Unreadable),
        };
        let metadata = folder.metadata().map_err(|_| CopyRemoval::Unreadable)?;
        Ok(Some(Self { folder, metadata }))
    }

    /// Whether a command of host session `session` may take this folder
    /// for its own, judged from this one descriptor: a private folder of
    /// this account, and for a command with a session one labelled for it
    /// or not at all. An unlabelled copy is one a command without a
    /// session made, or one whose label was lost: `close` removes it as it
    /// always did, and a cleanup reaches it only when its own task's
    /// record claims its token.
    fn may_take(&self, session: Option<&Key>) -> std::result::Result<(), CopyRemoval> {
        if !private_metadata(&self.metadata) {
            return Err(CopyRemoval::Unreadable);
        }
        let Some(session) = session else {
            return Ok(());
        };
        match self.label() {
            Label::Unreadable => Err(CopyRemoval::Unreadable),
            Label::Session(made) if made != *session => Err(CopyRemoval::AnotherSession),
            Label::Session(_) | Label::None => Ok(()),
        }
    }

    /// The folder's label, read from a file opened relative to it.
    fn label(&self) -> Label {
        let opened = openat(
            &self.folder,
            SESSION,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        );
        let mut file = match opened {
            Ok(fd) => fs::File::from(fd),
            Err(Errno::ENOENT) => return Label::None,
            Err(_) => return Label::Unreadable,
        };
        let plain = file.metadata().is_ok_and(|metadata| {
            metadata.is_file() && metadata.uid() == nix::unistd::getuid().as_raw()
        });
        let mut text = String::new();
        let read = Read::by_ref(&mut file).take(64).read_to_string(&mut text);
        if !plain || read.is_err() {
            return Label::Unreadable;
        }
        let line = text.strip_suffix('\n').unwrap_or(&text);
        Key::parse(line).map_or(Label::Unreadable, Label::Session)
    }

    /// The folder's node, as `DEVICE:INODE`.
    fn node(&self) -> String {
        format!("{}:{}", self.metadata.dev(), self.metadata.ino())
    }

    /// Whether `path` still names the folder this descriptor holds.
    fn still_at(&self, path: &Path) -> bool {
        fs::symlink_metadata(path)
            .is_ok_and(|now| now.dev() == self.metadata.dev() && now.ino() == self.metadata.ino())
    }
}

/// The node (`DEVICE:INODE`) of the copy folder `token` names under
/// `root`, when a cleanup of host session `session`'s work may treat it as
/// its own, judged from one descriptor: what quitting what runs from it is
/// checked against.
pub(crate) fn copy_node(root: &Path, token: &Token, session: &Key) -> Option<String> {
    root_if_owned_in(root)?;
    let held = open_folder(root).ok()?;
    let copy = OpenCopy::open(&held, token.as_str()).ok()??;
    copy.may_take(Some(session)).ok()?;
    Some(copy.node())
}

/// Remove the copy folder `token` names under `root` (ROOT, but in tests),
/// for a cleanup of host session `session`'s work, by the rule every
/// removal of a copy follows: only when it is a private folder of this
/// account, labelled for that session or not at all, all judged from one
/// descriptor, and not in use by an app in `running` (from a list the
/// helper gave once the apps running from it were quit). Copies are
/// Atlas's own duplicates of the person's files, so it is deleted where it
/// is, once its path is seen to name that same folder still; one swapped
/// meanwhile is kept. Only that one folder is looked at.
pub(crate) fn remove_copy(
    root: &Path,
    token: &Token,
    session: &Key,
    running: &[Running],
) -> CopyRemoval {
    take_copy(root, token.as_str(), Some(session), running)
}

/// [`remove_copy`], for a command of host `session` or none.
fn take_copy(root: &Path, name: &str, session: Option<&Key>, running: &[Running]) -> CopyRemoval {
    if root_if_owned_in(root).is_none() {
        return CopyRemoval::Unreadable;
    }
    let Ok(parent) = open_folder(root) else {
        return CopyRemoval::Unreadable;
    };
    let copy = match OpenCopy::open(&parent, name) {
        Ok(Some(copy)) => copy,
        Ok(None) => return CopyRemoval::AlreadyGone,
        Err(kept) => return kept,
    };
    if let Err(kept) = copy.may_take(session) {
        return kept;
    }
    let folder = root.join(name);
    if in_use(&folder, running) {
        return CopyRemoval::InUse;
    }
    if !copy.still_at(&folder) {
        return CopyRemoval::Unreadable;
    }
    match fs::remove_dir_all(&folder) {
        Ok(()) => CopyRemoval::Removed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CopyRemoval::AlreadyGone,
        Err(error) => CopyRemoval::Failed(error.to_string()),
    }
}

/// `path`, a folder, held open without following a link.
fn open_folder(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(path)
}

/// Whether `executable` lies inside `folder`: the one test of where a
/// process runs from that `close` and cleanup share.
pub fn runs_from(executable: &str, folder: &Path) -> bool {
    let path = Path::new(executable);
    !executable.is_empty() && path.starts_with(folder) && path != folder
}

/// Where the copy folder `token` names is, under `root`.
pub fn copy_folder(root: &Path, token: &Token) -> PathBuf {
    root.join(token.as_str())
}

/// Remove copies under `root` (ROOT, but in tests) older than KEEP that are
/// not in use by an app in `running` (the helper's apps), so a copy nobody
/// closed does not stay forever. The tokens of the folders removed come
/// back, so their claims can be settled.
pub(crate) fn prune_in(root: &Path, running: &[Running]) -> Vec<Token> {
    let mut removed = Vec::new();
    for folder in folders(root) {
        let old = fs::symlink_metadata(&folder)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEEP);
        if old
            && !in_use(&folder, running)
            && fs::remove_dir_all(&folder).is_ok()
            && let Some(token) = folder_token(&folder)
        {
            removed.push(token);
        }
    }
    removed
}

/// ROOT itself, for uninstall, when it and its parent are Atlas's own folders.
pub fn root_if_owned() -> Option<PathBuf> {
    root_if_owned_in(Path::new(ROOT))
}

fn root_if_owned_in(root: &Path) -> Option<PathBuf> {
    (root.parent().is_some_and(private_folder) && private_folder(root)).then(|| root.to_path_buf())
}

#[cfg(test)]
mod tests;
