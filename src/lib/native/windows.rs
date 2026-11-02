#[cfg(test)]
pub(crate) use crate::screen::windows::MENU_LAYER;
pub(crate) use crate::screen::windows::{Bounds, Dock, Window};
use crate::{
    error::{Code, Error, Result},
    native::cf,
};
use objc2_core_graphics::CGWindowListOption;

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
        bounds: Bounds {
            x: side("X")?,
            y: side("Y")?,
            width: side("Width")?,
            height: side("Height")?,
        },
    })
}

/// On-screen windows in front-to-back order.
pub(crate) fn windows() -> Result<Vec<Window>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = cf::windows(options)
        .ok_or_else(|| Error::new(Code::Windows, "The window list is unavailable"))?;

    Ok(list.iter().filter_map(|entry| read_window(entry)).collect())
}

#[cfg(test)]
mod tests;
