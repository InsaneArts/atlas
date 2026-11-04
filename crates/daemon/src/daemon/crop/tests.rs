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

    let over = app(&windows, 7, None, &DISPLAY, &icons(200.0)).unwrap();
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

    let aside = app(&windows, 7, None, &DISPLAY, &icons(650.0)).unwrap();
    assert!(aside.overlapping.is_empty(), "{:?}", aside.overlapping);

    for unknown in [Dock::Other, Dock::Unread] {
        let all = app(&windows, 7, None, &DISPLAY, &unknown).unwrap();
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
        system: None,
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
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();

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
        let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();
        let shown = bounds(&crop.region);

        assert!(shown.covers(&windows[0].bounds), "layer {layer}");
        assert!(shown.covers(&windows[1].bounds), "layer {layer}");
    }

    let windows = [
        menu_bar(),
        window(7, 101, 210.0, 24.0, 180.0, 400.0),
        window(7, 0, 200.0, 200.0, 200.0, 100.0),
    ];
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(
        crop.region,
        region(192, 16, 216, 416),
        "a menu below the bar"
    );
}

#[test]
fn a_crop_never_leaves_the_display() {
    let windows = [window(7, 0, -200.0, 700.0, 500.0, 300.0)];
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();

    assert_eq!(crop.region, region(0, 692, 308, 108));
}

#[test]
fn fractional_bounds_widen_to_whole_points() {
    let windows = [window(7, 0, 100.5, 100.25, 200.25, 100.5)];
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();

    assert_eq!(crop.region, region(92, 92, 217, 117));
}

#[test]
fn status_items_in_the_menu_bar_stay_out_of_the_crop() {
    let windows = [
        window(7, 25, 900.0, 0.0, 30.0, 24.0),
        menu_bar(),
        window(7, 0, 100.0, 100.0, 200.0, 200.0),
    ];
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();

    assert_eq!(crop.region, region(92, 92, 216, 216));
    assert_eq!(
        app(&windows[..2], 7, None, &DISPLAY, &NO_DOCK),
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
    let crop = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();

    assert_eq!(crop.region, region(92, 92, 216, 216));
}

#[test]
fn an_app_with_no_window_on_screen_has_no_crop() {
    let off = window(7, 0, 1200.0, 900.0, 300.0, 300.0);
    let others = window(8, 0, 100.0, 100.0, 300.0, 300.0);

    assert_eq!(app(&[], 7, None, &DISPLAY, &NO_DOCK), None);
    assert_eq!(
        app(&[off, others.clone()], 7, None, &DISPLAY, &NO_DOCK),
        None
    );
    assert_eq!(
        app(&[menu_bar(), others], 7, None, &DISPLAY, &NO_DOCK),
        None
    );
}

#[test]
fn windows_of_others_in_front_are_reported_as_overlapping() {
    let mut dock = window(3, 20, 0.0, 0.0, 1000.0, 800.0);
    dock.owner = Some("Dock".into());
    let mut center = window(4, 23, 0.0, 0.0, 1000.0, 800.0);
    center.owner = Some("Notification Center".into());
    center.system = Some(atlas_core::screen::windows::System::NotificationCenter);
    let mut alert = window(5, 8, 250.0, 250.0, 200.0, 120.0);
    alert.owner = Some("System Events".into());
    let mut gone = window(9, 0, 120.0, 120.0, 50.0, 50.0);
    gone.alpha = 0.0;

    let mut indicator = window(2, 2147483630, 990.0, 1.0, 28.0, 22.0);
    indicator.owner = Some("Window Server".into());

    let mut windows = [
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
    let owners = |windows: &[Window]| -> Vec<String> {
        app(windows, 7, None, &DISPLAY, &NO_DOCK)
            .unwrap()
            .overlapping
            .iter()
            .map(|window| window.owner.clone().unwrap())
            .collect()
    };

    assert_eq!(owners(&windows), ["System Events"]);

    windows[3].system = None;
    assert_eq!(
        owners(&windows),
        ["Notification Center", "System Events"],
        "a window only named as Notification Center's is in front like any other"
    );
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

/// Two windows of app 7 apart, and app 5's window in front over the second.
fn apart() -> Vec<Window> {
    let mut first = window(7, 0, 100.0, 100.0, 200.0, 150.0);
    first.number = 71;
    let mut second = window(7, 0, 600.0, 100.0, 200.0, 150.0);
    second.number = 72;
    vec![window(5, 3, 650.0, 120.0, 50.0, 50.0), first, second]
}

#[test]
fn one_window_of_an_app_is_cropped_alone_with_what_covers_it() {
    let windows = apart();
    let first = app(&windows, 7, Some(Part::Window(71)), &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(
        first.region,
        region(92, 92, 216, 166),
        "its own frame and margin"
    );
    assert!(first.overlapping.is_empty());

    let second = app(&windows, 7, Some(Part::Window(72)), &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(second.region, region(592, 92, 216, 166));
    assert_eq!(second.overlapping.len(), 1, "app 5's window covers it");

    assert_eq!(
        app(&windows, 7, Some(Part::Window(99)), &DISPLAY, &NO_DOCK),
        None
    );
    assert_eq!(
        app(&windows, 5, Some(Part::Window(71)), &DISPLAY, &NO_DOCK),
        None,
        "another app's window number is not the app's"
    );
}

#[test]
fn an_apps_part_of_a_region_is_where_its_crop_lies_inside_it() {
    let windows = apart();
    let within = |x, y, width, height| Some(Part::Within(region(x, y, width, height)));
    let left = app(&windows, 7, within(0, 0, 400, 800), &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(
        left.region,
        region(92, 92, 216, 166),
        "only the window inside it"
    );
    assert!(left.overlapping.is_empty());

    let part = app(&windows, 7, within(150, 150, 100, 50), &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(part.region, region(150, 150, 100, 50), "cut to the region");

    assert_eq!(
        app(&windows, 7, within(350, 300, 100, 100), &DISPLAY, &NO_DOCK),
        None,
        "a region between its windows shows none of them"
    );
}

#[test]
fn a_window_asked_for_alone_names_the_apps_own_windows_in_front_of_it() {
    let mut front = window(7, 0, 150.0, 150.0, 100.0, 100.0);
    front.number = 73;
    let mut asked = window(7, 0, 100.0, 100.0, 200.0, 150.0);
    asked.number = 71;
    let windows = [front, asked];
    let alone = app(&windows, 7, Some(Part::Window(71)), &DISPLAY, &NO_DOCK).unwrap();
    assert_eq!(alone.overlapping.len(), 1);
    assert_eq!(alone.overlapping[0].number, 73);
    let whole = app(&windows, 7, None, &DISPLAY, &NO_DOCK).unwrap();
    assert!(
        whole.overlapping.is_empty(),
        "the whole app's own windows cover nothing of it"
    );
}
