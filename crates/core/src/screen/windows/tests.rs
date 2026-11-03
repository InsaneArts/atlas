use super::*;

#[test]
fn routing_compares_visibility_but_ignores_names_and_visible_alpha_changes() {
    let before = vec![placed(42, 0, None, 1.0), placed(43, 0, None, 1.0)];
    for change in 0..8 {
        let mut after = before.clone();
        match change {
            0 => after[0].alpha = 0.0,
            1 => after[0].alpha = 0.5,
            2 => after[0].title = Some("Renamed".into()),
            3 => after.reverse(),
            4 => after[0].bounds.x += 1.0,
            5 => after[0].pid += 1,
            6 => after[0].layer += 1,
            _ => {
                after.pop();
            }
        }
        assert_eq!(same_routing(&before, &after), matches!(change, 1 | 2));
        assert_eq!(same_routing(&after, &before), matches!(change, 1 | 2));
    }
}

#[test]
fn routing_compares_visible_occluders_and_ignores_invisible_windows() {
    let before = vec![placed(42, 0, None, 1.0), placed(43, 3, None, 1.0)];
    for (layer, alpha) in [
        (DOCK_LAYER, 1.0),
        (MENU_BAR_LAYER, 1.0),
        (MENU_LAYER, 1.0),
        (25, 1.0),
        (-1, 1.0),
        (0, 0.0),
        (3, 0.0),
    ] {
        let mut after = before.clone();
        after.insert(0, placed(99, layer, None, alpha));
        assert_eq!(same_routing(&before, &after), alpha == 0.0);
        assert_eq!(same_routing(&after, &before), alpha == 0.0);
        after.swap(0, 1);
        assert_eq!(same_routing(&before, &after), alpha == 0.0);
    }
}

#[test]
fn the_capture_indicator_a_screenshot_raises_leaves_routing_the_same() {
    let before = vec![placed(42, 0, None, 1.0)];
    let mut after = before.clone();
    let mut indicator = placed(99, 2147483630, Some("StatusIndicator"), 1.0);
    indicator.owner = Some("Window Server".into());
    after.insert(0, indicator);

    assert!(same_routing(&before, &after));
    assert!(same_routing(&after, &before));
}

/// A window of process 8 at `layer` with `title`, at one place.
fn placed(number: i64, layer: i64, title: Option<&str>, alpha: f64) -> Window {
    Window {
        number,
        pid: 8,
        owner: Some("TextEdit".into()),
        title: title.map(str::to_owned),
        layer,
        alpha,
        system: None,
        bounds: Bounds {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        },
    }
}

/// Where every window here is placed, as Accessibility reports it.
fn frame() -> Bounds {
    placed(0, 0, None, 1.0).bounds
}

#[test]
fn a_described_window_is_the_one_its_frame_kind_and_title_leave() {
    let document = placed(5, 0, Some("Letter.txt"), 1.0);
    let panel = placed(6, 3, Some("Fonts"), 1.0);
    let faint = placed(7, 3, None, 0.0);
    let listed = [panel.clone(), document.clone()];
    let floating = Some("AXFloatingWindow");
    let at = |listed: &[Window], title, subrole| described(listed, 8, &frame(), title, subrole);

    assert_eq!(at(&listed, None, floating), Listed::One(6));
    assert_eq!(
        at(&listed, None, None),
        Listed::Several,
        "a document window may be raised above the ordinary layer"
    );
    assert_eq!(at(&listed, Some("Fonts"), None), Listed::One(6));
    assert_eq!(
        at(&[faint, document.clone()], None, None),
        Listed::Several,
        "a faint window at the same place still counts"
    );
    assert_eq!(
        at(std::slice::from_ref(&document), None, floating),
        Listed::Missing
    );
    assert_eq!(at(&[document], None, None), Listed::One(5));
    assert_eq!(described(&listed, 9, &frame(), None, None), Listed::Missing);
}

#[test]
fn only_a_floating_subrole_rules_out_the_ordinary_layer() {
    let ordinary = [placed(5, 0, None, 1.0)];
    let raised = [placed(6, 3, None, 1.0)];
    let high = [placed(7, 101, None, 1.0)];
    let at = |listed: &[Window], subrole| described(listed, 8, &frame(), None, subrole);

    for subrole in [None, Some("AXStandardWindow"), Some("AXDialog")] {
        assert_eq!(at(&ordinary, subrole), Listed::One(5), "{subrole:?}");
        assert_eq!(at(&high, subrole), Listed::One(7), "{subrole:?}");
    }
    for subrole in [Some("AXFloatingWindow"), Some("AXSystemFloatingWindow")] {
        assert_eq!(at(&ordinary, subrole), Listed::Missing, "{subrole:?}");
        assert_eq!(at(&raised, subrole), Listed::One(6), "{subrole:?}");
    }
}

