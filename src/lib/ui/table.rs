//! Plain aligned columns: a dim header, two spaces between columns, no
//! borders. Widths are measured as the terminal draws them, so styled and
//! wide characters still line up.
use super::style;
use console::measure_text_width;

/// A header row over one row per entry.
pub fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| measure_text_width(h)).collect();

    for row in rows {
        for (column, cell) in row.iter().enumerate() {
            if column >= widths.len() {
                widths.push(0);
            }
            widths[column] = widths[column].max(measure_text_width(cell));
        }
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

    let header: Vec<String> = headers.iter().map(style::dim).collect();
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
}
