//! Plain aligned columns: a dim header, two spaces between columns, no
//! borders. Widths are measured as the terminal draws them, so styled and
//! wide characters still line up.
use super::style;
use console::measure_text_width;

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// A header row over one row per entry.
pub(crate) fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
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
#[path = "table_tests.rs"]
mod tests;
