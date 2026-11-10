//! Capture enough detail for Retina terminals without capturing an entire
//! large display for a narrow pane. Explicit limits remain deterministic.
use super::graphics::Layout;

pub(super) fn current(limit: Option<u32>, display: (f64, f64)) -> u32 {
    limit.unwrap_or_else(|| {
        crossterm::terminal::window_size()
            .ok()
            .map(|size| dimension((size.columns, size.rows, size.width, size.height), display))
            .unwrap_or(2560)
    })
}

fn dimension(terminal: (u16, u16, u16, u16), display: (f64, f64)) -> u32 {
    let (columns, rows, width, height) = terminal;
    if !display.0.is_finite() || !display.1.is_finite() || display.0 <= 0.0 || display.1 <= 0.0 {
        return 2560;
    }
    let Some(layout) = Layout::fit(
        columns,
        rows,
        (width, height),
        (display.0.ceil() as u32, display.1.ceil() as u32),
    ) else {
        return 2560;
    };
    let fitted_width = u32::from(layout.columns) * u32::from(width) / u32::from(columns);
    let fitted_height = u32::from(layout.rows) * u32::from(height) / u32::from(rows);
    // Terminals may report logical pixels on a Retina screen. Retain two
    // source samples per reported pixel; the terminal performs the final fit.
    // Quantization avoids reopening a stream for every tiny resize step.
    (fitted_width.max(fitted_height) * 2)
        .div_ceil(128)
        .saturating_mul(128)
        .clamp(320, 4096)
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
