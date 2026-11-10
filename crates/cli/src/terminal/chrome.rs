//! The viewer's own controls. Geometry is shared by drawing and hit testing;
//! none of these cells belongs to the helper's display.
use super::{Command, ControlPhase, viewport::Change};
use crossterm::{
    cursor::MoveTo,
    event::{MouseButton, MouseEvent, MouseEventKind},
    queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
};
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

const BACKGROUND: Color = Color::Rgb {
    r: 24,
    g: 27,
    b: 30,
};
const SURFACE: Color = Color::Rgb {
    r: 31,
    g: 35,
    b: 39,
};
const TEXT: Color = Color::Rgb {
    r: 233,
    g: 238,
    b: 240,
};
const MUTED: Color = Color::Rgb {
    r: 151,
    g: 163,
    b: 171,
};
const ACCENT: Color = Color::Rgb {
    r: 133,
    g: 222,
    b: 201,
};
const ACCENT_SURFACE: Color = Color::Rgb {
    r: 36,
    g: 68,
    b: 63,
};
const WARNING: Color = Color::Rgb {
    r: 234,
    g: 191,
    b: 125,
};
const HOVER: Color = Color::Rgb {
    r: 57,
    g: 64,
    b: 69,
};
const PRESSED: Color = Color::Rgb {
    r: 76,
    g: 92,
    b: 92,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Take,
    GiveBack,
    Stop,
    Quit,
    Fit,
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    Diagnostics,
}

impl Action {
    pub(super) fn command(self, frame: Option<&str>) -> Option<Command> {
        match self {
            Self::Take => frame.map(|key| Command::Take(key.into())),
            Self::GiveBack => Some(Command::GiveBack),
            Self::Stop => Some(Command::Stop),
            Self::Quit => Some(Command::Quit),
            Self::Fit => Some(Command::View(Change::Fit)),
            Self::ZoomIn => Some(Command::View(Change::ZoomIn)),
            Self::ZoomOut => Some(Command::View(Change::ZoomOut)),
            Self::PanLeft => Some(Command::View(Change::PanLeft)),
            Self::PanRight => Some(Command::View(Change::PanRight)),
            Self::PanUp => Some(Command::View(Change::PanUp)),
            Self::PanDown => Some(Command::View(Change::PanDown)),
            Self::Diagnostics => Some(Command::Diagnostics),
        }
    }
}

pub(super) struct Information<'a> {
    pub(super) controlling: bool,
    pub(super) phase: ControlPhase,
    pub(super) paused: bool,
    pub(super) read_only: bool,
    pub(super) available: bool,
    pub(super) message: &'a str,
    pub(super) picture: Option<(&'a str, (u32, u32))>,
    pub(super) view_label: &'a str,
    pub(super) detail_limited: bool,
    pub(super) diagnostics: Option<&'a str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Watching,
    Controlling,
    Taking,
    Releasing,
    Waiting,
    Attention,
    ReadOnly,
    Uncertain,
    Paused,
}

impl Phase {
    fn of(info: &Information<'_>) -> Self {
        if info.phase == ControlPhase::Taking {
            Self::Taking
        } else if info.phase == ControlPhase::Releasing {
            Self::Releasing
        } else if info.phase == ControlPhase::Uncertain {
            Self::Uncertain
        } else if !info.available {
            Self::Waiting
        } else if info.paused && info.controlling {
            Self::Paused
        } else if info.controlling {
            Self::Controlling
        } else if info.read_only {
            Self::ReadOnly
        } else if info.message.starts_with("Watching") || info.message.is_empty() {
            Self::Watching
        } else {
            Self::Attention
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Watching => "● Watching",
            Self::Controlling => "● You have control",
            Self::Taking => "◌ Taking control",
            Self::Releasing => "◌ Giving back control",
            Self::Waiting => "◌ Connecting",
            Self::Attention => "● Watching",
            Self::ReadOnly => "● Read only",
            Self::Uncertain => "● Recovery needed",
            Self::Paused => "◌ Input paused",
        }
    }
    fn compact_label(self) -> &'static str {
        match self {
            Self::Controlling => "● In control",
            Self::Releasing => "◌ Giving back",
            Self::Uncertain => "● Recovery",
            _ => self.label(),
        }
    }
    fn color(self) -> Color {
        match self {
            Self::Controlling => ACCENT,
            Self::Attention | Self::Uncertain | Self::Paused => WARNING,
            _ => MUTED,
        }
    }
}

#[derive(Clone)]
struct Span {
    column: u16,
    text: String,
    foreground: Color,
    background: Color,
    bold: bool,
}
#[derive(Clone)]
struct Button {
    start: u16,
    end: u16,
    action: Action,
}

#[derive(Clone, Default)]
pub(super) struct Footer {
    width: u16,
    first_row: u16,
    controls_row: u16,
    rows: [Vec<Span>; 2],
    buttons: Vec<Button>,
}

