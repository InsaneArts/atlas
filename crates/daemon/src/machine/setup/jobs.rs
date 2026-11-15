//! The jobs that run Atlas in the guest: the machine daemon in the guest
//! account's own session, and the root installer in the system's, each
//! written root's own and started; and the pairing secret handed to the
//! guest account, from the share its host gave it.
use super::{Account, Places, System, io, refused, sync};
use crate::machine::{
    installer::{
        copy::{BUNDLE, copy_in},
        own,
    },
    launch::LABEL,
};
use atlas_core::{
    desktop::DesktopId,
    error::Result,
    files::Token,
    fs_move::{FileName, Replace, rename_within},
    stage::Snapshot,
};
use nix::{
    fcntl::{AtFlags, OFlag, open, openat},
    sys::stat::{Mode, SFlag, fchmod, fstat, fstatat, mkdirat},
    unistd::{UnlinkatFlags, fchown, unlinkat},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Read,
    os::unix::fs::{PermissionsExt, chown},
    path::Path,
};

/// The installed Atlas's program, as the guest names it.
const PROGRAM: &str = "/Applications/Atlas.app/Contents/MacOS/atlas";
/// The root installer's own program, in its own copy of the bundle it
/// replaces.
const INSTALLER_PROGRAM: &str =
    "/Library/Application Support/Atlas/Installer/Atlas.app/Contents/MacOS/atlas";
/// The root installer's job.
const INSTALLER_LABEL: &str = "com.insanearts.atlas.machine-installer";
/// The longest pairing secret read.
const SECRET: u64 = 256;

/// Write and start both jobs for `account`, the guest daemon for `desktop`
/// pairing for `enrolment`, and hand it the pairing secret unless `paired`.
pub(super) fn jobs(
    places: &Places,
    system: &impl System,
    desktop: &DesktopId,
    enrolment: &Token,
    account: &str,
    paired: bool,
) -> Result<Value> {
    let Some(owner) = system.account(account) else {
        return Err(refused("the guest account is not there"));
    };
    let agent = places.agents().join(format!("{LABEL}.plist"));
    let arguments = [
        PROGRAM,
        "daemon",
        "--machine",
        "--desktop-id",
        desktop.as_str(),
        "--enrolment",
        enrolment.as_str(),
    ];
    keep_root(
        system,
        &agent,
        plist(LABEL, &arguments, true).as_bytes(),
        0o644,
    )?;

    installer_copy(places, system)?;
    let daemon = places.daemons().join(format!("{INSTALLER_LABEL}.plist"));
    keep_root(
        system,
        &daemon,
        plist(
            INSTALLER_LABEL,
            &[INSTALLER_PROGRAM, "machine-installer"],
            false,
        )
        .as_bytes(),
        0o644,
    )?;

    let secret = hand_secret(places, system, &owner, paired)?;

    let daemon_path = daemon.to_string_lossy().into_owned();
    let started_installer = system.launchctl(&["bootstrap", "system", &daemon_path])
        || system.launchctl(&["kickstart", "-k", &format!("system/{INSTALLER_LABEL}")]);
    let domain = format!("gui/{}", owner.uid);
    let agent_path = agent.to_string_lossy().into_owned();
    let started_daemon = system.launchctl(&["bootstrap", &domain, &agent_path])
        || system.launchctl(&["kickstart", "-k", &format!("{domain}/{LABEL}")]);
    if !started_installer || !started_daemon {
        // launchd may not take it yet, as before the account's session
        // starts: asked again, it may pass.
        return Err(atlas_core::error::Error::new(
            atlas_core::error::Code::Daemon,
            "Atlas's setup could not start a job yet.",
        ));
    }
    Ok(json!({"ok": true, "secret": secret, "installer": "started", "daemon": "started"}))
}