fn window(number: i64, x: f64, y: f64, width: f64, height: f64) -> Window {
    Window {
        number,
        pid: number,
        owner: None,
        title: None,
        layer: 0,
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

#[test]
fn bounds_intersect_only_with_area_in_common() {
    let square = window(1, 0.0, 0.0, 100.0, 100.0).bounds;
    let inside = window(2, 10.0, 10.0, 5.0, 5.0).bounds;
    let overlapping = window(3, 90.0, 90.0, 50.0, 50.0).bounds;
    let touching = window(4, 100.0, 0.0, 50.0, 100.0).bounds;
    let corner = window(5, 100.0, 100.0, 10.0, 10.0).bounds;
    let apart = window(6, 300.0, 300.0, 10.0, 10.0).bounds;
    let line = window(7, 50.0, 0.0, 0.0, 100.0).bounds;

    assert!(square.intersects(&inside) && inside.intersects(&square));
    assert!(square.intersects(&overlapping) && overlapping.intersects(&square));
    assert!(!square.intersects(&touching), "an edge in common");
    assert!(!square.intersects(&corner), "a corner in common");
    assert!(!square.intersects(&apart));
    assert!(!square.intersects(&line), "no area of its own");
}

fn display() -> Bounds {
    Bounds::display(1000.0, 800.0)
}

/// The test display with the Dock's icons along its bottom edge.
fn docked() -> Screen {
    Screen::new(display(), Dock::Icons(vec![dock_icons()]))
}

fn dock_icons() -> Bounds {
    Bounds {
        x: 200.0,
        y: 740.0,
        width: 600.0,
        height: 60.0,
    }
}

/// The test display with no Dock icons on it.
fn undocked() -> Screen {
    Screen::new(display(), Dock::Icons(Vec::new()))
}

#[test]
fn visible_points_avoid_windows_in_front() {
    let display = undocked();
    let target = window(2, 0.0, 0.0, 100.0, 100.0);
    let point = |windows: &[Window]| visible_point(windows, 2, &display);
    assert_eq!(point(std::slice::from_ref(&target)), Some((50.0, 50.0)));

    let cover = window(1, 0.0, 0.0, 80.0, 100.0);
    assert!(point(&[cover, target.clone()]).unwrap().0 >= 80.0);

    let behind = window(3, 0.0, 0.0, 100.0, 100.0);
    assert_eq!(point(&[target.clone(), behind]), Some((50.0, 50.0)));

    let mut overlay = window(1, 0.0, 0.0, 1000.0, 800.0);
    overlay.layer = 20;
    overlay.owner = Some("Dock".into());
    assert_eq!(
        point(&[overlay, target.clone()]),
        Some((50.0, 50.0)),
        "full-display overlays pass clicks through"
    );

    let mut unknown = window(1, 0.0, 0.0, 1000.0, 800.0);
    unknown.layer = 20;
    unknown.owner = Some("Other".into());
    assert_eq!(
        point(&[unknown, target.clone()]),
        None,
        "unidentified overlays fail closed"
    );

    let mut floating = window(1, -10.0, -10.0, 200.0, 200.0);
    floating.layer = 3;
    assert_eq!(
        point(&[floating, target.clone()]),
        None,
        "a floating panel over the target still occludes"
    );

    let mut menu = window(1, 0.0, 0.0, 100.0, 60.0);
    menu.layer = 24;
    assert!(point(&[menu, target.clone()]).unwrap().1 >= 60.0);

    // The tested point is the clicked point: rounding cannot move it under
    // an occluder (unrounded centre 50.5 would round into the cover).
    let odd = window(2, 0.0, 0.0, 101.0, 100.0);
    let right = window(1, 51.0, 0.0, 50.0, 100.0);
    let (x, _) = point(&[right, odd]).unwrap();
    assert!(x < 51.0, "{x}");

    assert_eq!(point(&[]), None);

    let menu_bar = {
        let mut menu = window(9, 0.0, 0.0, 1000.0, 30.0);
        menu.layer = 24;
        menu
    };
    let windows = [
        menu_bar,
        window(2, 0.0, 0.0, 100.0, 100.0),
        window(3, 0.0, 0.0, 9.0, 9.0),
    ];

    assert!(
        !unobstructed(&windows, 2, 10.0, 10.0, &display),
        "under the menu bar"
    );
    assert!(unobstructed(&windows, 2, 10.0, 40.0, &display));
    assert!(
        !unobstructed(&windows, 2, 150.0, 40.0, &display),
        "outside the window"
    );
    assert!(
        !unobstructed(&windows, 7, 10.0, 40.0, &display),
        "unknown window"
    );
    assert_eq!(frontmost_pid(&windows), Some(2));
    assert_eq!(frontmost_pid(&windows[..1]), None);
}

/// A named click's element under the Dock's icons or the shelf around
/// them is covered for every app, so no visible point is found there;
/// beside the shelf the Dock lets clicks through.
#[test]
fn the_docks_icons_cover_every_window_under_them() {
    let mut dock = window(1, 0.0, 0.0, 1000.0, 800.0);
    dock.layer = 20;
    dock.owner = Some("Dock".into());
    let under = window(2, 300.0, 740.0, 100.0, 60.0);
    let beside = window(3, 0.0, 700.0, 150.0, 100.0);
    let behind = window(4, 0.0, 0.0, 1000.0, 800.0);
    let windows = [dock, under, beside, behind];

    for (target, x, y) in [(2, 350.0, 770.0), (4, 600.0, 770.0), (4, 195.0, 770.0)] {
        assert!(
            !unobstructed(&windows, target, x, y, &docked()),
            "window {target} at {x},{y}"
        );
        assert!(
            unobstructed(&windows, target, x, y, &undocked()),
            "window {target} at {x},{y} with no icons"
        );
    }
    assert_eq!(top_at(&windows, 350.0, 770.0, &docked()).unwrap().number, 1);
    assert_eq!(visible_point(&windows, 2, &docked()), None);

    assert!(unobstructed(&windows, 3, 75.0, 750.0, &docked()));
    assert_eq!(visible_point(&windows, 3, &docked()), Some((75.0, 750.0)));
    assert!(unobstructed(&windows, 4, 500.0, 700.0, &docked()));
}

/// A Dock that shows more than its icons, or could not be read, covers
/// every window under it anywhere on the display.
#[test]
fn a_dock_that_is_not_only_its_icons_covers_the_whole_display() {
    let mut dock = window(1, 0.0, 0.0, 1000.0, 800.0);
    dock.layer = 20;
    dock.owner = Some("Dock".into());
    let mut menu = window(5, 100.0, 100.0, 100.0, 100.0);
    menu.layer = MENU_LAYER;
    let windows = [menu, dock, window(4, 0.0, 0.0, 1000.0, 800.0)];

    for dock in [Dock::Other, Dock::Unread] {
        let screen = Screen::new(display(), dock);
        for (x, y) in [(500.0, 300.0), (10.0, 10.0), (75.0, 750.0)] {
            assert!(
                !unobstructed(&windows, 4, x, y, &screen),
                "{screen:?} {x},{y}"
            );
            assert!(top_at(&windows, x, y, &screen).unwrap().is_dock());
        }
        assert_eq!(visible_point(&windows, 4, &screen), None);
        assert!(
            unobstructed(&windows, 5, 150.0, 150.0, &screen),
            "a menu in front of the Dock"
        );
    }
}

#[test]
fn a_reply_says_where_the_dock_takes_clicks_or_leaves_it_unread() {
    use serde_json::json;

    let icons = json!({"icons": [{"x": 200.0, "y": 740.0, "width": 600.0, "height": 60.0}]});
    for (said, dock) in [
        (icons, Dock::Icons(vec![dock_icons()])),
        (json!("other"), Dock::Other),
        (json!("unread"), Dock::Unread),
    ] {
        let reply = json!({"ok": true, "windows": [], "starts": {}, "dock": said});
        let shown = from_reply(&reply).unwrap();
        assert_eq!(shown.dock, dock);
        assert_eq!(serde_json::to_value(&dock).unwrap(), said);
    }

    let unasked = from_reply(&json!({"ok": true, "windows": [], "starts": {}})).unwrap();
    assert_eq!(unasked.dock, Dock::Unread, "not asked for");
}

#[test]
fn the_top_window_at_a_point_agrees_with_the_unobstructed_check() {
    let layered = |number, layer, owner: &str, bounds: (f64, f64, f64, f64)| {
        let mut window = window(number, bounds.0, bounds.1, bounds.2, bounds.3);
        window.layer = layer;
        window.owner = Some(owner.to_owned());
        window
    };
    let mut hidden = layered(8, 0, "Ghost", (0.0, 0.0, 1000.0, 800.0));
    hidden.alpha = 0.0;
    let mut overlay = layered(9, 20, "Notification Center", (0.0, 0.0, 1000.0, 800.0));
    overlay.system = Some(System::NotificationCenter);
    let fixtures = [
        vec![
            layered(1, MENU_LAYER, "TextEdit", (100.0, 24.0, 200.0, 300.0)),
            layered(2, MENU_BAR_LAYER, "Window Server", (0.0, 0.0, 1000.0, 24.0)),
            layered(3, 20, "Dock", (0.0, 0.0, 1000.0, 800.0)),
            layered(4, 3, "Notes", (700.0, 100.0, 200.0, 200.0)),
            layered(5, 0, "Calculator", (250.0, 200.0, 300.0, 300.0)),
            layered(6, 0, "TextEdit", (50.0, 50.0, 600.0, 500.0)),
            layered(7, 0, "Finder", (0.0, 0.0, 1000.0, 800.0)),
        ],
        vec![
            hidden,
            overlay,
            layered(10, 20, "Other", (0.0, 0.0, 500.0, 800.0)),
            layered(11, 0, "TextEdit", (100.0, 100.0, 800.0, 600.0)),
        ],
    ];

    let screens = [undocked(), docked(), Screen::new(display(), Dock::Unread)];

    // Each fixture window is its own process.
    for windows in &fixtures {
        for screen in &screens {
            let starts = std::collections::BTreeMap::new();
            let hits = hit_windows(windows, &display(), screen.dock(), &starts);
            for x in (0..1000).step_by(25) {
                for y in (0..800).step_by(25) {
                    let (x, y) = (f64::from(x), f64::from(y));
                    let top = top_at(windows, x, y, screen).map(|window| window.number);
                    assert_eq!(
                        hits.iter()
                            .find(|window| window.bounds.contains(x, y))
                            .map(|window| window.number),
                        top
                    );
                    for ordinary in windows.iter().filter(|window| window.layer == 0) {
                        let clear = unobstructed(windows, ordinary.number, x, y, screen);
                        let at = format!(
                            "window {} at {x},{y} with {:?}",
                            ordinary.number, screen.dock
                        );
                        assert_eq!(reaches(windows, ordinary.pid, x, y, screen), clear, "{at}");
                        if ordinary.alpha > 0.0 {
                            assert_eq!(top == Some(ordinary.number), clear, "{at}");
                        }
                    }
                }
            }
        }
    }
    assert!(
        reaches(&fixtures[1], 8, 300.0, 300.0, &undocked()),
        "an ordinary window that shows nothing is reached where it is clear"
    );

    let first = &fixtures[0];
    assert_eq!(top_at(first, 150.0, 100.0, &undocked()).unwrap().number, 1);
    assert!(
        top_at(first, 10.0, 10.0, &undocked())
            .unwrap()
            .is_menu_bar()
    );
    assert_eq!(top_at(first, 750.0, 150.0, &undocked()).unwrap().number, 4);
    assert_eq!(
        top_at(&fixtures[1], 300.0, 300.0, &undocked())
            .unwrap()
            .number,
        10
    );
    assert_eq!(top_at(&[], 1.0, 1.0, &undocked()), None);

    // On the Dock's tiles the Dock is on top and no app is reached;
    // beside them the app under the Dock is.
    assert_eq!(top_at(first, 300.0, 775.0, &docked()).unwrap().number, 3);
    assert_eq!(top_at(first, 300.0, 775.0, &undocked()).unwrap().number, 7);
    for pid in [5, 6, 7] {
        assert!(!reaches(first, pid, 300.0, 775.0, &docked()), "{pid}");
    }
    assert!(reaches(first, 7, 100.0, 775.0, &docked()));
}

#[test]
fn a_window_only_named_as_notification_centers_covers_what_is_under_it() {
    let mut claimed = window(1, 0.0, 0.0, 1000.0, 800.0);
    claimed.layer = 20;
    claimed.owner = Some("Notification Center".into());
    let app = window(2, 100.0, 100.0, 400.0, 300.0);
    let listed = [claimed, app];

    assert!(!unobstructed(&listed, 2, 200.0, 200.0, &undocked()));
    assert_eq!(
        top_at(&listed, 200.0, 200.0, &undocked()).map(|window| window.number),
        Some(1)
    );
    assert_eq!(
        ground_at(&listed, 700.0, 600.0, &undocked()),
        Ground::Unread(&listed[0]),
        "what it shows is not known"
    );
}

/// A window of Notification Center's, as the helper proved it, at `layer`.
fn notification_center(number: i64, layer: i64, bounds: (f64, f64, f64, f64)) -> Window {
    let mut window = window(number, bounds.0, bounds.1, bounds.2, bounds.3);
    window.pid = 31082;
    window.owner = Some("Notification Center".into());
    window.layer = layer;
    window.system = Some(System::NotificationCenter);
    window
}

#[test]
fn banner_and_widget_regions_block_points_while_proven_empty_regions_pass_through() {
    let banner = notification_center(1, 23, (640.0, 40.0, 350.0, 80.0));
    let overlay = notification_center(2, 20, (0.0, 0.0, 1000.0, 800.0));
    let app = window(3, 300.0, 300.0, 400.0, 300.0);
    let month = notification_center(4, WIDGET_LAYER, (8.0, 38.0, 180.0, 180.0));
    let featured = notification_center(5, WIDGET_LAYER, (8.0, 218.0, 360.0, 180.0));
    let listed = [banner, overlay, app, month, featured];
    let screen = undocked();
    let number = |ground: Ground<'_>| match ground {
        Ground::Covered(window) => Some(window.number),
        Ground::Clear => None,
        other => panic!("{other:?}"),
    };

    assert_eq!(listed[0].surface(&display()), Some(Surface::Notification));
    assert_eq!(listed[1].surface(&display()), Some(Surface::Overlay));
    assert_eq!(listed[2].surface(&display()), None);
    assert_eq!(listed[3].surface(&display()), Some(Surface::Widget));

    for (x, y, what, expected) in [
        (700.0, 60.0, "a banner", Some(1)),
        (400.0, 400.0, "an app's window under the overlay", Some(3)),
        (70.0, 70.0, "a widget", Some(4)),
        (300.0, 300.0, "the app's window over a widget", Some(3)),
        (100.0, 300.0, "a widget below another", Some(5)),
        (800.0, 700.0, "the empty desktop under the overlay", None),
        (190.0, 100.0, "beside a widget", None),
    ] {
        assert_eq!(
            number(ground_at(&listed, x, y, &screen)),
            expected,
            "{what}"
        );
        assert_eq!(
            top_at(&listed, x, y, &screen).map(|window| window.number),
            expected,
            "{what}"
        );
    }
    assert!(unobstructed(&listed, 3, 400.0, 400.0, &screen));
    assert_eq!(ground_at(&listed, 1000.0, 400.0, &screen), Ground::Outside);
    assert_eq!(ground_at(&listed, -1.0, 400.0, &screen), Ground::Outside);

    let mut faint = window(6, 750.0, 650.0, 100.0, 100.0);
    faint.alpha = 0.0;
    let with_faint = [faint];
    assert_eq!(
        number(ground_at(&with_faint, 800.0, 700.0, &screen)),
        Some(6),
        "a window that shows nothing is still not the desktop"
    );

    let mut dock = window(7, 0.0, 0.0, 1000.0, 800.0);
    dock.layer = 20;
    dock.owner = Some("Dock".into());
    let docks = [dock];
    assert_eq!(number(ground_at(&docks, 300.0, 770.0, &docked())), Some(7));
    assert_eq!(ground_at(&docks, 300.0, 600.0, &docked()), Ground::Clear);
    assert_eq!(
        ground_at(&docks, 300.0, 600.0, &Screen::new(display(), Dock::Unread)),
        Ground::Unread(&docks[0])
    );
}

#[test]
fn a_window_says_which_system_drew_it_only_when_one_did() {
    let mut proven = window(1, 0.0, 0.0, 10.0, 10.0);
    proven.system = Some(System::NotificationCenter);
    let ordinary = window(2, 0.0, 0.0, 10.0, 10.0);

    assert_eq!(serde_json::json!(proven)["system"], "notificationCenter");
    assert!(serde_json::json!(ordinary).get("system").is_none());
    let mut wire = serde_json::json!(proven);
    wire.as_object_mut().unwrap().remove("system");
    assert_eq!(
        serde_json::from_value::<Window>(wire).unwrap().system,
        None,
        "a list from before the field says no system drew it"
    );
}
