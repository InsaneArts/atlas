//! Plain aligned columns: a dim header, two spaces between columns, no
//! borders. Widths are measured as the terminal draws them, so styled and
//! wide characters still line up.
use super::style;
use console::measure_text_width;

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// A header row over one row per entry.
pub fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
    let header: Vec<String> = headers.iter().map(style::dim).collect();
    let mut widths: Vec<usize> = header.iter().map(|h| measure_text_width(h)).collect();

    for row in rows {
        for (column, cell) in row.iter().enumerate() {
            if column >= widths.len() {
                widths.push(0);
            }
            widths[column] = widths[column].max(measure_text_width(cell));
        }
    }

    let bytes = std::iter::once(&header)
        .chain(rows)
        .try_fold(0usize, |total, cells| {
            let total =
                cells
                    .iter()
                    .enumerate()
                    .fold(total.saturating_add(1), |total, (column, cell)| {
                        let pad = widths[column].saturating_sub(measure_text_width(cell));
                        total
                            .saturating_add(cell.len())
                            .saturating_add(pad)
                            .saturating_add(2)
                    });
            (total <= MAX_OUTPUT_BYTES).then_some(total)
        });
    if bytes.is_none() {
        return "Table is too large to display; --json lists all entries.".to_owned();
    }

    let line = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let pad = widths[column].saturating_sub(measure_text_width(cell));
                format!("{cell}{}", " ".repeat(pad))
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_owned()
    };

    let mut lines = vec![line(&header)];
    lines.extend(rows.iter().map(|row| line(row)));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
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
}
