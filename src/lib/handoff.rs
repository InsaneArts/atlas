//! Copies of the person's apps and files for the helper to open, when the
//! helper cannot read the originals. Each copy lives in a folder of its own
//! under ROOT, private to this account apart from an inherited read-only
//! grant to the helper, and nothing there is ever replaced: a new copy gets
//! a new folder, so a running app's files never change under it.
use crate::{
    client::Client,
    control::{Ownership, Token},
    error::{Error, Result},
    host_session::{HostSession, Key},
    task::{Claims, Owned, State},
    text::same_text,
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// Where the copies live.
pub const ROOT: &str = "/Users/Shared/Atlas/Open";

/// How long a copy is kept once nothing of its name runs in the helper.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn problem(message: impl Into<String>) -> Error {
    Error::new("io", message)
}

/// Whether `path` is a real folder (never a link) owned by this account and
/// writable by nobody else.
fn private_folder(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_dir()
            && metadata.uid() == nix::unistd::getuid().as_raw()
            && metadata.mode() & 0o022 == 0
    })
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

    let account = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(helper))
        .ok()
        .flatten()
        .ok_or_else(|| Error::new("identity", "The helper account was not found"))?;

    // The grant is set once; it is replaced only when it names someone else,
    // so the helper is never shut out of copies it is using.
    if !granted(&root, &account.name) {
        let grant = format!(
            "{} allow list,search,readattr,readextattr,readsecurity,read,execute,file_inherit,directory_inherit",
            account.name
        );
        for args in [vec!["-N"], vec!["+a", grant.as_str()]] {
            quiet("/bin/chmod", &args, &root)
                .map_err(|_| problem(format!("Could not let the helper into {ROOT}")))?;
        }
    }

    Ok(root)
}

/// Whether `folder`'s only access entry is the helper's inherited read grant.
fn granted(folder: &Path, account: &str) -> bool {
    let Ok(output) = Command::new("/bin/ls")
        .arg("-lde")
        .arg(folder)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
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
        "list",
        "search",
        "readattr",
        "readextattr",
        "readsecurity",
        "file_inherit",
        "directory_inherit",
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
    let mut wanted = expected.to_vec();
    wanted.sort_unstable();

    entries.len() == 1 && rights == wanted
}

/// Run a system tool on `path`, quietly, failing when the tool does.
fn quiet(tool: &str, args: &[&str], path: &Path) -> Result<()> {
    let status = Command::new(tool)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(problem(format!("{tool} failed on {}", path.display())))
    }
}

