use super::*;
use crossterm::event::KeyModifiers;

fn info(controlling: bool) -> Information<'static> {
    Information {
        controlling,
        phase: if controlling {
            ControlPhase::Controlling
        } else {
            ControlPhase::Watching
        },
        paused: false,
        read_only: false,
        available: true,
        message: if controlling {
            "You have control"
        } else {
            "Watching the hidden helper"
        },
        picture: Some(("RGB", (2560, 1600))),
        view_label: "Fit",
        detail_limited: false,
        diagnostics: None,
    }
}
fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}
fn button(footer: &Footer, action: Action) -> &Button {
    footer
        .buttons
        .iter()
        .find(|button| button.action == action)
        .unwrap()
}

#[test]
fn drawing_and_hit_testing_share_the_same_fitted_button_cells() {
    for columns in 2..180 {
        let footer = Footer::new(columns, 24, info(false), &Pointer::default());
        for row in &footer.rows {
            for span in row {
                assert!(
                    usize::from(span.column) + text_width(&span.text) <= usize::from(columns - 1)
                );
            }
        }
        for button in &footer.buttons {
            assert!(button.end < columns);
            for column in button.start..button.end {
                assert_eq!(footer.hit(column, 23), Some(button.action));
            }
            assert_eq!(footer.hit(button.start, 22), None);
            assert_eq!(footer.hit(button.start, 21), None);
        }
    }
}

#[test]
fn footer_text_never_injects_escape_sequences_or_splits_a_wide_character() {
    assert_eq!(fit("A界B", 3), "A界");
    assert_eq!(fit("A界B", 2), "A");
    let text = fit("bad\x1b[31m\n\r\t\u{202e}界", 100);
    assert!(!text.chars().any(char::is_control));
    assert!(!text.contains('\u{202e}'));
    let mut state = info(false);
    state.message = "Failed \x1b[2J\n界";
    let footer = Footer::new(100, 24, state, &Pointer::default());
    for row in footer.rows {
        for span in row {
            assert!(!span.text.contains('\x1b'));
        }
    }
}

#[test]
fn controls_reflect_read_only_and_pending_ownership_instead_of_offering_ignored_actions() {
    let mut state = info(false);
    state.read_only = true;
    let readonly = Footer::new(100, 24, state, &Pointer::default());
    assert!(readonly.buttons.iter().all(|button| !matches!(
        button.action,
        Action::Take | Action::GiveBack | Action::Stop
    )));
    for phase in [
        ControlPhase::Taking,
        ControlPhase::Releasing,
        ControlPhase::Uncertain,
    ] {
        let mut state = info(false);
        state.phase = phase;
        let pending = Footer::new(100, 24, state, &Pointer::default());
        assert!(
            pending
                .buttons
                .iter()
                .all(|button| button.action == Action::Quit)
        );
    }
    let controlling = Footer::new(100, 24, info(true), &Pointer::default());
    assert!(
        controlling
            .buttons
            .iter()
            .any(|button| button.action == Action::GiveBack)
    );
    assert!(
        controlling
            .buttons
            .iter()
            .all(|button| button.action != Action::Take)
    );
}

#[test]
fn giving_back_control_keeps_its_visible_button_in_narrow_panes() {
    for columns in 16..30 {
        let footer = Footer::new(columns, 24, info(true), &Pointer::default());
        assert!(
            footer
                .buttons
                .iter()
                .any(|button| button.action == Action::GiveBack)
        );
    }
}

#[test]
fn taking_control_consumes_the_entire_footer_press_even_after_control_arrives() {
    let watching = Footer::new(100, 24, info(false), &Pointer::default());
    let take = button(&watching, Action::Take).start;
    let mut pointer = Pointer::default();
    let down = pointer.handle(
        &watching,
        mouse(MouseEventKind::Down(MouseButton::Left), take, 23),
    );
    assert_eq!(down.action, Some(Action::Take));
    assert!(down.consumed);
    assert_eq!(
        down.action.unwrap().command(Some("displayed-frame")),
        Some(Command::Take("displayed-frame".into()))
    );
    let controlling = Footer::new(100, 24, info(true), &pointer);
    let drag = pointer.handle(
        &controlling,
        mouse(MouseEventKind::Drag(MouseButton::Left), 4, 4),
    );
    assert!(drag.consumed);
    assert!(drag.action.is_none());
    let up = pointer.handle(
        &controlling,
        mouse(MouseEventKind::Up(MouseButton::Left), 4, 4),
    );
    assert!(up.consumed);
    assert!(up.action.is_none());
    assert!(
        !pointer
            .handle(&controlling, mouse(MouseEventKind::Moved, 4, 4))
            .consumed
    );
}

