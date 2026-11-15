//! The guest's own system, as the setup asks it: signatures, launchd, the
//! directory service and its account tools, each bounded in time.
use super::{Account, System};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

/// How long one tool may take before it counts as failed.
const ASKING: Duration = Duration::from_secs(120);

/// The guest's own system.
pub(super) struct Real;

/// Run `command`: whether it succeeded in time.
fn succeeded(command: &mut Command) -> bool {
    atlas_core::bounded::output(command, ASKING).is_ok_and(|output| output.status.success())
}

/// Run `command` with `secret` on its standard input, never as an argument:
/// whether it succeeded.
fn given(command: &mut Command, secret: &str) -> bool {
    let Ok(mut child) = atlas_core::spawn::spawn(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    ) else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut input| writeln!(input, "{secret}").is_ok());
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return written && status.success(),
            Ok(None) if started.elapsed() < ASKING => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

impl System for Real {
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>> {
        crate::machine::installer::verified_bundle(bundle)
    }

    fn launchctl(&self, arguments: &[&str]) -> bool {
        succeeded(Command::new("/bin/launchctl").args(arguments))
    }

    fn account(&self, name: &str) -> Option<Account> {
        let user = nix::unistd::User::from_name(name).ok().flatten()?;
        // The directory service gives the full name as the record's gecos.
        let full_name = user
            .gecos
            .to_str()
            .ok()
            .filter(|full| !full.is_empty())
            .map(str::to_owned);
        Some(Account {
            uid: user.uid.as_raw(),
            gid: user.gid.as_raw(),
            home: user.dir,
            full_name,
        })
    }

    fn create_admin(&self, name: &str, full_name: &str, password: &str) -> bool {
        // `-password -` has the tool ask for it, on its standard input.
        given(
            Command::new("/usr/sbin/sysadminctl").args([
                "-addUser",
                name,
                "-fullName",
                full_name,
                "-password",
                "-",
                "-admin",
            ]),
            password,
        )
    }

    fn set_password(&self, name: &str, password: &str) -> bool {
        given(
            Command::new("/usr/sbin/sysadminctl").args([
                "-resetPasswordFor",
                name,
                "-newPassword",
                "-",
            ]),
            password,
        )
    }

    fn is_admin(&self, name: &str) -> Option<bool> {
        atlas_core::account::administrator(name)
    }

    fn set_admin(&self, name: &str, admin: bool) -> bool {
        let edit = if admin { "-a" } else { "-d" };
        succeeded(
            Command::new("/usr/sbin/dseditgroup")
                .args(["-o", "edit", edit, name, "-t", "user", "admin"]),
        )
    }

    fn hide(&self, name: &str) -> bool {
        succeeded(Command::new("/usr/bin/dscl").args([
            ".",
            "create",
            &format!("/Users/{name}"),
            "IsHidden",
            "1",
        ]))
    }

    fn close_remote_login(&self) -> bool {
        // Apart from this process and the session that asked, so that
        // session hears this answer before Remote Login goes.
        atlas_core::spawn::spawn(
            Command::new("/bin/sh")
            .args([
                "-c",
                "sleep 1; /bin/launchctl disable system/com.openssh.sshd; /bin/launchctl bootout system/com.openssh.sshd",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .is_ok()
    }

    fn root(&self) -> bool {
        nix::unistd::geteuid().is_root()
    }
}