/// Give the root installer its own copy of the published Atlas: kept when
/// it is that very code, else made again by root from a snapshot of it,
/// verified, and put in place.
fn installer_copy(places: &Places, system: &impl System) -> Result<()> {
    let published = places.applications().join(BUNDLE);
    let Some(cdhash) = system.verified(&published) else {
        return Err(refused("the published Atlas is not Atlas's, signed"));
    };
    let home = places.installer();
    std::fs::create_dir_all(&home).map_err(io)?;
    let wanted = |bundle: &Path| system.verified(bundle).as_ref() == Some(&cdhash);
    if wanted(&home.join(own::OWN)) {
        return Ok(());
    }
    let making = home.join("Making");
    match std::fs::remove_dir_all(&making) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(error)),
        _ => {}
    }
    std::fs::create_dir(&making).map_err(io)?;
    std::fs::set_permissions(&making, std::fs::Permissions::from_mode(0o700)).map_err(io)?;
    let into = std::fs::File::open(&making).map_err(io)?;
    let name = FileName::new(BUNDLE).expect("a plain name");
    copy_in(
        &Snapshot::of(&published)?,
        &published,
        &into,
        &name,
        system.root(),
    )?;
    if !wanted(&making.join(BUNDLE)) {
        return Err(refused("the installer's copy is not the published Atlas"));
    }
    own::take(&home, Some((&into, &name)), &wanted)?;
    std::fs::remove_dir(&making).map_err(io)?;
    if system.root() {
        crate::machine::launch::root_owned(&home.join(own::OWN).join("Contents/MacOS/atlas"))?;
    }
    sync(&home)
}

