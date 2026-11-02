use crate::native::cf;
use atlas_core::error::{Code, Error, Result};
pub(crate) use atlas_core::screen::windows::{Bounds, Window};
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
///
/// # Errors
///
/// Returns an error when the window list is unavailable.
pub fn windows() -> Result<Vec<Window>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let list = cf::windows(options)
        .ok_or_else(|| Error::new(Code::Windows, "The window list is unavailable"))?;

    Ok(list.iter().filter_map(|entry| read_window(entry)).collect())
}

#[cfg(test)]
mod tests;
