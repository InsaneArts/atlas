use super::*;

#[test]
fn columns_line_up_under_their_headers() {
    console::set_colors_enabled(false);

    let rows = vec![
        vec!["TextEdit".to_owned(), "Untitled".to_owned()],
        vec!["Safari".to_owned(), "Docs".to_owned()],
    ];

    assert_eq!(
        render(&["APP", "TITLE"], &rows),
        "APP       TITLE\nTextEdit  Untitled\nSafari    Docs"
    );
}

#[test]
fn cells_keep_their_styles_and_widths_beyond_twenty_four_columns() {
    console::set_colors_enabled(false);

    let name = "abcdefghijklmnopqrstuvwxyz012345";
    let rows = vec![vec![format!("\x1b[32m{name}\x1b[0m"), "1".to_owned()]];
    let header = "H".repeat(32);

    assert_eq!(
        render(&[&header, "PID"], &rows),
        format!("{header}  PID\n\x1b[32m{name}\x1b[0m  1")
    );
}

#[test]
fn tables_that_exceed_the_output_budget_are_rejected_before_padding() {
    console::set_colors_enabled(false);

    let rows = vec![vec!["x".repeat(24)]; 1024 * 1024 / 24];

    assert_eq!(
        render(&["APP"], &rows),
        "Table is too large to display; --json lists all entries."
    );
}