/// Keep `bytes` at `file`, written whole beside it and renamed, mode
/// `mode`, root's own, and its folder written through.
fn keep_root(system: &impl System, file: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let folder = file
        .parent()
        .ok_or_else(|| refused("a job has no folder"))?;
    std::fs::create_dir_all(folder).map_err(io)?;
    let writing = folder.join(format!(
        ".{}.writing",
        file.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_file(&writing);
    let mut options = std::fs::OpenOptions::new();
    std::os::unix::fs::OpenOptionsExt::custom_flags(
        options.write(true).create_new(true),
        libc::O_NOFOLLOW,
    );
    let mut made = options.open(&writing).map_err(io)?;
    std::io::Write::write_all(&mut made, bytes).map_err(io)?;
    made.set_permissions(std::fs::Permissions::from_mode(mode))
        .map_err(io)?;
    if system.root() {
        chown(&writing, Some(0), Some(0)).map_err(io)?;
    }
    made.sync_all().map_err(io)?;
    std::fs::rename(&writing, file).map_err(io)?;
    sync(folder)
}

/// Hand the pairing secret from the share to `owner`'s private Atlas
/// folder; what was found. A share with none passes only when it is in
/// place already or the pairing is complete. Every folder below the home
/// is opened from the one above it, never through a link, and given to
/// its owner by its descriptor, so a link the guest account placed never
/// turns root's change onto another file.
fn hand_secret(
    places: &Places,
    system: &impl System,
    owner: &Account,
    paired: bool,
) -> Result<&'static str> {
    let Some(secret) = read_secret(&places.share().join("pairing-secret"))? else {
        if secret_present(owner) {
            return Ok("present");
        }
        if paired {
            return Ok("paired");
        }
        return Err(refused("no pairing secret was given"));
    };
    let root = system.root();
    let home = opened(None, &owner.home)?;
    let library = folder_in(&home, "Library", owner, root, false)?;
    let support = folder_in(&library, "Application Support", owner, root, false)?;
    let atlas = folder_in(&support, "Atlas", owner, root, true)?;
    let machine = folder_in(&atlas, "Machine", owner, root, true)?;
    let writing = c".pairing-secret.writing";
    match unlinkat(&machine, writing, UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => return Err(io(error)),
    }
    let made = File::from(
        openat(
            &machine,
            writing,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .map_err(io)?,
    );
    std::io::Write::write_all(&mut &made, &secret).map_err(io)?;
    if root {
        fchown(&made, Some(owner.uid.into()), Some(owner.gid.into())).map_err(io)?;
    }
    fchmod(&made, Mode::S_IRUSR | Mode::S_IWUSR).map_err(io)?;
    made.sync_all().map_err(io)?;
    rename_within(&machine, writing, c"pairing-secret", Replace::Allowed).map_err(io)?;
    machine.sync_all().map_err(io)?;
    Ok("copied")
}

/// Whether the secret is in `owner`'s private Atlas folder already, as a
/// regular file, every folder to it opened never through a link.
fn secret_present(owner: &Account) -> bool {
    let mut folder = match opened(None, &owner.home) {
        Ok(home) => home,
        Err(_) => return false,
    };
    for name in ["Library", "Application Support", "Atlas", "Machine"] {
        folder = match opened(Some(&folder), Path::new(name)) {
            Ok(inner) => inner,
            Err(_) => return false,
        };
    }
    fstatat(&folder, c"pairing-secret", AtFlags::AT_SYMLINK_NOFOLLOW)
        .is_ok_and(|seen| SFlag::from_bits_truncate(seen.st_mode) & SFlag::S_IFMT == SFlag::S_IFREG)
}

/// The folder at `path`, below `parent` when given, opened never through
/// a link at its last step.
fn opened(parent: Option<&File>, path: &Path) -> Result<File> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let opened = match parent {
        Some(parent) => openat(parent, path, flags, Mode::empty()),
        None => open(path, flags, Mode::empty()),
    };
    opened.map(File::from).map_err(io)
}

/// The folder `name` in `parent`, opened never through a link; made when
/// missing. Made, or `private`, it is `owner`'s own (when `root`) with
/// mode 0700, set through its descriptor.
fn folder_in(
    parent: &File,
    name: &str,
    owner: &Account,
    root: bool,
    private: bool,
) -> Result<File> {
    let made = match mkdirat(parent, name, Mode::S_IRWXU) {
        Ok(()) => true,
        Err(nix::errno::Errno::EEXIST) => false,
        Err(error) => return Err(io(error)),
    };
    let folder = opened(Some(parent), Path::new(name))?;
    if made || private {
        if root {
            fchown(&folder, Some(owner.uid.into()), Some(owner.gid.into())).map_err(io)?;
        }
        fchmod(&folder, Mode::S_IRWXU).map_err(io)?;
    }
    Ok(folder)
}

/// The secret at `file`: never through a link, never waiting on a pipe,
/// only a regular file of at most [`SECRET`] bytes; none when it is not there.
fn read_secret(file: &Path) -> Result<Option<Vec<u8>>> {
    let opened = match open(
        file,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(opened) => opened,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let seen = fstat(&opened).map_err(io)?;
    let regular = nix::sys::stat::SFlag::from_bits_truncate(seen.st_mode)
        & nix::sys::stat::SFlag::S_IFMT
        == nix::sys::stat::SFlag::S_IFREG;
    if !regular || u64::try_from(seen.st_size).unwrap_or(u64::MAX) > SECRET {
        return Err(refused("the pairing secret is not a small file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::from(opened)
        .take(SECRET + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    Ok(Some(bytes))
}

/// A launchd job's plist: `label` running `arguments`, kept alive; in the
/// logged-in session when `agent`, as an interactive Aqua job.
fn plist(label: &str, arguments: &[&str], agent: bool) -> String {
    let string = |value: &str| format!("<string>{}</string>", escape(value));
    let listed: String = arguments
        .iter()
        .map(|argument| format!("\t\t{}\n", string(argument)))
        .collect();
    let session = if agent {
        "\t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\t<key>ProcessType</key>\n\t<string>Interactive</string>\n"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t{label}\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{listed}\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<true/>\n\
         {session}</dict>\n</plist>\n",
        label = string(label),
    )
}

/// `text` as XML text.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
