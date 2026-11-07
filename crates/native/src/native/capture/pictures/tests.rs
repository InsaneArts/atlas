use super::*;

#[test]
fn every_core_graphics_picture_is_counted_with_why_the_stream_did_not_give_it() {
    let counts = Counts::default();
    assert_eq!(
        counts.snapshot(),
        json!({"streamed": 0, "coreGraphics": 0, "lastCoreGraphics": "none"})
    );

    counts.captured(Captured::Stream);
    counts.captured(Captured::CoreGraphics(Unserved::Closed));
    counts.captured(Captured::Stream);
    counts.captured(Captured::Area);
    assert_eq!(
        counts.snapshot(),
        json!({"streamed": 2, "coreGraphics": 2, "lastCoreGraphics": "area"})
    );

    counts.captured(Captured::CoreGraphics(Unserved::Smaller));
    assert_eq!(counts.snapshot()["lastCoreGraphics"], "smaller");
    counts.captured(Captured::CoreGraphics(Unserved::Unproven));
    assert_eq!(counts.snapshot()["lastCoreGraphics"], "unproven");
    counts.captured(Captured::CoreGraphics(Unserved::Elsewhere));
    assert_eq!(counts.snapshot()["lastCoreGraphics"], "other-display");
    assert_eq!(counts.snapshot()["coreGraphics"], 5);
}
