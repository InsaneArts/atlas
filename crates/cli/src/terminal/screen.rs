//! The terminal is owned by one thread and restored before any error is reported.
use super::{
    Command, State,
    chrome::{Footer, Information, Pointer},
    graphics::{self, Layout},
    input,
};
use atlas_core::{
    error::{Code, Error, Result},
    screen::png::png_dimensions,
};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use nix::poll::{PollFd, PollFlags, poll};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

struct Screen {
    painter: super::tiles::Tiles,
}

impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self {
            painter: super::tiles::Tiles::default(),
        };
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            Hide,
            EnableFocusChange,
            EnableMouseCapture,
            Clear(ClearType::All)
        )?;
        Ok(guard)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = out.write_all(b"\x1b[?2026l");
        let _ = self.painter.clear(&mut out);
        let _ = graphics::delete(&mut out);
        let _ = execute!(
            out,
            DisableMouseCapture,
            DisableFocusChange,
            Show,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

fn probe() -> Result<bool> {
    if !query(graphics::query, graphics::supported, Duration::from_secs(2))? {
        return Err(Error::new(
            Code::Usage,
            "This terminal did not confirm Kitty graphics support. Use Rex, Kitty or another compatible terminal, or atlas monitor for the native viewer.",
        ));
    }
    // Root-frame edits are optional. Keep a complete-frame fallback on
    // terminals that support images without this part of the protocol.
    let patches = query(
        graphics::patch_query,
        graphics::patch_supported,
        Duration::from_millis(150),
    );
    let mut out = io::stdout();
    graphics::patch_query_clear(&mut out)?;
    out.flush()?;
    patches
}

fn query(
    send: impl FnOnce(&mut io::Stdout) -> io::Result<()>,
    accepts: fn(&[u8]) -> bool,
    duration: Duration,
) -> Result<bool> {
    let mut out = io::stdout();
    send(&mut out)?;
    out.flush()?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let until = Instant::now() + duration;
    let mut reply = Vec::new();
    while Instant::now() < until && reply.len() < 4096 {
        let mut fd = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        poll(&mut fd, 50u16).map_err(io::Error::other)?;
        if fd[0]
            .revents()
            .is_some_and(|flags| flags.contains(PollFlags::POLLIN))
        {
            let mut bytes = [0u8; 256];
            let read = reader.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            reply.extend_from_slice(&bytes[..read]);
            if accepts(&reply) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Keyboard input runs independently of decoding, resizing and terminal writes.
#[derive(Clone, Default)]
struct Visible {
    frame: Option<Arc<crate::viewer::SharedFrame>>,
    layout: Option<Layout>,
    footer: Footer,
    projection: Option<super::viewport::Projection>,
    budget: Arc<std::sync::atomic::AtomicU64>,
}

struct PresentedPicture {
    frame: Arc<crate::viewer::SharedFrame>,
    layout: Layout,
    terminal: (u16, u16, u16, u16),
    projection: super::viewport::Projection,
}

struct StopOnExit<'a>(&'a AtomicBool);
impl Drop for StopOnExit<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) fn run(
    mut state: watch::Receiver<State>,
    commands: mpsc::Sender<Command>,
    stop: Arc<AtomicBool>,
    read_only: bool,
    budget: Arc<std::sync::atomic::AtomicU64>,
) -> Result<()> {
    let mut screen = Screen::enter()?;
    screen.painter.set_patches(probe()?);
    let visible = std::sync::Mutex::new(Visible {
        budget,
        ..Visible::default()
    });
    let pointer = std::sync::Mutex::new(Pointer::default());
    let resized = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let input_state = state.clone();
        let visible = &visible;
        let resized = &resized;
        let pointer = &pointer;
        let stopped = stop.as_ref();
        let _stop = StopOnExit(stopped);
        let events = scope.spawn(move || {
            let _stop = StopOnExit(stopped);
            read_events(
                input_state,
                commands,
                stopped,
                visible,
                pointer,
                resized,
                read_only,
            )
        });
        let drawing = render(
            &mut state,
            &mut screen,
            stopped,
            visible,
            pointer,
            resized,
            read_only,
        );
        stopped.store(true, Ordering::Release);
        let input = events
            .join()
            .map_err(|_| Error::new(Code::Io, "The keyboard reader stopped"))?;
        drawing.and(input)
    })
}

