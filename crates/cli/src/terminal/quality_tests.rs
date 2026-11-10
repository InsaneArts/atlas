use super::*;
#[test]
fn automatic_quality_retains_retina_detail_and_excludes_unused_letterbox_space() {
    assert_eq!(dimension((120, 42, 960, 672), (1600.0, 1000.0)), 1920);
    assert_eq!(dimension((100, 100, 800, 1600), (1600.0, 1000.0)), 1664);
    assert_eq!(dimension((200, 60, 2560, 1440), (1600.0, 1000.0)), 4096);
}
#[test]
fn small_or_temporarily_unreported_panes_have_bounded_capture_sizes() {
    assert_eq!(dimension((20, 8, 100, 80), (1600.0, 1000.0)), 320);
    assert_eq!(dimension((0, 0, 0, 0), (1600.0, 1000.0)), 2560);
    assert_eq!(dimension((100, 40, 1000, 600), (f64::NAN, 1000.0)), 2560);
    assert_eq!(
        dimension((u16::MAX, u16::MAX, u16::MAX, u16::MAX), (1.0, 1.0)),
        4096
    );
}
#[test]
fn explicit_quality_does_not_depend_on_terminal_detection() {
    assert_eq!(current(Some(640), (1600.0, 1000.0)), 640);
    assert_eq!(current(Some(4096), (1600.0, 1000.0)), 4096);
}
