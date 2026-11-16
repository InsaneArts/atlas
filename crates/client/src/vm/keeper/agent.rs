//! The launchd agent that keeps a VM's session keeper alive in this
//! account's own session, beside the VM's runner's: one per VM, judged by
//! what launchd loaded and never reloaded while it matches, since that would
//! end the session it holds.
use super::super::{System, Vms, sync, try_lock};
use atlas_core::{
    desktop::{DesktopId, RecordKey},
    error::{Error, Result},
    launchd::{Job, Loaded},
};
use serde_json::{Value, json};
use std::path::Path;

/// The seconds launchd gives a keeper to end after TERM before KILL.
const EXIT_TIMEOUT: u64 = 30;

/// Write and start the keeper's agent of the VM `vm`, its keeper reading
/// the settings at `settings`, or say how it runs.
///
/// # Errors
///
/// Fails when Atlas's program cannot be found, the plist cannot be written
/// or launchd cannot be asked.
pub fn install_keeper(vm: &DesktopId, settings: &Path) -> Result<Value> {
    let program =
        atlas_core::app_bundle::installed_app()?.join(atlas_core::app_bundle::BUNDLE_EXECUTABLE);
    let settings =
        std::path::absolute(settings).map_err(|error| Error::writing(settings, error))?;
    install_with(&Vms::own()?, &super::super::Real, vm, &program, &settings)
}

fn install_with(
    vms: &Vms,
    system: &impl System,
    vm: &DesktopId,
    program: &Path,
    settings: &Path,
) -> Result<Value> {
    let Some(_station) = try_lock(&vms.station_lock(vm))? else {
        return Ok(pending("Another station operation holds this VM."));
    };
    // A marker that cannot be read is never taken for absent.
    match std::fs::symlink_metadata(vms.removal(vm)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Ok(json!({"ok": false, "error": "This VM is being removed."})),
        Err(error) => return Err(Error::writing(&vms.removal(vm), error)),
    }
    let label = Vms::keeper_label(vm);
    let plist = vms.agents().join(format!("{label}.plist"));
    let arguments = arguments(program, vm, settings);
    let bytes = plist_of(&label, &arguments);
    if std::fs::read(&plist).ok().as_deref() != Some(bytes.as_bytes()) {
        std::fs::create_dir_all(vms.agents())
            .map_err(|error| Error::writing(&vms.agents(), error))?;
        atlas_core::files::write_atomic(&plist, bytes.as_bytes(), 0o644)?;
    }
    // Written through every time, so a try cut short after its rename is
    // finished before launchd is asked.
    sync(&vms.agents())?;
    let domain = vms.domain();
    if system.job(&domain, &label)? == Job::Absent {
        let path = plist.to_string_lossy().into_owned();
        // Its outcome is read back, never taken from launchctl's word.
        let _ = system.launchctl(&["bootstrap", &domain, &path])?;
    }
    match system.job(&domain, &label)? {
        Job::Absent => Ok(pending("launchd has not loaded the keeper's job.")),
        Job::Loaded(loaded) => match mismatch(&loaded, &plist, &arguments) {
            None => Ok(json!({"ok": true, "keeper": "loaded", "pid": loaded.pid})),
            // Reloading would end the session the keeper holds.
            Some(why) => Ok(pending(&format!(
                "The keeper's job is loaded otherwise ({why}); it is left as it is."
            ))),
        },
    }
}

/// What the keeper of `vm` runs: Atlas's `program`, with its settings.
fn arguments(program: &Path, vm: &DesktopId, settings: &Path) -> Vec<String> {
    vec![
        program.to_string_lossy().into_owned(),
        "station".to_owned(),
        "keep".to_owned(),
        "--key".to_owned(),
        RecordKey::vm(vm).as_str().to_owned(),
        "--settings".to_owned(),
        settings.to_string_lossy().into_owned(),
    ]
}

/// Why `loaded` is not the job written at `plist` to run `arguments`; none
/// when it is.
fn mismatch(loaded: &Loaded, plist: &Path, arguments: &[String]) -> Option<&'static str> {
    if loaded.path.as_deref() != Some(plist) {
        return Some("another plist");
    }
    if loaded.program.as_deref() != arguments.first().map(String::as_str) {
        return Some("another program");
    }
    if loaded.arguments.as_deref() != Some(arguments) {
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

/// The plist of the agent `label`, running `arguments`.
fn plist_of(label: &str, arguments: &[String]) -> String {
    let string = |value: &str| {
        format!(
            "<string>{}</string>",
            value
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    };
    let arguments: String = arguments
        .iter()
        .map(|argument| format!("\t\t{}\n", string(argument)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t{label}\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{arguments}\t</array>\n\
         \t<key>KeepAlive</key>\n\t<true/>\n\
         \t<key>ExitTimeOut</key>\n\t<integer>{EXIT_TIMEOUT}</integer>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         \t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
         </dict>\n</plist>\n",
        label = string(label),
    )
}

fn pending(why: &str) -> Value {
    json!({"ok": true, "keeper": "pending", "why": why})
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
