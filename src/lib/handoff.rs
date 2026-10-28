//! Copies of the person's apps and files for the helper to open, when the
//! helper cannot read the originals. Each copy lives in a folder of its own
//! under ROOT, private to this account apart from an inherited read-only
//! grant to the helper, and nothing there is ever replaced: a new copy gets
//! a new folder, so a running app's files never change under it.
use crate::{
    agent::same_text,
    client::Client,
    control::{Ownership, Token},
    error::{Error, Result},
    host_session::{HostSession, Key},
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

/// Which app the copies are opened with, by the names it may run as.
const OPENED_WITH: &str = ".opened-with";

/// The host session a copy folder was made for: its key, alone on a line.
const SESSION: &str = ".session";

/// What [`hand_over`] gives the helper to open.
pub(crate) struct Handed {
    /// The targets as the helper can open them, in order.
    pub paths: Vec<String>,
    /// The copies made for it, each `{"from", "to"}`.
    pub copied: Vec<Value>,
}

/// The paths among `targets` as the helper can open them: unchanged when it
/// can read them (or they are copies already), otherwise copied first. When
/// one cannot be handed over, the copies already made for the others go.
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

    let mut made = Vec::new();
    let session = client.host_session.as_ref().map(HostSession::key);
    let placed = place(
        targets,
        answers,
        running,
        &opened_with,
        client.config.helper_uid,
        session,
        &mut made,
    );

    match placed {
        Ok(paths) => Ok(Handed {
            paths,
            copied: made
                .into_iter()
                .map(|(from, to)| json!({"from": from, "to": to}))
                .collect(),
        }),
        Err(error) => {
            // Nothing has opened these copies yet: they go with the failure.
            for folder in made.iter().filter_map(|(_, copy)| copy.parent()) {
                let _ = fs::remove_dir_all(folder);
            }
            Err(error)
        }
    }
}