/// Which of `paths` the helper itself can open: readable, a folder it can
/// enter, and for an app, its executable too. The helper answers, so its
/// groups and access lists count, not just permission bits.
async fn readable(client: &Client, ownership: &Ownership, paths: &[PathBuf]) -> Result<Vec<bool>> {
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
    let reply = client.inspect_owned(ownership, argv).await?;
    let output = run_output(&reply, 65536).unwrap_or_default();
    let answers: Vec<bool> = String::from_utf8_lossy(&output)
        .lines()
        .map(|line| line.trim() == "1")
        .collect();
    if reply["exitCode"] != 0 || answers.len() != paths.len() {
        return Err(Error::new(
            "input",
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
    pub name: String,
    pub executable: String,
}

impl Running {
    /// The running apps of `apps`, an app list the helper gave; a name or
    /// executable it does not know is empty.
    pub fn listed(apps: &Value) -> Vec<Self> {
        apps["apps"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|app| Self {
                name: app["name"].as_str().unwrap_or("").to_owned(),
                executable: app["executable"].as_str().unwrap_or("").to_owned(),
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
    pub paths: Vec<String>,
    /// The copies made for it, each `{"from", "to"}`.
    pub copied: Vec<Value>,
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
        _ => Vec::new(),
    };

    let root_path = PathBuf::from(ROOT);
    let checks: Vec<PathBuf> = targets
        .iter()
        .filter_map(|target| match target {
            Target::Path(path) if !path.starts_with(&root_path) => Some(path.clone()),
            _ => None,
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
                        "input",
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
                        "input",
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
    let mut failure = None;
    for placed in placed {
        match placed {
            Placed::Given(given) => paths.push(given),
            Placed::Copy { path, token } => {
                let copied = match copy(&path, root, &token) {
                    Ok(copied) => copied,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                };
                made.push((path.clone(), copied.clone()));
                if let Err(error) = owner.mark(&path, &copied) {
                    failure = Some(error);
                    break;
                }
                paths.push(copied.to_string_lossy().into_owned());
            }
        }
    }

    if failure.is_some() {
        // Nothing has opened these copies yet: they go with the failure.
        for folder in made.iter().filter_map(|(_, copy)| copy.parent()) {
            let _ = fs::remove_dir_all(folder);
        }
    }

    let mut settled = Vec::new();
    let mut leftover = Vec::new();
    for (token, owned) in tokens.iter().zip(owned) {
        let folder = root.join(token.as_str());
        match (fs::symlink_metadata(&folder).is_ok(), failure.is_some()) {
            (false, _) => settled.push((owned, State::Gone)),
            (true, false) => settled.push((owned, State::Made)),
            (true, true) => leftover.push(folder.to_string_lossy().into_owned()),
        }
    }
    let _ = owner.claims.settle_all(&settled);

    match failure {
        None => Ok(Handed {
            paths,
            copied: made
                .into_iter()
                .map(|(from, to)| json!({"from": from, "to": to}))
                .collect(),
        }),
        Some(error) if leftover.is_empty() => Err(error),
        Some(mut error) => {
            error.error.push_str(&format!(
                " Atlas could not remove what it began copying at {}; it stays claimed for this task.",
                leftover.join(", ")
            ));
            Err(error.with("leftover", leftover))
        }
    }
}

impl Owner<'_> {
    /// Label `copied`, a copy of `path` just made, with the host session it
    /// was made for and the app its files are opened with. A lost session
    /// label leaves the copy everyone's, as before.
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
        .ok_or_else(|| Error::new("input", format!("Cannot open {}", path.display())))?;
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
    )
    .and_then(|()| quiet("/bin/chmod", &["-R", "go-w"], &copy));
    if made.is_err() {
        let _ = fs::remove_dir_all(&folder);
        return Err(problem(format!(
            "Could not copy {} for the helper",
            path.display()
        )));
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
pub(crate) fn copy_app(root: &Path, token: &Token) -> Option<String> {
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
        .map(|app| crate::doing::bounded(&app))
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
    let output = Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(bundle.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
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
    pub folders: Vec<String>,
    /// How many copies were kept because another host session made them,
    /// or their label could not be read.
    pub kept_others: usize,
    /// The copies that could not be removed.
    pub failed: Vec<Failed>,
}

/// A copy folder that could not be removed, and why.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Failed {
    pub path: String,
    pub error: String,
}

impl Removed {
    /// The tokens of the folders removed.
    pub fn tokens(&self) -> Vec<Token> {
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
/// copy not in use goes, as it always did: a session only ever keeps more.
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
        let opened = nix::fcntl::openat(
            root,
            name,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        );
        let folder = match opened {
            Ok(fd) => fs::File::from(fd),
            Err(nix::errno::Errno::ENOENT) => return Ok(None),
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
        let private = self.metadata.is_dir()
            && self.metadata.uid() == nix::unistd::getuid().as_raw()
            && self.metadata.mode() & 0o022 == 0;
        if !private {
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
        let opened = nix::fcntl::openat(
            &self.folder,
            SESSION,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_NONBLOCK
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        );
        let mut file = match opened {
            Ok(fd) => fs::File::from(fd),
            Err(nix::errno::Errno::ENOENT) => return Label::None,
            Err(_) => return Label::Unreadable,
        };
        let plain = file.metadata().is_ok_and(|metadata| {
            metadata.is_file() && metadata.uid() == nix::unistd::getuid().as_raw()
        });
        let mut text = String::new();
        let read =
            std::io::Read::read_to_string(&mut std::io::Read::take(&mut file, 64), &mut text);
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
pub(crate) fn runs_from(executable: &str, folder: &Path) -> bool {
    let path = Path::new(executable);
    !executable.is_empty() && path.starts_with(folder) && path != folder
}

/// Where the copy folder `token` names is, under `root`.
pub(crate) fn copy_folder(root: &Path, token: &Token) -> PathBuf {
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
mod tests {
    use super::*;

    #[test]
    fn only_a_real_private_folder_of_this_account_counts() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("own");
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        assert!(private_folder(&folder));

        fs::set_permissions(&folder, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(!private_folder(&folder), "writable by others");

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&folder, &link).unwrap();
        assert!(!private_folder(&link), "a link is never followed");
        assert!(!private_folder(&dir.path().join("missing")));
    }

    #[test]
    fn a_symlinked_parent_keeps_personal_files_out_of_cleanup() {
        let shared = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let open = projects.path().join("Open");
        fs::DirBuilder::new().mode(0o700).create(&open).unwrap();
        let copy = copy_folder(&open, "a", "note.txt");
        age(&copy, 8);
        let atlas = shared.path().join("Atlas");
        std::os::unix::fs::symlink(projects.path(), &atlas).unwrap();
        let root = atlas.join("Open");
        assert!(private_folder(&root), "Open alone passes the checks");

        assert_eq!(
            remove_copies_in(&root, "note", &[], None),
            Removed::default()
        );
        prune_in(&root, &[]);
        let owned = root_if_owned_in(&root);
        if let Some(root) = &owned {
            fs::remove_dir_all(root).unwrap();
        }
        assert!(owned.is_none(), "uninstall must refuse the root");
        assert_eq!(fs::read_to_string(copy.join("note.txt")).unwrap(), "hello");
    }

    #[test]
    fn a_real_private_parent_allows_cleanup() {
        let shared = tempfile::tempdir().unwrap();
        let atlas = shared.path().join("Atlas");
        fs::DirBuilder::new().mode(0o700).create(&atlas).unwrap();
        let root = atlas.join("Open");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let closed = copy_folder(&root, "a", "note.txt");
        let old = copy_folder(&root, "b", "old.txt");
        age(&old, 8);

        assert_eq!(root_if_owned_in(&root), Some(root.clone()));
        assert_eq!(
            remove_copies_in(&root, "note", &[], None).folders,
            [closed.to_string_lossy().into_owned()]
        );
        prune_in(&root, &[]);
        assert!(!closed.exists());
        assert!(!old.exists());
        fs::remove_dir_all(root_if_owned_in(&root).unwrap()).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn a_copy_gets_its_own_folder_and_nobody_else_may_write_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let source = dir.path().join("note.txt");
        fs::write(&source, "hello").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o666)).unwrap();

        let first = copy(&source, &root, &Token::new()).unwrap();
        let second = copy(&source, &root, &Token::new()).unwrap();
        assert_ne!(first.parent(), second.parent(), "never replaced");
        assert_eq!(fs::read_to_string(&first).unwrap(), "hello");
        let mode = fs::metadata(&first).unwrap().mode() & 0o777;
        assert_eq!(mode & 0o022, 0, "group and others cannot write: {mode:o}");
    }

    #[test]
    fn the_grant_is_recognised_once_set() {
        let dir = tempfile::tempdir().unwrap();
        let me = nix::unistd::User::from_uid(nix::unistd::getuid())
            .unwrap()
            .unwrap()
            .name;
        assert!(!granted(dir.path(), &me));

        let grant = format!(
            "{me} allow list,search,readattr,readextattr,readsecurity,read,execute,file_inherit,directory_inherit"
        );
        let grant_with_write = format!("{grant},write,add_file");

        quiet("/bin/chmod", &["+a", &grant], dir.path()).unwrap();
        assert!(granted(dir.path(), &me));
        assert!(!granted(dir.path(), "someone-else"));

        // A grant that also lets the helper write is not the one wanted.
        let other = tempfile::tempdir().unwrap();
        quiet("/bin/chmod", &["+a", &grant_with_write], other.path()).unwrap();
        assert!(!granted(other.path(), &me));
    }

    /// An app bundle at `path` that calls itself Runner (runner on disk).
    fn runner(path: &Path) {
        fs::create_dir_all(path.join("Contents")).unwrap();
        fs::write(
            path.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>runner</string><key>CFBundleName</key><string>Runner</string></dict></plist>"#,
        )
        .unwrap();
    }

    /// A copy folder under `root` holding `name` (an app or a file).
    fn copy_folder(root: &Path, folder: &str, name: &str) -> PathBuf {
        let folder = root.join(folder);
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();

        if name.ends_with(".app") {
            runner(&folder.join(name));
        } else {
            fs::write(folder.join(name), "hello").unwrap();
        }

        folder
    }

    /// Make `folder` look `days` old.
    fn age(folder: &Path, days: u64) {
        let then = std::time::SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60);
        fs::File::open(folder).unwrap().set_modified(then).unwrap();
    }

    /// A copy is found by every name its app may run as, in any case, and
    /// is never removed while an app of one of those names still runs.
    #[test]
    fn copies_in_use_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let app = copy_folder(&root, "a", "Long Runner.app");
        let opened = copy_folder(&root, "b", "note.txt");
        fs::write(opened.join(OPENED_WITH), "TextEdit").unwrap();
        let other = copy_folder(&root, "c", "other.txt");

        for name in ["runner", "long runner"] {
            assert_eq!(copies_of(&root, name), std::slice::from_ref(&app), "{name}");
        }

        assert_eq!(copies_of(&root, "TEXTEDIT"), std::slice::from_ref(&opened));
        assert!(copies_of(&root, "Preview").is_empty());
        assert!(in_use(&app, &apps(&["RUNNER"])));
        assert!(
            in_use(&opened, &apps(&["TextEdit"])),
            "opened with a running app"
        );
        assert!(!in_use(&other, &apps(&["TextEdit", "Runner"])));

        // `close --app runner` while Runner still runs keeps its copy.
        assert_eq!(
            remove_copies_in(&root, "runner", &apps(&["Finder", "Runner"]), None),
            Removed::default()
        );
        assert!(app.exists());
        let removed = remove_copies_in(&root, "runner", &apps(&["Finder"]), None);
        assert_eq!(removed.folders, [app.to_string_lossy().into_owned()]);
        assert!(!app.exists());

        // Old copies go unless in use; new ones stay.
        age(&opened, 8);
        age(&other, 8);
        let new = copy_folder(&root, "d", "new.txt");
        prune_in(&root, &apps(&["textedit"]));
        assert!(opened.exists(), "in use");
        assert!(!other.exists(), "old and unused");
        assert!(new.exists(), "recent");
        prune_in(&root, &[]);
        assert!(!opened.exists());
    }

    fn key(id: &str) -> Key {
        Key::of(crate::host_session::Host::Codex, id)
    }

    /// Apps of these names running from somewhere outside any copy.
    fn apps(names: &[&str]) -> Vec<Running> {
        names
            .iter()
            .map(|name| Running {
                name: (*name).to_owned(),
                executable: format!("/Applications/{name}.app/Contents/MacOS/{name}"),
            })
            .collect()
    }

    /// A copy root under `dir` holding one copy opened with TextEdit.
    fn one_copy(dir: &Path) -> (PathBuf, PathBuf) {
        let root = dir.join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let copy = copy_folder(&root, "copy", "notes.rtf");
        fs::write(copy.join(OPENED_WITH), "TextEdit").unwrap();
        (root, copy)
    }

    #[test]
    fn an_unreadable_label_keeps_the_copy_for_a_session() {
        type Make = fn(&Path);
        let unreadable: [(&str, Make); 5] = [
            ("a folder", |label| fs::create_dir(label).unwrap()),
            ("a link", |label| {
                std::os::unix::fs::symlink("/etc/hosts", label).unwrap();
            }),
            ("empty", |label| fs::write(label, "").unwrap()),
            ("two lines", |label| {
                fs::write(label, format!("{}\n{}\n", key("left"), key("left"))).unwrap();
            }),
            ("not a key", |label| fs::write(label, "../x\n").unwrap()),
        ];

        for (what, make) in unreadable {
            let dir = tempfile::tempdir().unwrap();
            let (root, copy) = one_copy(dir.path());
            make(&copy.join(SESSION));
            assert_eq!(labelled(&copy), Label::Unreadable, "{what}");

            let removed = remove_copies_in(&root, "TextEdit", &[], Some(&key("left")));
            assert_eq!(
                (removed.folders.len(), removed.kept_others),
                (0, 1),
                "{what}"
            );
            assert!(copy.exists(), "{what}");

            let removed = remove_copies_in(&root, "TextEdit", &[], None);
            assert_eq!(removed.folders.len(), 1, "without a session: {what}");
        }
    }

    #[test]
    fn a_copy_an_app_runs_from_is_in_use_even_without_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let app = copy_folder(&root, "app", "Long Runner.app");
        let beside = copy_folder(&root, "app-beside", "note.txt");
        let running = [Running {
            name: String::new(),
            executable: app
                .join("Long Runner.app/Contents/MacOS/runner")
                .to_string_lossy()
                .into_owned(),
        }];

        assert!(in_use(&app, &running));
        assert!(!in_use(&beside, &running), "a folder whose name it begins");
        assert_eq!(
            remove_copies_in(&root, "runner", &running, None),
            Removed::default()
        );
        age(&app, 8);
        prune_in(&root, &running);
        assert!(app.exists());
    }

    #[test]
    fn a_copy_that_cannot_be_removed_is_reported_not_dropped() {
        // Permissions do not hold root back, so nothing would fail.
        if nix::unistd::geteuid().is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, copy) = one_copy(dir.path());
        fs::set_permissions(&copy, fs::Permissions::from_mode(0o500)).unwrap();

        let removed = remove_copies_in(&root, "TextEdit", &[], None);
        fs::set_permissions(&copy, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(removed.folders.is_empty());
        assert_eq!(removed.failed.len(), 1);
        assert_eq!(removed.failed[0].path, copy.to_string_lossy());
        assert!(removed.failed[0].error.contains("ermission"));
        assert!(copy.exists());
    }

    #[test]
    fn a_copy_folder_records_its_session_out_of_the_helpers_sight() {
        let dir = tempfile::tempdir().unwrap();
        let folder = copy_folder(dir.path(), "a", "note.txt");
        fs::write(folder.join(OPENED_WITH), "TextEdit").unwrap();
        label(&folder, &key("right")).unwrap();

        let session = folder.join(SESSION);
        assert_eq!(
            fs::read_to_string(&session).unwrap(),
            format!("{}\n", key("right"))
        );
        assert_eq!(fs::metadata(&session).unwrap().mode() & 0o777, 0o600);
        assert_eq!(labelled(&folder), Label::Session(key("right")));
        assert_eq!(entries(&folder), [folder.join("note.txt")]);
        assert_eq!(folder_names(&folder), ["TextEdit", "note"]);
        assert!(label(&folder, &key("other")).is_err(), "never replaced");
    }

    #[test]
    fn close_keeps_another_sessions_copies_and_counts_them() {
        let made = |dir: &Path| {
            let root = dir.join("root");
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            let own = copy_folder(&root, "own", "own.txt");
            let other = copy_folder(&root, "other", "other.txt");
            let nobody = copy_folder(&root, "nobody", "nobody.txt");
            let busy = copy_folder(&root, "busy", "busy.txt");
            for folder in [&own, &other, &nobody, &busy] {
                fs::write(folder.join(OPENED_WITH), "TextEdit").unwrap();
            }
            label(&own, &key("left")).unwrap();
            label(&other, &key("right")).unwrap();
            fs::write(busy.join(OPENED_WITH), "TextEdit\nBusy").unwrap();
            (root, [own, other, nobody, busy])
        };

        let dir = tempfile::tempdir().unwrap();
        let (root, [own, other, nobody, busy]) = made(dir.path());
        let removed = remove_copies_in(&root, "TextEdit", &apps(&["Busy"]), Some(&key("left")));
        assert_eq!(removed.kept_others, 1);
        let mut folders = removed.folders.clone();
        folders.sort();
        let mut expected = [own, nobody].map(|folder| folder.to_string_lossy().into_owned());
        expected.sort();
        assert_eq!(folders, expected);
        assert!(other.exists() && busy.exists());

        let dir = tempfile::tempdir().unwrap();
        let (root, [.., busy]) = made(dir.path());
        let removed = remove_copies_in(&root, "TextEdit", &apps(&["Busy"]), None);
        assert_eq!(
            (removed.folders.len(), removed.kept_others),
            (3, 0),
            "as before"
        );
        assert!(busy.exists(), "one in use always stays");
    }

    #[test]
    fn prune_ignores_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let other = copy_folder(&root, "other", "other.txt");
        label(&other, &key("right")).unwrap();
        age(&other, 8);

        prune_in(&root, &[]);
        assert!(!other.exists());
    }

    #[test]
    fn a_copy_folder_is_named_by_its_token_and_its_app_never_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let folder = |name: &str| {
            let token = Token::new();
            copy_folder(dir.path(), token.as_str(), name);
            token
        };
        let token = folder("Quarterly Plan.txt");
        fs::write(
            dir.path().join(token.as_str()).join(OPENED_WITH),
            "TextEdit\nTextEdit Helper",
        )
        .unwrap();
        let app = folder("Long Runner.app");
        let bare = folder("notes.txt");

        assert_eq!(copy_app(dir.path(), &token).as_deref(), Some("TextEdit"));
        assert_eq!(copy_app(dir.path(), &app).as_deref(), Some("Long Runner"));
        assert_eq!(copy_app(dir.path(), &bare), None);
        assert!(copy_exists(dir.path(), &token));
        assert!(!copy_exists(dir.path(), &Token::new()));

        let path = format!("{ROOT}/{token}/Quarterly Plan.txt");
        assert_eq!(token_of(&path), Some(token.clone()));
        assert_eq!(token_of(&format!("{ROOT}/{token}")), Some(token));
        assert_eq!(token_of("/tmp/Quarterly Plan.txt"), None);
        assert_eq!(token_of(&format!("{ROOT}/not-a-token/x")), None);
        assert_eq!(token_of(&format!("{ROOT}/../{}", Token::new())), None);
    }

    /// A private copy root and an output folder under `dir`, and the claims
    /// of a task of a fresh session kept there, with that task.
    fn claiming(dir: &Path) -> (PathBuf, PathBuf, Claims, crate::task::Task) {
        let root = dir.join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let output = dir.join("output");
        fs::create_dir(&output).unwrap();
        let task = crate::task::Task::of(&key("claiming"), Some("probe")).unwrap();
        let store = crate::task::store::Store::at(&output);
        (root, output, Claims::in_store(store, task.clone()), task)
    }

    /// The copies of `sources`, as `hand_over` places them once checked.
    fn to_copy(sources: &[&Path]) -> Vec<Placed> {
        sources
            .iter()
            .map(|path| Placed::Copy {
                path: path.to_path_buf(),
                token: Token::new(),
            })
            .collect()
    }

    /// The states `output`'s record of `task` holds, in claim order.
    fn states(output: &Path, task: &crate::task::Task) -> Vec<State> {
        crate::task::store::Store::at(output)
            .load(task.key())
            .unwrap()
            .claims()
            .map(|(_, state)| state)
            .collect()
    }

    #[test]
    fn pruning_names_each_copy_it_removed_by_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let (old, recent) = (Token::new(), Token::new());
        let gone = copy_folder(&root, old.as_str(), "old.txt");
        age(&gone, 8);
        copy_folder(&root, recent.as_str(), "new.txt");

        assert_eq!(prune_in(&root, &[]), [old]);
        assert!(!gone.exists());
    }

    #[test]
    fn every_copy_is_claimed_in_one_write_before_any_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let (root, output, claims, task) = claiming(dir.path());
        let owner = Owner {
            session: None,
            claims: &claims,
            opened_with: &[],
        };
        let sources = [dir.path().join("a.txt"), dir.path().join("b.txt")];
        for source in &sources {
            fs::write(source, "hello").unwrap();
        }
        let placed = to_copy(&[&sources[0], &sources[1]]);

        let mut seen = Vec::new();
        let handed = handed_in(&root, placed, &owner, |path, root, token| {
            seen.push(states(&output, &task));
            copy(path, root, token)
        })
        .unwrap();

        assert_eq!(seen, [[State::Intent, State::Intent]; 2]);
        assert_eq!(handed.paths.len(), 2);
        assert_eq!(states(&output, &task), [State::Made, State::Made]);
    }

    #[test]
    fn a_failed_copy_takes_the_ones_before_it_and_each_is_settled_once_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (root, output, claims, task) = claiming(dir.path());
        let owner = Owner {
            session: Some(&key("claiming")),
            claims: &claims,
            opened_with: &["TextEdit".to_owned()],
        };
        let made = dir.path().join("made.txt");
        fs::write(&made, "hello").unwrap();
        let missing = dir.path().join("missing.txt");
        let placed = to_copy(&[&made, &missing, &made]);

        let error = handed_in(&root, placed, &owner, copy).unwrap_err();

        assert!(error.error.contains("Could not copy"), "{error}");
        assert!(error.details.get("leftover").is_none());
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            0,
            "the first copy went"
        );
        assert_eq!(states(&output, &task), [State::Gone; 3]);
    }

    #[test]
    fn a_failed_copy_whose_folder_stays_is_left_claimed_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (root, output, claims, task) = claiming(dir.path());
        let owner = Owner {
            session: None,
            claims: &claims,
            opened_with: &[],
        };
        let source = dir.path().join("note.txt");
        fs::write(&source, "hello").unwrap();

        // The copy fails after making its folder, and cannot remove it.
        let error = handed_in(&root, to_copy(&[&source]), &owner, |_, root, token| {
            fs::create_dir(root.join(token.as_str())).unwrap();
            Err(problem("Could not copy"))
        })
        .unwrap_err();

        let leftover = error.details["leftover"].as_array().unwrap();
        assert_eq!(leftover.len(), 1);
        assert!(Path::new(leftover[0].as_str().unwrap()).exists());
        assert_eq!(states(&output, &task), [State::Intent]);
    }

    #[test]
    fn the_record_keeps_no_file_names_or_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (root, output, claims, task) = claiming(dir.path());
        let owner = Owner {
            session: None,
            claims: &claims,
            opened_with: &[],
        };
        let source = dir.path().join("Secret plan.rtf");
        fs::write(&source, "hello").unwrap();

        let handed = handed_in(&root, to_copy(&[&source]), &owner, copy).unwrap();

        assert!(handed.paths[0].ends_with("Secret plan.rtf"));
        let record = fs::read_to_string(output.join(format!("tasks/{}.json", task.key()))).unwrap();
        assert!(!record.contains("Secret"), "{record}");
        assert!(
            !record.contains(&dir.path().to_string_lossy().into_owned()),
            "{record}"
        );
        assert!(record.contains(token_of_copy(&handed.paths[0]).as_str()));
    }

    /// The token of the copy folder `copied` lies in, wherever the root is.
    fn token_of_copy(copied: &str) -> Token {
        folder_token(Path::new(copied).parent().unwrap()).unwrap()
    }

    #[test]
    fn names_come_from_the_file_and_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Long Runner.app");
        runner(&app);
        let names = names(&app);

        for name in ["Long Runner", "Runner", "runner"] {
            assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
        }
    }

    /// A private copy root in `dir` holding a copy folder for `session`,
    /// and that folder's token.
    fn labelled_copy(dir: &Path, session: &Key) -> (PathBuf, Token) {
        let root = dir.join("copies");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let token = Token::new();
        let folder = root.join(token.as_str());
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        fs::write(folder.join("notes.rtf"), "hello").unwrap();
        label(&folder, session).unwrap();
        (root, token)
    }

    #[test]
    fn a_copy_this_session_may_take_is_removed_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let session = Key::of(crate::host_session::Host::Codex, "removing");
        let (root, token) = labelled_copy(&base, &session);

        assert_eq!(
            remove_copy(&root, &token, &session, &[]),
            CopyRemoval::Removed
        );
        assert!(
            fs::read_dir(&root).unwrap().next().is_none(),
            "nothing moved anywhere"
        );
        assert_eq!(
            remove_copy(&root, &token, &session, &[]),
            CopyRemoval::AlreadyGone
        );
    }

    #[test]
    fn a_copy_labelled_for_another_session_or_unreadably_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let (own, other) = (
            Key::of(crate::host_session::Host::Codex, "own"),
            Key::of(crate::host_session::Host::Codex, "other"),
        );
        let (root, theirs) = labelled_copy(&base, &other);
        assert_eq!(
            remove_copy(&root, &theirs, &own, &[]),
            CopyRemoval::AnotherSession
        );
        let linked = Token::new();
        let folder = root.join(linked.as_str());
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", folder.join(SESSION)).unwrap();
        assert_eq!(
            remove_copy(&root, &linked, &own, &[]),
            CopyRemoval::Unreadable
        );
        assert!(root.join(theirs.as_str()).exists() && folder.exists());
    }

    #[test]
    fn a_copy_folder_swapped_after_it_was_opened_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let session = Key::of(crate::host_session::Host::Codex, "swapped");
        let (root, token) = labelled_copy(&base, &session);
        let parent = open_folder(&root).unwrap();
        let copy = OpenCopy::open(&parent, token.as_str()).unwrap().unwrap();
        assert!(copy.may_take(Some(&session)).is_ok());

        // Another folder takes its place after it was judged.
        let path = root.join(token.as_str());
        fs::rename(&path, base.join("judged")).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        assert!(!copy.still_at(&path), "the path names another folder now");
        assert!(copy.still_at(&base.join("judged")));
    }

    #[test]
    fn a_copy_quit_target_is_only_one_this_session_may_take() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let (own, other) = (
            Key::of(crate::host_session::Host::Codex, "own"),
            Key::of(crate::host_session::Host::Codex, "other"),
        );
        let (root, token) = labelled_copy(&base, &own);
        let folder = root.join(token.as_str());
        let metadata = fs::symlink_metadata(&folder).unwrap();
        assert_eq!(
            copy_node(&root, &token, &own),
            Some(format!("{}:{}", metadata.dev(), metadata.ino()))
        );
        assert_eq!(copy_node(&root, &token, &other), None);
    }
}
