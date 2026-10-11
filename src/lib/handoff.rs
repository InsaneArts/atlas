//! Copies of the person's apps and files for the helper to open, when the
//! helper cannot read the originals. Each copy lives in a folder of its own
//! under ROOT, private to this account apart from an inherited read-only
//! grant to the helper, and nothing there is ever replaced: a new copy gets
//! a new folder, so a running app's files never change under it.
use crate::{
    client::Client,
    control::Ownership,
    error::{Error, Result},
    transport::run_output,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
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
        Err(error) => return Err(error.into()),
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

/// Run a system tool on `path`, quietly; whether it succeeded.
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
    let reply = client.run_owned(ownership, argv).await?;
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

/// What a copy of `paths` (a path here, or not) looks like to the helper.
pub enum Target {
    /// An app name or URL, passed through.
    Name(String),
    /// A file or folder here, made absolute.
    Path(PathBuf),
}

/// `target` as given: a path here when one exists (made absolute, since the
/// helper has its own working directory), otherwise a name or URL.
pub fn resolve(target: &str) -> Target {
    match fs::canonicalize(target) {
        Ok(path) => Target::Path(path),
        Err(_) => Target::Name(target.to_owned()),
    }
}

/// The paths among `targets` as the helper can open them: unchanged when it
/// can read them (or they are copies already), otherwise copied first. The
/// copies made are added to `copied`.
/// A running app, as the helper lists it.
pub struct Running {
    pub name: String,
    pub executable: String,
}

/// Which app the copies are opened with, by the names it may run as.
const OPENED_WITH: &str = ".opened-with";

pub async fn hand_over(
    client: &Client,
    ownership: &Ownership,
    targets: Vec<Target>,
    running: &[Running],
    copied: &mut Vec<Value>,
) -> Result<Vec<String>> {
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
    let mut answers = readable(client, ownership, &checks).await?.into_iter();
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
                        None => root.insert(self::root(client.config.helper_uid)?),
                    };
                    let copy = copy(&path, root)?;
                    if path.extension().is_none_or(|extension| extension != "app")
                        && !opened_with.is_empty()
                        && let Some(folder) = copy.parent()
                    {
                        fs::write(folder.join(OPENED_WITH), opened_with.join("\n"))?;
                    }
                    copied.push(json!({"from": path, "to": copy}));
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
    fs::DirBuilder::new().mode(0o700).create(&folder)?;
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

/// The copies of `app`: those named so, or whose bundle calls itself so.
fn copies_of(app: &str) -> Vec<PathBuf> {
    folders()
        .into_iter()
        .filter(|folder| {
            let with = fs::read_to_string(folder.join(OPENED_WITH)).unwrap_or_default();
            with.lines().any(|name| name == app)
                || entries(folder)
                    .iter()
                    .any(|copy| names(copy).iter().any(|name| name == app))
        })
        .collect()
}

/// The copy folders under ROOT, when ROOT is Atlas's own private folder.
fn folders() -> Vec<PathBuf> {
    let root = Path::new(ROOT);
    if !private_folder(root) {
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

/// Remove the copies of `app`, which has quit; the folders removed.
pub fn remove_copies_of(app: &str) -> Vec<String> {
    copies_of(app)
        .into_iter()
        .filter(|folder| fs::remove_dir_all(folder).is_ok())
        .map(|folder| folder.to_string_lossy().into_owned())
        .collect()
}

/// Remove copies older than KEEP whose app is not running in `running`
/// (the helper's app names), so a copy nobody closed does not stay forever.
pub fn prune(running: &[&str]) {
    for folder in folders() {
        let old = fs::symlink_metadata(&folder)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEEP);
        let in_use = entries(&folder).iter().any(|copy| {
            names(copy)
                .iter()
                .any(|name| running.contains(&name.as_str()))
        });
        if old && !in_use {
            let _ = fs::remove_dir_all(&folder);
        }
    }
}

/// ROOT itself, for uninstall, when it is Atlas's own folder.
pub fn root_if_owned() -> Option<PathBuf> {
    let root = PathBuf::from(ROOT);
    private_folder(&root).then_some(root)
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

    #[test]
    fn names_come_from_the_file_and_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Long Runner.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>runner</string><key>CFBundleName</key><string>Runner</string></dict></plist>"#,
        )
        .unwrap();
        let names = names(&app);
        for name in ["Long Runner", "Runner", "runner"] {
            assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
        }
    }
}