impl Footer {
    pub(super) fn new(columns: u16, rows: u16, info: Information<'_>, pointer: &Pointer) -> Self {
        let width = columns.saturating_sub(1);
        let mut footer = Self {
            width,
            first_row: rows.saturating_sub(2),
            controls_row: rows.saturating_sub(1),
            ..Self::default()
        };
        if width == 0 || rows < 2 {
            return footer;
        }
        let phase = Phase::of(&info);
        let (state_column, state_label) = if usize::from(width) > 9 + text_width(phase.label()) {
            footer.span(0, 1, "Atlas", TEXT, BACKGROUND, true);
            (9, phase.label())
        } else {
            // Ownership remains legible before branding when a split is narrow.
            (1, phase.compact_label())
        };
        footer.span(
            0,
            state_column,
            state_label,
            phase.color(),
            BACKGROUND,
            false,
        );
        let view_label = fit(info.view_label, 24);
        let detail = info
            .picture
            .map(|(kind, (w, h))| format!("{view_label} · {kind} · {w} × {h}"));
        let mut details = Vec::new();
        if info.detail_limited {
            if let Some(detail) = detail {
                details.push(format!("Detail limited · {detail}"));
            }
            details.push(format!("Detail limited · {view_label}"));
            details.push("Detail limited".into());
            details.push(view_label);
        } else {
            if let Some(detail) = detail {
                details.push(detail);
            }
            details.push(view_label);
        }
        let left = usize::from(state_column) + text_width(state_label) + 3;
        if let Some(detail) = details
            .into_iter()
            .find(|detail| usize::from(width) > left + text_width(detail))
        {
            let column = width.saturating_sub(text_width(&detail) as u16 + 1);
            footer.span(
                0,
                column,
                &detail,
                if info.detail_limited { WARNING } else { MUTED },
                BACKGROUND,
                false,
            );
        }
        let compact = width < 66;
        let mut actions = vec![(
            Action::Quit,
            if info.controlling {
                if compact {
                    " Quit Ctrl+Q "
                } else {
                    " Quit  Ctrl+Q "
                }
            } else {
                " Quit  Q "
            },
        )];
        if !info.read_only
            && info.available
            && (info.controlling || !info.paused)
            && !matches!(phase, Phase::Taking | Phase::Releasing | Phase::Uncertain)
        {
            if width >= 160 {
                actions.push((Action::Stop, " Stop agents "));
            }
            actions.push(if info.controlling {
                (
                    Action::GiveBack,
                    if compact {
                        " Back Ctrl+G "
                    } else {
                        " Give back  Ctrl+G "
                    },
                )
            } else {
                (
                    Action::Take,
                    if compact {
                        " Take  T "
                    } else {
                        " Take control  T "
                    },
                )
            });
        }
        let changing = matches!(phase, Phase::Taking | Phase::Releasing | Phase::Uncertain);
        // View controls are secondary to ownership and quit. Add complete
        // groups and leave a useful status area rather than filling every cell.
        let mut secondary = Vec::new();
        if !changing {
            secondary.push(vec![(Action::Diagnostics, " D ")]);
            if info.available && !info.paused {
                secondary.push(vec![
                    (Action::ZoomIn, " + "),
                    (Action::Fit, " Fit "),
                    (Action::ZoomOut, " − "),
                ]);
                if info.view_label != "Fit" && width >= 140 {
                    secondary.push(vec![
                        (Action::PanRight, " → "),
                        (Action::PanDown, " ↓ "),
                        (Action::PanUp, " ↑ "),
                        (Action::PanLeft, " ← "),
                    ]);
                }
            }
        }
        for group in secondary {
            let used = actions
                .iter()
                .map(|(_, label)| text_width(label) + 1)
                .sum::<usize>();
            let extra = group
                .iter()
                .map(|(_, label)| text_width(label) + 1)
                .sum::<usize>();
            if usize::from(width) >= used + extra + 28 {
                actions.extend(group);
            }
        }
        let required = actions
            .iter()
            .map(|(_, label)| text_width(label) + 1)
            .sum::<usize>()
            + 1;
        if required > usize::from(width)
            && actions
                .iter()
                .any(|(action, _)| matches!(action, Action::Take | Action::GiveBack))
        {
            // Give Back stays visible in narrow controlling panes. Quit is
            // still available from its universal keyboard shortcut.
            actions.retain(|(action, _)| matches!(action, Action::Take | Action::GiveBack));
        }
        // Place from the right, preserving the primary action as space shrinks.
        let mut cursor = width.saturating_sub(1);
        for (action, label) in actions {
            let length = text_width(label) as u16;
            if length > cursor {
                continue;
            }
            let start = cursor - length;
            let primary = matches!(action, Action::Take | Action::GiveBack);
            let selected = (action == Action::Fit && info.view_label == "Fit")
                || (action == Action::Diagnostics && info.diagnostics.is_some());
            let background = if pointer.pressed == Some(action) {
                PRESSED
            } else if pointer.hover == Some(action) || selected {
                HOVER
            } else if primary {
                ACCENT_SURFACE
            } else {
                SURFACE
            };
            footer.span(
                1,
                start,
                label,
                if primary {
                    ACCENT
                } else if selected {
                    TEXT
                } else {
                    MUTED
                },
                background,
                primary,
            );
            footer.buttons.push(Button {
                start,
                end: cursor,
                action,
            });
            cursor = start.saturating_sub(1);
        }
        let message = match phase {
            Phase::Watching => info
                .diagnostics
                .unwrap_or("Hidden desktop · ready when you are"),
            Phase::Controlling => info.diagnostics.unwrap_or("Keyboard and mouse connected"),
            Phase::ReadOnly => info.diagnostics.unwrap_or("Observation only"),
            Phase::Paused => "Input paused · click this pane to continue",
            Phase::Waiting | Phase::Attention if info.diagnostics.is_some() => {
                info.diagnostics.unwrap_or(info.message)
            }
            _ => info.message,
        };
        if cursor > 3 {
            footer.span(
                1,
                1,
                &fit(message, usize::from(cursor.saturating_sub(2))),
                if matches!(phase, Phase::Attention | Phase::Uncertain) {
                    WARNING
                } else {
                    MUTED
                },
                SURFACE,
                false,
            );
        }
        footer
    }

