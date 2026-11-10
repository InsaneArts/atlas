use super::*;

#[test]
fn helper_names_cannot_drive_the_terminal() {
    assert_eq!(printable("Untitled"), "Untitled");
    assert_eq!(printable("a\u{1b}[31mb"), "a\\u{1b}[31mb");
    assert_eq!(printable("x\u{202e}y\nz"), "x\\u{202e}y\\nz");
}

#[test]
fn values_are_cut_to_fit_with_an_ellipsis() {
    assert_eq!(clip("short", 10), "short");
    assert_eq!(clip("a long window title", 8), "a long…");
    assert!(clip("日本語のタイトル", 7).width() <= 7);
    assert_eq!(clip("abc", 0), "…");
    assert_eq!(clip("abc", 1), "…");
}

#[test]
fn plain_words_for_numbers() {
    assert_eq!(count(1, "window"), "1 window");
    assert_eq!(count(2, "window"), "2 windows");
    assert_eq!(dimensions(2560.0, 1440.0), "2560 × 1440");
    assert_eq!(number(1.5), "1.5");
    assert_eq!(duration(12.3), "12 ms");
    assert_eq!(duration(1234.0), "1.2 s");
    assert_eq!(duration(125_000.0), "2m 05s");
    assert_eq!(duration(59_960.0), "1m 00s", "never 60.0 s");
}