#[test]
fn a_helper_drag_released_over_the_footer_still_reaches_the_outside_picture_cleanup() {
    let footer = Footer::new(100, 24, info(true), &Pointer::default());
    let mut pointer = Pointer::default();
    assert!(
        !pointer
            .handle(
                &footer,
                mouse(MouseEventKind::Down(MouseButton::Left), 4, 4)
            )
            .consumed
    );
    assert!(
        pointer
            .handle(
                &footer,
                mouse(MouseEventKind::Drag(MouseButton::Left), 4, 23)
            )
            .consumed
    );
    assert!(
        !pointer
            .handle(&footer, mouse(MouseEventKind::Up(MouseButton::Left), 4, 23))
            .consumed
    );
}

#[test]
fn footer_scrolling_and_right_clicks_never_become_helper_input_or_take_control() {
    let footer = Footer::new(100, 24, info(false), &Pointer::default());
    let take = button(&footer, Action::Take).start;
    let mut pointer = Pointer::default();
    let scroll = pointer.handle(&footer, mouse(MouseEventKind::ScrollDown, take, 23));
    assert!(scroll.consumed);
    assert!(scroll.action.is_none());
    let right = pointer.handle(
        &footer,
        mouse(MouseEventKind::Down(MouseButton::Right), take, 23),
    );
    assert!(right.consumed);
    assert!(right.action.is_none());
    assert!(
        pointer
            .handle(
                &footer,
                mouse(MouseEventKind::Up(MouseButton::Right), take, 23)
            )
            .consumed
    );
    assert!(Action::Take.command(None).is_none());
}

#[test]
fn another_mouse_buttons_release_cannot_leak_the_rest_of_a_footer_press_to_the_helper() {
    let footer = Footer::new(100, 24, info(false), &Pointer::default());
    let take = button(&footer, Action::Take).start;
    let mut pointer = Pointer::default();
    assert!(
        pointer
            .handle(
                &footer,
                mouse(MouseEventKind::Down(MouseButton::Left), take, 23)
            )
            .consumed
    );
    assert!(
        pointer
            .handle(&footer, mouse(MouseEventKind::Up(MouseButton::Right), 4, 4))
            .consumed
    );
    assert!(
        pointer
            .handle(
                &footer,
                mouse(MouseEventKind::Drag(MouseButton::Left), 4, 4)
            )
            .consumed
    );
    assert!(
        pointer
            .handle(&footer, mouse(MouseEventKind::Up(MouseButton::Left), 4, 4))
            .consumed
    );
    assert!(
        !pointer
            .handle(&footer, mouse(MouseEventKind::Moved, 4, 4))
            .consumed
    );
}

#[test]
fn a_transient_focus_pause_is_visible_and_preserves_the_give_back_action() {
    let mut state = info(true);
    state.paused = true;
    let footer = Footer::new(120, 24, state, &Pointer::default());
    assert!(
        footer.rows[0]
            .iter()
            .any(|span| span.text.contains("Input paused") && span.foreground == WARNING)
    );
    assert!(
        footer.rows[1]
            .iter()
            .any(|span| span.text.contains("click this pane to continue"))
    );
    assert!(
        footer
            .buttons
            .iter()
            .any(|button| button.action == Action::GiveBack)
    );
    assert!(
        footer
            .buttons
            .iter()
            .all(|button| button.action != Action::Take)
    );
}

#[test]
fn a_watching_viewer_never_offers_take_control_until_focus_has_returned() {
    let mut state = info(false);
    state.paused = true;
    let footer = Footer::new(100, 24, state, &Pointer::default());
    assert!(footer.buttons.iter().all(|button| !matches!(
        button.action,
        Action::Take | Action::GiveBack | Action::Stop
    )));
}