    fn span(
        &mut self,
        row: usize,
        column: u16,
        text: &str,
        foreground: Color,
        background: Color,
        bold: bool,
    ) {
        let text = fit(text, usize::from(self.width.saturating_sub(column)));
        if !text.is_empty() {
            self.rows[row].push(Span {
                column,
                text,
                foreground,
                background,
                bold,
            });
        }
    }

    pub(super) fn draw(&self, out: &mut impl Write) -> io::Result<()> {
        for (index, row) in [self.first_row, self.controls_row].into_iter().enumerate() {
            let background = if index == 0 { BACKGROUND } else { SURFACE };
            queue!(
                out,
                MoveTo(0, row),
                SetAttribute(Attribute::Reset),
                SetBackgroundColor(background),
                Print(" ".repeat(usize::from(self.width)))
            )?;
            for span in &self.rows[index] {
                queue!(
                    out,
                    MoveTo(span.column, row),
                    SetAttribute(Attribute::Reset),
                    SetForegroundColor(span.foreground),
                    SetBackgroundColor(span.background)
                )?;
                if span.bold {
                    queue!(out, SetAttribute(Attribute::Bold))?;
                }
                queue!(out, Print(&span.text))?;
            }
        }
        queue!(out, SetAttribute(Attribute::Reset), ResetColor)
    }

    fn hit(&self, column: u16, row: u16) -> Option<Action> {
        if row != self.controls_row {
            return None;
        }
        self.buttons
            .iter()
            .find(|button| (button.start..button.end).contains(&column))
            .map(|button| button.action)
    }

    fn contains(&self, row: u16) -> bool {
        self.width > 0 && (row == self.first_row || row == self.controls_row)
    }
}

fn text_width(text: &str) -> usize {
    text.chars().filter_map(UnicodeWidthChar::width).sum()
}
fn fit(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .filter(|ch| !ch.is_control())
        .filter_map(|ch| {
            let count = UnicodeWidthChar::width(ch)?;
            (count > 0).then_some((ch, count))
        })
        .take_while(|(_, count)| {
            used += count;
            used <= width
        })
        .map(|(ch, _)| ch)
        .collect()
}

/// A press begun on the footer belongs to the footer until release, even if
/// taking control finishes while the mouse button is still down.
#[derive(Default)]
pub(super) struct Pointer {
    footer_press: Option<MouseButton>,
    hover: Option<Action>,
    pressed: Option<Action>,
}

pub(super) struct PointerEvent {
    pub(super) action: Option<Action>,
    pub(super) consumed: bool,
    pub(super) changed: bool,
}

impl Pointer {
    pub(super) fn handle(&mut self, footer: &Footer, event: MouseEvent) -> PointerEvent {
        let previous = (self.hover, self.pressed);
        self.hover = footer.hit(event.column, event.row);
        let inside = footer.contains(event.row);
        let mut action = None;
        let consumed = if let Some(button) = self.footer_press {
            if event.kind == MouseEventKind::Up(button) {
                self.footer_press = None;
                self.pressed = None;
            }
            true
        } else {
            match event.kind {
                MouseEventKind::Down(button) if inside => {
                    self.footer_press = Some(button);
                    if button == MouseButton::Left {
                        self.pressed = self.hover;
                        action = self.hover;
                    }
                    true
                }
                MouseEventKind::Up(_) => false,
                _ => inside,
            }
        };
        PointerEvent {
            action,
            consumed,
            changed: previous != (self.hover, self.pressed),
        }
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