fn render(
    state: &mut watch::Receiver<State>,
    screen: &mut Screen,
    stop: &AtomicBool,
    visible: &std::sync::Mutex<Visible>,
    pointer: &std::sync::Mutex<Pointer>,
    resized: &AtomicBool,
    read_only: bool,
) -> Result<()> {
    let mut revision = u64::MAX;
    let mut last_picture: Option<PresentedPicture> = None;
    let mut acquisition_times = std::collections::VecDeque::new();
    let mut out = io::stdout();
    while !stop.load(Ordering::Acquire) {
        let snapshot = state.borrow_and_update().clone();
        if revision != snapshot.revision || resized.swap(false, Ordering::AcqRel) {
            revision = snapshot.revision;
            let size = terminal::window_size()?;
            if size.width == 0 || size.height == 0 {
                return Err(Error::new(
                    Code::Usage,
                    "The terminal did not report its pixel size. Use a Rex pane with pixel geometry, or atlas monitor for the native viewer.",
                ));
            }
            let projection = snapshot.frame.as_ref().and_then(|frame| {
                let dimensions = if frame.rgb.is_some() {
                    frame.pixels
                } else {
                    png_dimensions(&frame.png).ok()?
                };
                Some(snapshot.view.projection(dimensions))
            });
            let next = projection.and_then(|projection| {
                Layout::fit(
                    size.columns,
                    size.rows,
                    (size.width, size.height),
                    projection.size,
                )
            });
            let layout = next;
            let shown = snapshot.frame.clone();
            let budget = visible
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .budget
                .clone();
            if let (Some(frame), Some(fit), Some(projection)) = (&shown, layout, projection) {
                let terminal = (size.columns, size.rows, size.width, size.height);
                let unchanged = last_picture.as_ref().is_some_and(|previous| {
                    Arc::ptr_eq(&previous.frame, frame)
                        && previous.layout == fit
                        && previous.terminal == terminal
                        && previous.projection == projection
                });
                // Local hover and control feedback must not rehash or encode a
                // high-resolution picture that is already on the terminal.
                if !unchanged {
                    let encoding = Instant::now();
                    if let Some(metrics) = frame.metrics {
                        acquisition_times.push_back(metrics.total_ms);
                        if acquisition_times.len() > 120 {
                            acquisition_times.pop_front();
                        }
                    }
                    if let Some(rgb) = &frame.rgb {
                        if projection.is_full() {
                            screen
                                .painter
                                .draw_rgb(&mut out, rgb, frame.pixels, fit, terminal)?;
                        } else {
                            let cropped = projection.crop_rgb(rgb)?;
                            screen.painter.draw_rgb(
                                &mut out,
                                &cropped,
                                projection.size,
                                fit,
                                terminal,
                            )?;
                        }
                    } else {
                        if projection.is_full() {
                            screen.painter.draw(&mut out, &frame.png, fit, terminal)?;
                        } else {
                            let decoded = image::load_from_memory_with_format(
                                &frame.png,
                                image::ImageFormat::Png,
                            )
                            .map_err(io::Error::other)?
                            .to_rgb8();
                            let cropped = projection.crop_rgb(decoded.as_raw())?;
                            screen.painter.draw_rgb(
                                &mut out,
                                &cropped,
                                projection.size,
                                fit,
                                terminal,
                            )?;
                        }
                    }
                    budget.store(
                        encoding.elapsed().as_millis().min(500) as u64,
                        Ordering::Relaxed,
                    );
                }
                last_picture = Some(PresentedPicture {
                    frame: frame.clone(),
                    layout: fit,
                    terminal,
                    projection,
                });
            } else {
                screen.painter.clear(&mut out)?;
                last_picture = None;
            }
            let picture = shown.as_ref().and_then(|frame| {
                let dimensions = if frame.rgb.is_some() {
                    frame.pixels
                } else {
                    png_dimensions(&frame.png).ok()?
                };
                Some((if frame.rgb.is_some() { "RGB" } else { "PNG" }, dimensions))
            });
            let diagnostics = if snapshot.diagnostics > 0 {
                let timing = screen.painter.timing();
                let mode = if screen.painter.patches_enabled() {
                    "root edits"
                } else {
                    "full frames"
                };
                Some(if snapshot.diagnostics == 7 {
                    format!(
                        "7/7 Last interruption: {}",
                        snapshot.last_frame_error.as_deref().unwrap_or("none")
                    )
                } else if let Some(metrics) = shown.as_ref().and_then(|frame| frame.metrics) {
                    let mut sorted: Vec<_> = acquisition_times.iter().copied().collect();
                    sorted.sort_by(f64::total_cmp);
                    let p50 = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
                    let p95 = sorted
                        .get(sorted.len().saturating_sub(1) * 95 / 100)
                        .copied()
                        .unwrap_or(0.0);
                    let native = metrics
                        .native_capture_ms
                        .map(|ms| format!("{ms:.1}ms"))
                        .unwrap_or_else(|| "unknown".into());
                    match snapshot.diagnostics {
                        1 => format!(
                            "1/7 {} | {mode} | {} proofs | cap {}",
                            metrics.backend,
                            if metrics.profile.persistent_proofs {
                                "persistent"
                            } else {
                                "legacy"
                            },
                            metrics.profile.ceiling
                        ),
                        2 => format!(
                            "2/7 Read p50/p95 {p50:.1}/{p95:.1}ms | input ack {}",
                            snapshot
                                .input_ack_ms
                                .map(|ms| format!("{ms:.1}ms"))
                                .unwrap_or_else(|| "unknown".into())
                        ),
                        3 => format!(
                            "3/7 Native {native} | proof {:.1}ms | wait/read {:.1}/{:.1}ms",
                            metrics.before_ms + metrics.after_ms,
                            metrics.capture_wait_ms,
                            metrics.transfer_ms
                        ),
                        4 => format!(
                            "4/7 Encode/write {:.1}/{:.1}ms | {}B",
                            timing.encode_ms, timing.write_ms, timing.bytes
                        ),
                        5 => metrics.capture_stats.map_or_else(
                            || "5/7 Native stream counters unavailable".into(),
                            |stats| {
                                format!(
                                    "5/7 SCK {} | fallback {} ({}) | idle {} | timeout {}",
                                    stats.native_frames,
                                    stats.fallbacks,
                                    if stats.last_fallback == "native-error" {
                                        stats.native_reason
                                    } else {
                                        stats.last_fallback
                                    },
                                    stats.idle_hints,
                                    stats.timeouts
                                )
                            },
                        ),
                        _ => metrics.capture_stats.map_or_else(
                            || "6/7 Native stream timings unavailable".into(),
                            |stats| {
                                format!(
                                    "6/7 SCK last wait {:.1}ms | guard {:.1}ms | copy {:.1}ms",
                                    stats.wait_ms, stats.guard_ms, stats.copy_ms
                                )
                            },
                        ),
                    }
                } else if shown.as_ref().is_some_and(|frame| frame.rgb.is_some()) {
                    // Raw pixels without acquisition timings come from the
                    // menu bar's shared frame service.
                    format!("Shared frame service | raw RGB | {mode}")
                } else {
                    format!(
                        "PNG | {mode} | encode/write {:.1}/{:.1}ms | {}B",
                        timing.encode_ms, timing.write_ms, timing.bytes
                    )
                })
            } else {
                None
            };
            let footer = Footer::new(
                size.columns,
                size.rows,
                Information {
                    controlling: snapshot.controlling,
                    paused: snapshot.paused,
                    phase: snapshot.phase,
                    read_only,
                    available: shown.is_some() && layout.is_some(),
                    message: &snapshot.message,
                    picture,
                    view_label: snapshot.view.label(),
                    detail_limited: shown.as_ref().is_some_and(|frame| {
                        frame.metrics.is_some_and(|metrics| {
                            frame.pixels.0 < metrics.native_size.0
                                || frame.pixels.1 < metrics.native_size.1
                        })
                    }),
                    diagnostics: diagnostics.as_deref(),
                },
                &pointer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            footer.draw(&mut out)?;
            out.flush()?;
            *visible
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Visible {
                frame: shown.clone(),
                layout,
                footer,
                projection,
                budget,
            };
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn read_events(
    state: watch::Receiver<State>,
    commands: mpsc::Sender<Command>,
    stop: &AtomicBool,
    visible: &std::sync::Mutex<Visible>,
    pointer: &std::sync::Mutex<Pointer>,
    resized: &AtomicBool,
    read_only: bool,
) -> Result<()> {
    let mut prefix = None;
    let mut focused = true;
    while !stop.load(Ordering::Acquire) {
        if !event::poll(Duration::from_millis(8))? {
            continue;
        }
        let event = event::read()?;
        let view = visible
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let shown = view.frame;
        let layout = view.layout;
        // Local focus closes the gate before the actor can publish its state.
        // FocusGained is sent in FIFO order before any following input, so a
        // delayed watch update must not discard the first key after returning.
        let captured = focused && state.borrow().controlling;
        let mut chrome_command = None;
        let chrome_consumed = if let Event::Mouse(mouse) = event {
            let outcome = pointer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .handle(&view.footer, mouse);
            if outcome.changed {
                resized.store(true, Ordering::Release);
            }
            chrome_command = outcome.action.and_then(|action| {
                action.command(
                    shown
                        .as_ref()
                        .filter(|_| layout.is_some())
                        .map(|frame| frame.key.as_str()),
                )
            });
            outcome.consumed
        } else {
            false
        };
        let command = if chrome_consumed {
            chrome_command
        } else {
            match event {
            Event::Resize(_, _) => {
                resized.store(true, Ordering::Release);
                Some(Command::Suspend("Control released: pane resized. Press T to take control again."))
            }
            Event::FocusLost => { focused = false; prefix = None; Some(Command::FocusPause(true)) },
            Event::FocusGained => { focused = true; Some(Command::FocusPause(false)) },
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if !focused && !matches!(input::shortcut(key, captured, read_only, None), Some(Command::GiveBack | Command::Quit)) {
                    continue;
                }
                let frame = shown
                    .as_ref()
                    .filter(|_| layout.is_some())
                    .map(|frame| frame.key.as_str());
                if let Some(command) = input::shortcut(key, captured, read_only, frame) {
                    prefix = None;
                    Some(command)
                } else if key.code == KeyCode::Char('a')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    if prefix.take().is_some() {
                        shown.as_ref().and_then(|frame| {
                            input::key(key).map(|action| Command::Input {
                                frame: frame.key.clone(),
                                action,
                            })
                        })
                    } else {
                        prefix = Some(Instant::now());
                        None
                    }
                } else if prefix
                    .take()
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
                {
                    match key.code {
                        KeyCode::Char('q' | 'Q') => Some(Command::Quit),
                        KeyCode::Char('t' | 'T') if !read_only => shown
                            .as_ref()
                            .filter(|_| layout.is_some())
                            .map(|frame| Command::Take(frame.key.clone())),
                        KeyCode::Char('g' | 'G') => Some(Command::GiveBack),
                        KeyCode::Char('s' | 'S') if !read_only => Some(Command::Stop),
                        _ => None,
                    }
                } else if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                    && !captured
                {
                    Some(Command::Quit)
                } else if captured {
                    shown.as_ref().and_then(|frame| {
                        input::key(key).map(|action| Command::Input {
                            frame: frame.key.clone(),
                            action,
                        })
                    })
                } else {
                    let change = match key.code {
                        KeyCode::Char('f' | 'F') => Some(super::viewport::Change::Fit),
                        KeyCode::Char('+' | '=') => Some(super::viewport::Change::ZoomIn),
                        KeyCode::Char('-') => Some(super::viewport::Change::ZoomOut),
                        KeyCode::Left => Some(super::viewport::Change::PanLeft),
                        KeyCode::Right => Some(super::viewport::Change::PanRight),
                        KeyCode::Up => Some(super::viewport::Change::PanUp),
                        KeyCode::Down => Some(super::viewport::Change::PanDown),
                        _ => None,
                    };
                    change.map(Command::View).or_else(|| matches!(key.code,KeyCode::Char('d'|'D')).then_some(Command::Diagnostics))
                }
            }
            Event::Mouse(mouse) if captured => shown.as_ref().and_then(|frame| {
                let action = layout
                    .and_then(|fit| view.projection.and_then(|projection| projection.point(fit, mouse.column, mouse.row, frame.display)))
                    .and_then(|point| input::mouse(mouse, point))
                    .map(|action| Command::Input {
                        frame: frame.key.clone(),
                        action,
                    });
                action.or_else(|| {
                    matches!(mouse.kind, event::MouseEventKind::Up(_)).then_some(Command::Suspend("Control released: mouse released outside the picture. Press T to take control again."))
                })
            }),
            _ => None,
        }
        };
        if let Some(command) = command {
            let command = match (command, view.projection) {
                (Command::Take(frame), Some(projection)) => {
                    Command::TakeView(frame, projection.generation)
                }
                (Command::Input { frame, action }, Some(projection)) => Command::InputView {
                    frame,
                    action,
                    generation: projection.generation,
                },
                (command, _) => command,
            };
            // Overflow ends control: in particular, a dropped mouse-up must not stick.
            if commands.try_send(command).is_err() {
                stop.store(true, Ordering::Release);
                break;
            }
        }
    }
    Ok(())
}