#[test]
fn ownership_and_focus_status_take_precedence_over_branding_in_narrow_panes() {
    let active = Footer::new(18, 24, info(true), &Pointer::default());
    assert!(
        active.rows[0]
            .iter()
            .any(|span| span.text.contains("In control"))
    );
    assert!(active.rows[0].iter().all(|span| span.text != "Atlas"));
    let mut state = info(true);
    state.paused = true;
    let paused = Footer::new(18, 24, state, &Pointer::default());
    assert!(
        paused.rows[0]
            .iter()
            .any(|span| span.text.contains("Input paused"))
    );
    for row in paused.rows {
        for span in row {
            assert!(usize::from(span.column) + text_width(&span.text) <= 17);
        }
    }
}

#[test]
fn view_controls_are_complete_secondary_groups_and_never_replace_ownership_actions() {
    for columns in 16..180 {
        let footer = Footer::new(columns, 24, info(true), &Pointer::default());
        assert!(
            footer
                .buttons
                .iter()
                .any(|button| button.action == Action::GiveBack)
        );
        let zoom = footer
            .buttons
            .iter()
            .filter(|button| {
                matches!(
                    button.action,
                    Action::Fit | Action::ZoomIn | Action::ZoomOut
                )
            })
            .count();
        assert!(
            zoom == 0 || zoom == 3,
            "{columns} columns must not offer a partial zoom group"
        );
        for row in footer.rows {
            for span in row {
                assert!(usize::from(span.column) + text_width(&span.text) < usize::from(columns));
            }
        }
    }
}

#[test]
fn view_actions_request_local_view_changes_instead_of_helper_input() {
    for (action, change) in [
        (Action::Fit, Change::Fit),
        (Action::ZoomIn, Change::ZoomIn),
        (Action::ZoomOut, Change::ZoomOut),
        (Action::PanLeft, Change::PanLeft),
        (Action::PanRight, Change::PanRight),
        (Action::PanUp, Change::PanUp),
        (Action::PanDown, Change::PanDown),
    ] {
        assert_eq!(action.command(None), Some(Command::View(change)));
    }
    assert_eq!(
        Action::Diagnostics.command(None),
        Some(Command::Diagnostics)
    );
}

#[test]
fn the_header_reports_real_detail_limits_without_claiming_native_quality() {
    let mut state = info(false);
    state.detail_limited = true;
    state.view_label = "200%";
    let footer = Footer::new(160, 24, state, &Pointer::default());
    assert!(
        footer.rows[0]
            .iter()
            .any(|span| span.text.contains("Detail limited") && span.foreground == WARNING)
    );
    assert!(footer.rows[0].iter().any(|span| span.text.contains("200%")));
    assert!(
        footer.rows[0]
            .iter()
            .all(|span| !span.text.contains("100%") && !span.text.contains("Native"))
    );
}

#[test]
fn diagnostics_stay_optional_and_never_hide_pending_or_paused_control_feedback() {
    let mut state = info(false);
    state.diagnostics = Some("Helper: legacy1600 · proof: spawned");
    let footer = Footer::new(180, 24, state, &Pointer::default());
    assert!(
        footer.rows[1]
            .iter()
            .any(|span| span.text.contains("legacy1600"))
    );
    for phase in [
        ControlPhase::Taking,
        ControlPhase::Releasing,
        ControlPhase::Uncertain,
    ] {
        let mut state = info(false);
        state.phase = phase;
        state.message = "Ownership transition needs attention";
        state.diagnostics = Some("Helper: legacy1600");
        let footer = Footer::new(180, 24, state, &Pointer::default());
        assert!(
            footer.rows[1]
                .iter()
                .all(|span| !span.text.contains("legacy1600"))
        );
    }
    let mut state = info(true);
    state.paused = true;
    state.diagnostics = Some("Helper: legacy1600");
    let footer = Footer::new(180, 24, state, &Pointer::default());
    assert!(
        footer.rows[1]
            .iter()
            .any(|span| span.text.contains("Input paused"))
    );
    assert!(
        footer.rows[1]
            .iter()
            .all(|span| !span.text.contains("legacy1600"))
    );
}
