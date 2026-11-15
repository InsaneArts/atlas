//! The launchd agent that keeps a VM's runner, Atlas VM, alive in this
//! account's own session: one per VM, its plist this account's own, judged
//! by what launchd loaded and never repaired while its runner may serve.
use super::{System, Vms, sync, try_lock};
use atlas_core::{
    desktop::DesktopId,
    error::{Error, Result},
    launchd::{Job, Loaded},
};
use serde_json::{Value, json};
use std::path::Path;

/// The seconds launchd gives a runner to end after TERM before KILL.
const EXIT_TIMEOUT: u64 = 30;

/// Write and start the agent of the VM of `desktop`, or say how it runs.
///
/// # Errors
///
/// Fails when the plist cannot be written or launchd cannot be asked.
pub fn install_agent(desktop: &DesktopId) -> Result<Value> {
    install_with(&Vms::own()?, &super::Real, desktop)
}

fn install_with(vms: &Vms, system: &impl System, desktop: &DesktopId) -> Result<Value> {
    let Some(_station) = try_lock(&vms.station_lock(desktop))? else {
        return Ok(pending("Another station operation holds this VM."));
    };
    // A marker that cannot be read is never taken for absent.
    match std::fs::symlink_metadata(vms.removal(desktop)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Ok(json!({"ok": false, "error": "This VM is being removed."})),
        Err(error) => return Err(Error::writing(&vms.removal(desktop), error)),
    }
    let program = system.program()?;
    let plist = vms.agent_plist(desktop);
    let bytes = plist_of(&Vms::label(desktop), &program, desktop);
    if std::fs::read(&plist).ok().as_deref() != Some(bytes.as_bytes()) {
        std::fs::create_dir_all(vms.agents())
            .map_err(|error| Error::writing(&vms.agents(), error))?;
        atlas_core::files::write_atomic(&plist, bytes.as_bytes(), 0o644)?;
    }
    // Written through every time, so a try cut short after its rename is
    // finished before launchd is asked.
    sync(&vms.agents())?;
    let (domain, label) = (vms.domain(), Vms::label(desktop));
    if system.job(&domain, &label)? == Job::Absent {
        let path = plist.to_string_lossy().into_owned();
        // Its outcome is read back, never taken from launchctl's word.
        let _ = system.launchctl(&["bootstrap", &domain, &path])?;
    }
    match system.job(&domain, &label)? {
        Job::Absent => Ok(pending("launchd has not loaded the VM's job.")),
        Job::Loaded(loaded) => match mismatch(&loaded, &plist, &program, desktop) {
            None => Ok(json!({"ok": true, "agent": "loaded", "pid": loaded.pid})),
            // Reloading would end its runner, and with it its VM.
            Some(why) => Ok(pending(&format!(
                "The VM's job is loaded otherwise ({why}); it is left as it is."
            ))),
        },
    }
}

/// Why `loaded` is not the job written for `desktop`, running `program`
/// from `plist`; none when it is.
fn mismatch(
    loaded: &Loaded,
    plist: &Path,
    program: &Path,
    desktop: &DesktopId,
) -> Option<&'static str> {
    let program = program.to_string_lossy();
    let arguments = [program.as_ref(), "serve", desktop.as_str()];
    if loaded.path.as_deref() != Some(plist) {
        return Some("another plist");
    }
    if loaded.program.as_deref() != Some(program.as_ref()) {
        return Some("another program");
    }
    if loaded
        .arguments
        .as_deref()
        .is_none_or(|loaded| loaded.iter().map(String::as_str).ne(arguments))
    {
        return Some("other arguments");
    }
    if !loaded
        .properties
        .as_ref()
        .is_some_and(|properties| properties.iter().any(|property| property == "keepalive"))
    {
        return Some("not kept alive");
    }
    if loaded.exit_timeout != Some(EXIT_TIMEOUT) {
        return Some("another exit timeout");
    }
    None
}

/// The plist of the agent `label`, running `program` for `desktop`.
fn plist_of(label: &str, program: &Path, desktop: &DesktopId) -> String {
    let string = |value: &str| {
        format!(
            "<string>{}</string>",
            value
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t{label}\n\
         \t<key>ProgramArguments</key>\n\t<array>\n\t\t{program}\n\t\t<string>serve</string>\n\t\t{desktop}\n\t</array>\n\
         \t<key>KeepAlive</key>\n\t<true/>\n\
         \t<key>ExitTimeOut</key>\n\t<integer>{EXIT_TIMEOUT}</integer>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         \t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
         </dict>\n</plist>\n",
        label = string(label),
        program = string(&program.to_string_lossy()),
        desktop = string(desktop.as_str()),
    )
}

fn pending(why: &str) -> Value {
    json!({"ok": true, "agent": "pending", "why": why})
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
