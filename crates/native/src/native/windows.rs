use crate::native::cf;
pub(crate) use atlas_core::screen::windows::{Bounds, Window};
use atlas_core::{
    app_bundle::signature::executable_of,
    error::{Code, Error, Result},
    process::{self, Info},
    screen::windows::System,
};
use objc2_core_graphics::CGWindowListOption;
use std::{collections::BTreeMap, path::Path};

/// The program Notification Center runs, on the system's sealed volume.
const NOTIFICATION_CENTER: &str =
    "/System/Library/CoreServices/NotificationCenter.app/Contents/MacOS/NotificationCenter";

/// One window-list entry, if it carries the fields every window has.
fn read_window(entry: &cf::Record) -> Option<Window> {
    // The bounds dictionary holds X, Y, Width and Height as numbers.
    let bounds = cf::nested(entry, "kCGWindowBounds")?;
    let side = |key| cf::float(&bounds, key);

    Some(Window {
        number: cf::integer(entry, "kCGWindowNumber")?,
        pid: cf::integer(entry, "kCGWindowOwnerPID")?,
        owner: cf::text(entry, "kCGWindowOwnerName"),
        title: cf::text(entry, "kCGWindowName"),
        layer: cf::integer(entry, "kCGWindowLayer")?,
        alpha: cf::float(entry, "kCGWindowAlpha").unwrap_or(1.0),
        system: None,
        bounds: Bounds {
            x: side("X")?,
            y: side("Y")?,
            width: side("Width")?,
            height: side("Height")?,
        },
    })
}

/// On-screen windows in front-to-back order, each a system process is
/// proven to draw marked as that system's.
///
/// # Errors
///
/// Returns an error when the window list is unavailable.
pub fn windows() -> Result<Vec<Window>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = cf::windows(options)
        .ok_or_else(|| Error::new(Code::Windows, "The window list is unavailable"))?;

    let mut windows: Vec<Window> = list.iter().filter_map(|entry| read_window(entry)).collect();
    attest(&mut windows, system_of);
    Ok(windows)
}

/// Mark every window with the system its process is, asking once per
/// process.
fn attest(windows: &mut [Window], mut system: impl FnMut(i64) -> Option<System>) {
    let mut known = BTreeMap::new();
    for window in windows {
        window.system = *known
            .entry(window.pid)
            .or_insert_with(|| system(window.pid));
    }
}

/// The system process `pid` is, when it is one.
fn system_of(pid: i64) -> Option<System> {
    system_with(
        pid,
        nix::unistd::geteuid().as_raw(),
        |pid| process::info(pid).ok().flatten(),
        |pid| executable_of(pid.unsigned_abs()),
    )
}

/// The system process `pid` is: one of account `uid`'s own processes,
/// whose real and effective user are both `uid`, running the system's
/// program for it, and the same process before and after its program was
/// read. Its window's owner name plays no part.
fn system_with(
    pid: i64,
    uid: u32,
    describe: impl Fn(i32) -> Option<Info>,
    program: impl FnOnce(i32) -> Option<std::path::PathBuf>,
) -> Option<System> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let own = |info: &Info| {
        info.instance.pid == pid && info.uid == uid && info.ruid == uid && !info.zombie
    };
    let before = describe(pid).filter(own)?;
    let path = program(pid)?;
    let after = describe(pid).filter(own)?;
    (after.instance == before.instance && path == Path::new(NOTIFICATION_CENTER))
        .then_some(System::NotificationCenter)
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
