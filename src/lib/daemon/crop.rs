//! What part of the display a cropped screenshot shows: an app's windows
//! with a small margin, or a region cut to the display.
use crate::{
    daemon::protocol::Region,
    native::windows::{Bounds, Dock, MENU_BAR_LAYER, Window},
};

/// Points of the display shown around an app's windows, so their edges and
/// focus rings are in the picture.
const PADDING: f64 = 8.0;

/// The part of the display showing one app's windows.
#[derive(Debug, Clone, PartialEq)]
pub struct AppCrop {
    pub region: Region,
    /// The owner of the app's frontmost window.
    pub name: Option<String>,
    /// Other apps' windows in front of the app's, inside the crop.
    pub overlapping: Vec<Window>,
}

fn bounds(region: &Region) -> Bounds {
    Bounds {
        x: f64::from(region.x),
        y: f64::from(region.y),
        width: f64::from(region.width),
        height: f64::from(region.height),
    }
}

/// The part of `bounds` inside `area`, empty when they do not meet.
fn clipped(bounds: &Bounds, area: &Bounds) -> Bounds {
    let left = bounds.x.max(area.x);
    let top = bounds.y.max(area.y);
    let right = (bounds.x + bounds.width).min(area.x + area.width);
    let bottom = (bounds.y + bounds.height).min(area.y + area.height);

    Bounds {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

/// `region` cut to the display; None when nothing of it is on the display.
pub fn clamp(region: Region, display: &Region) -> Option<Region> {
    let left = region.x.max(display.x);
    let top = region.y.max(display.y);
    let right = (region.x + region.width).min(display.x + display.width);
    let bottom = (region.y + region.height).min(display.y + display.height);

    (right > left && bottom > top).then_some(Region {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// The crop showing every on-screen window of `pid`, and the windows of
/// others in front of them inside it, the parts of the Dock that take
/// clicks among them; None when it has no window on screen. `windows` is
/// front to back, in display points.
pub fn app(windows: &[Window], pid: i64, display: &Region, dock: &Dock) -> Option<AppCrop> {
    let screen = bounds(display);
    let strips: Vec<&Bounds> = windows
        .iter()
        .filter(|window| window.layer == MENU_BAR_LAYER)
        .map(|window| &window.bounds)
        .collect();
    // Status items and the menu bar's own titles would stretch a crop across
    // the whole top of the display. Only the part on the display counts: the
    // recording indicator reaches past its edge.
    let in_menu_bar = |window: &Window| {
        let shown = clipped(&window.bounds, &screen);
        strips.iter().any(|strip| strip.covers(&shown))
    };

    let included: Vec<usize> = windows
        .iter()
        .enumerate()
        .filter(|(_, window)| {
            window.pid == pid
                && window.alpha > 0.0
                && window.bounds.width >= 1.0
                && window.bounds.height >= 1.0
                && !in_menu_bar(window)
                && window.bounds.intersects(&screen)
        })
        .map(|(index, _)| index)
        .collect();
    let (&front, &back) = (included.first()?, included.last()?);

    let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
    let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for bounds in included.iter().map(|&index| &windows[index].bounds) {
        left = left.min(bounds.x);
        top = top.min(bounds.y);
        right = right.max(bounds.x + bounds.width);
        bottom = bottom.max(bounds.y + bounds.height);
    }

    let left = (left - PADDING).max(screen.x).floor();
    let top = (top - PADDING).max(screen.y).floor();
    let right = (right + PADDING).min(screen.x + screen.width).ceil();
    let bottom = (bottom + PADDING).min(screen.y + screen.height).ceil();
    if right - left < 1.0 || bottom - top < 1.0 {
        return None;
    }

    let region = Region {
        x: left as i32,
        y: top as i32,
        width: (right - left) as i32,
        height: (bottom - top) as i32,
    };
    let area = bounds(&region);
    let overlay = |window: &Window| window.passes_through(&screen);
    let in_front = || {
        windows[..back]
            .iter()
            .filter(|window| window.pid != pid && window.alpha > 0.0)
    };
    let mut overlapping: Vec<Window> = in_front()
        .filter(|window| {
            window.layer != MENU_BAR_LAYER
                && !in_menu_bar(window)
                && !overlay(window)
                && window.bounds.intersects(&area)
        })
        .cloned()
        .collect();
    // The Dock's full-display window lets clicks through but for where it
    // takes them: those parts of it are in front of the app.
    for window in in_front().filter(|window| overlay(window) && window.is_dock()) {
        for part in dock.areas(&window.bounds) {
            if part.intersects(&area) {
                overlapping.push(Window {
                    bounds: part,
                    ..window.clone()
                });
            }
        }
    }

    Some(AppCrop {
        region,
        name: windows[front].owner.clone(),
        overlapping,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Region = Region {
        x: 0,
        y: 0,
        width: 1000,
        height: 800,
    };

    /// A Dock showing no icons, which takes no clicks.
    const NO_DOCK: Dock = Dock::Icons(Vec::new());

    /// The parts of the Dock that take clicks are in front of an app's
    /// windows under them; one that could not be read is in front all over.
    #[test]
    fn the_dock_is_overlapping_where_it_takes_clicks() {
        let mut dock = window(3, 20, 0.0, 0.0, 1000.0, 800.0);
        dock.owner = Some("Dock".into());
        let windows = [dock, window(7, 0, 100.0, 600.0, 400.0, 180.0)];
        let icons = |x| {
            Dock::Icons(vec![Bounds {
                x,
                y: 740.0,
                width: 300.0,
                height: 60.0,
            }])
        };

        let over = app(&windows, 7, &DISPLAY, &icons(200.0)).unwrap();
        let parts: Vec<_> = over
            .overlapping
            .iter()
            .map(|window| &window.bounds)
            .collect();
        assert_eq!(over.overlapping.len(), 1);
        assert_eq!(over.overlapping[0].owner.as_deref(), Some("Dock"));
        assert!(parts[0].covers(&Bounds {
            x: 200.0,
            y: 740.0,
            width: 300.0,
            height: 60.0
        }));

        let aside = app(&windows, 7, &DISPLAY, &icons(650.0)).unwrap();
        assert!(aside.overlapping.is_empty(), "{:?}", aside.overlapping);

        for unknown in [Dock::Other, Dock::Unread] {
            let all = app(&windows, 7, &DISPLAY, &unknown).unwrap();
            assert_eq!(all.overlapping.len(), 1, "{unknown:?}");
            assert_eq!(all.overlapping[0].bounds, windows[0].bounds);
        }
    }

    fn window(pid: i64, layer: i64, x: f64, y: f64, width: f64, height: f64) -> Window {
        Window {
            number: pid * 100 + layer,
            pid,
            owner: Some(format!("App {pid}")),
            title: None,
            layer,
            alpha: 1.0,
            bounds: Bounds {
                x,
                y,
                width,
                height,
            },
        }
    }

    fn region(x: i32, y: i32, width: i32, height: i32) -> Region {
        Region {
            x,
            y,
            width,
            height,
        }
    }

    fn menu_bar() -> Window {
        let mut bar = window(1, MENU_BAR_LAYER, 0.0, 0.0, 1000.0, 24.0);
        bar.owner = Some("Window Server".into());
        bar
    }

    #[test]
    fn an_apps_crop_is_its_windows_with_a_margin() {
        let windows = [
            menu_bar(),
            window(7, 0, 100.0, 100.0, 400.0, 300.0),
            window(7, 0, 300.0, 200.0, 400.0, 300.0),
            window(8, 0, 0.0, 0.0, 1000.0, 800.0),
        ];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();

        assert_eq!(crop.region, region(92, 92, 616, 416));
        assert_eq!(crop.name.as_deref(), Some("App 7"));
        assert!(crop.overlapping.is_empty(), "the window behind is not");
    }

    #[test]
    fn sheets_panels_and_open_menus_belong_to_the_apps_crop() {
        for (layer, x, y) in [(0, 600.0, 120.0), (3, 50.0, 400.0), (8, 600.0, 600.0)] {
            let windows = [
                window(7, layer, x, y, 100.0, 100.0),
                window(7, 0, 200.0, 200.0, 200.0, 200.0),
            ];
            let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();
            let shown = bounds(&crop.region);

            assert!(shown.covers(&windows[0].bounds), "layer {layer}");
            assert!(shown.covers(&windows[1].bounds), "layer {layer}");
        }

        let windows = [
            menu_bar(),
            window(7, 101, 210.0, 24.0, 180.0, 400.0),
            window(7, 0, 200.0, 200.0, 200.0, 100.0),
        ];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();
        assert_eq!(
            crop.region,
            region(192, 16, 216, 416),
            "a menu below the bar"
        );
    }

    #[test]
    fn a_crop_never_leaves_the_display() {
        let windows = [window(7, 0, -200.0, 700.0, 500.0, 300.0)];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();

        assert_eq!(crop.region, region(0, 692, 308, 108));
    }

    #[test]
    fn fractional_bounds_widen_to_whole_points() {
        let windows = [window(7, 0, 100.5, 100.25, 200.25, 100.5)];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();

        assert_eq!(crop.region, region(92, 92, 217, 117));
    }

    #[test]
    fn status_items_in_the_menu_bar_stay_out_of_the_crop() {
        let windows = [
            window(7, 25, 900.0, 0.0, 30.0, 24.0),
            menu_bar(),
            window(7, 0, 100.0, 100.0, 200.0, 200.0),
        ];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();

        assert_eq!(crop.region, region(92, 92, 216, 216));
        assert_eq!(
            app(&windows[..2], 7, &DISPLAY, &NO_DOCK),
            None,
            "only a status item"
        );
    }

    #[test]
    fn invisible_and_off_display_windows_stay_out() {
        let mut hidden = window(7, 0, 600.0, 600.0, 100.0, 100.0);
        hidden.alpha = 0.0;
        let windows = [
            hidden,
            window(7, 0, 1200.0, 100.0, 300.0, 300.0),
            window(7, 0, 500.0, 500.0, 0.5, 40.0),
            window(7, 0, 100.0, 100.0, 200.0, 200.0),
        ];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();

        assert_eq!(crop.region, region(92, 92, 216, 216));
    }

    #[test]
    fn an_app_with_no_window_on_screen_has_no_crop() {
        let off = window(7, 0, 1200.0, 900.0, 300.0, 300.0);
        let others = window(8, 0, 100.0, 100.0, 300.0, 300.0);

        assert_eq!(app(&[], 7, &DISPLAY, &NO_DOCK), None);
        assert_eq!(app(&[off, others.clone()], 7, &DISPLAY, &NO_DOCK), None);
        assert_eq!(app(&[menu_bar(), others], 7, &DISPLAY, &NO_DOCK), None);
    }

    #[test]
    fn windows_of_others_in_front_are_reported_as_overlapping() {
        let mut dock = window(3, 20, 0.0, 0.0, 1000.0, 800.0);
        dock.owner = Some("Dock".into());
        let mut center = window(4, 23, 0.0, 0.0, 1000.0, 800.0);
        center.owner = Some("Notification Center".into());
        let mut alert = window(5, 8, 250.0, 250.0, 200.0, 120.0);
        alert.owner = Some("System Events".into());
        let mut gone = window(9, 0, 120.0, 120.0, 50.0, 50.0);
        gone.alpha = 0.0;

        let mut indicator = window(2, 2147483630, 990.0, 1.0, 28.0, 22.0);
        indicator.owner = Some("Window Server".into());

        let windows = [
            indicator,
            menu_bar(),
            dock,
            center,
            window(6, 0, 700.0, 600.0, 100.0, 100.0),
            alert,
            gone,
            window(7, 0, 100.0, 100.0, 400.0, 300.0),
            window(7, 0, 950.0, 30.0, 50.0, 50.0),
            window(8, 0, 150.0, 150.0, 100.0, 100.0),
        ];
        let crop = app(&windows, 7, &DISPLAY, &NO_DOCK).unwrap();
        let owners: Vec<_> = crop
            .overlapping
            .iter()
            .map(|window| window.owner.as_deref().unwrap())
            .collect();

        assert_eq!(owners, ["System Events"]);
    }

    #[test]
    fn a_region_is_cut_to_the_display_or_refused_outside_it() {
        assert_eq!(
            clamp(region(100, 100, 400, 300), &DISPLAY),
            Some(region(100, 100, 400, 300))
        );
        assert_eq!(
            clamp(region(-50, -50, 200, 200), &DISPLAY),
            Some(region(0, 0, 150, 150))
        );
        assert_eq!(
            clamp(region(900, 700, 400, 300), &DISPLAY),
            Some(region(900, 700, 100, 100))
        );
        assert_eq!(clamp(region(5000, 5000, 10, 10), &DISPLAY), None);
        assert_eq!(clamp(region(1000, 0, 10, 10), &DISPLAY), None, "an edge");
    }
}