/// [`hand_over`]'s targets in order, given `answers` (whether the helper
/// can read each path outside ROOT), for `helper` and the host session of
/// the command copying, if any; each copy made is added to `made` as
/// (original, copy) as soon as it exists.
fn place(
    targets: Vec<Target>,
    answers: Vec<bool>,
    running: &[Running],
    opened_with: &[String],
    helper: u32,
    session: Option<&Key>,
    made: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<String>> {
    let root_path = PathBuf::from(ROOT);
    let mut answers = answers.into_iter();
    let mut root = None;
    let mut out = Vec::new();

    for target in targets {
        let path = match target {
            Target::Name(name) => {
                out.push(name);
                continue;
            }
            Target::Path(path) if path.starts_with(&root_path) => path,
            Target::Path(path) => {
                if answers.next().unwrap_or(false) {
                    path
                } else if path.is_dir()
                    && path.extension().is_none_or(|extension| extension != "app")
                {
                    // A whole folder could be any size; only apps and files are copied.
                    return Err(Error::new(
                        "input",
                        format!(
                            "The helper cannot read {}. Atlas copies apps and files, not folders; share a project with atlas environment.",
                            path.display()
                        ),
                    ));
                } else {
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

                    let root = match &root {
                        Some(root) => root,
                        None => root.insert(self::root(helper)?),
                    };

                    let copy = copy(&path, root)?;
                    made.push((path.clone(), copy.clone()));
                    if let (Some(session), Some(folder)) = (session, copy.parent()) {
                        // A label for close and the session's record; a
                        // lost one leaves the copy everyone's, as before.
                        let _ = label(folder, session);
                    }

                    if path.extension().is_none_or(|extension| extension != "app")
                        && !opened_with.is_empty()
                        && let Some(folder) = copy.parent()
                    {
                        fs::write(folder.join(OPENED_WITH), opened_with.join("\n"))?;
                    }

                    copy
                }
            }
        };

        out.push(path.to_string_lossy().into_owned());
    }

    Ok(out)
}

/// Copy `path` into a new folder of its own under `root`, read-only for the
/// helper: the copy keeps execute bits but nobody else may write to it.
fn copy(path: &Path, root: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::new("input", format!("Cannot open {}", path.display())))?;
    let folder = root.join(crate::control::token());
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

/// The host session a copy folder was made for, when it names one.
fn labelled(folder: &Path) -> Option<Key> {
    let text = crate::helper_info::read_own(&folder.join(SESSION), 64)?;
    Key::parse(std::str::from_utf8(&text).ok()?.trim())
}

/// The token of the copy folder `path` lies in, when it lies in one under
/// ROOT: the folder's own name, never the copy's.
pub(crate) fn token_of(path: &str) -> Option<Token> {
    let rest = Path::new(path).strip_prefix(ROOT).ok()?;
    Token::parse(rest.components().next()?.as_os_str().to_str()?)
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
/// `running` (the helper's app names). Such a folder is never removed.
fn in_use(folder: &Path, running: &[&str]) -> bool {
    folder_names(folder)
        .iter()
        .any(|name| running.iter().any(|app| same_text(app, name)))
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
    /// How many copies another host session made were kept.
    pub kept_others: usize,
}

/// Remove the copies of `app` under `root` (ROOT, but in tests), which has
/// quit, except any still in use by an app among `running` (the helper's
/// app names) and, for a command of host `session`, any another session
/// made. Without a session, every copy not in use goes, as it always did:
/// a session only ever keeps more.
pub(crate) fn remove_copies_in(
    root: &Path,
    app: &str,
    running: &[&str],
    session: Option<&Key>,
) -> Removed {
    let mut removed = Removed::default();
    for folder in copies_of(root, app) {
        if in_use(&folder, running) {
            continue;
        }
        let others = labelled(&folder).is_some_and(|made| session.is_some_and(|own| *own != made));
        if others {
            removed.kept_others += 1;
        } else if fs::remove_dir_all(&folder).is_ok() {
            removed.folders.push(folder.to_string_lossy().into_owned());
        }
    }

    removed
}

/// Remove copies older than KEEP that are not in use by an app among
/// `running` (the helper's app names), so a copy nobody closed does not
/// stay forever.
pub(crate) fn prune(running: &[&str]) {
    prune_in(Path::new(ROOT), running);
}

fn prune_in(root: &Path, running: &[&str]) {
    for folder in folders(root) {
        let old = fs::symlink_metadata(&folder)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEEP);
        if old && !in_use(&folder, running) {
            let _ = fs::remove_dir_all(&folder);
        }
    }
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

        let first = copy(&source, &root).unwrap();
        let second = copy(&source, &root).unwrap();
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
        assert!(in_use(&app, &["RUNNER"]));
        assert!(in_use(&opened, &["TextEdit"]), "opened with a running app");
        assert!(!in_use(&other, &["TextEdit", "Runner"]));

        // `close --app runner` while Runner still runs keeps its copy.
        assert_eq!(
            remove_copies_in(&root, "runner", &["Finder", "Runner"], None),
            Removed::default()
        );
        assert!(app.exists());
        let removed = remove_copies_in(&root, "runner", &["Finder"], None);
        assert_eq!(removed.folders, [app.to_string_lossy().into_owned()]);
        assert!(!app.exists());

        // Old copies go unless in use; new ones stay.
        age(&opened, 8);
        age(&other, 8);
        let new = copy_folder(&root, "d", "new.txt");
        prune_in(&root, &["textedit"]);
        assert!(opened.exists(), "in use");
        assert!(!other.exists(), "old and unused");
        assert!(new.exists(), "recent");
        prune_in(&root, &[]);
        assert!(!opened.exists());
    }

    fn key(id: &str) -> Key {
        Key::of(crate::host_session::Host::Codex, id)
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
        assert_eq!(labelled(&folder), Some(key("right")));
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
        let removed = remove_copies_in(&root, "TextEdit", &["Busy"], Some(&key("left")));
        assert_eq!(removed.kept_others, 1);
        let mut folders = removed.folders.clone();
        folders.sort();
        let mut expected = [own, nobody].map(|folder| folder.to_string_lossy().into_owned());
        expected.sort();
        assert_eq!(folders, expected);
        assert!(other.exists() && busy.exists());

        let dir = tempfile::tempdir().unwrap();
        let (root, [.., busy]) = made(dir.path());
        let removed = remove_copies_in(&root, "TextEdit", &["Busy"], None);
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
}
