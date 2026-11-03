//! A Kitty graphics terminal front end to the existing local viewer service.
//! Control is acquired only by a terminal shortcut. It uses the same frame keys,
//! leases, cancellation and pending recovery as Atlas.app; no VNC or HID path.
mod chrome;
mod graphics;
mod input;
mod output;
mod quality;
mod screen;
mod tiles;
mod viewport;

use crate::viewer::{self, SharedFrame};
use atlas_client::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    protocol::CaptureBackend,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::IsTerminal,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Notify, mpsc, watch},
    task::JoinSet,
};

#[derive(Clone)]
struct State {
    revision: u64,
    frame: Option<Arc<SharedFrame>>,
    controlling: bool,
    paused: bool,
    phase: ControlPhase,
    message: String,
    view: viewport::Viewport,
    diagnostics: u8,
    input_ack_ms: Option<f64>,
    last_frame_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlPhase {
    Watching,
    Taking,
    Controlling,
    Releasing,
    Uncertain,
}

#[derive(Debug, PartialEq)]
enum Command {
    Take(String),
    TakeView(String, u64),
    InputView {
        frame: String,
        action: Value,
        generation: u64,
    },
    View(viewport::Change),
    Diagnostics,
    GiveBack,
    FocusPause(bool),
    Suspend(&'static str),
    Stop,
    Quit,
    Input {
        frame: String,
        action: Value,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Operation {
    Acquire,
    Input,
    Renew,
    Release,
    Stop,
}

/// Watch the helper inside an interactive terminal until it closes or the person exits.
///
/// # Errors
///
/// Returns an error when either standard stream is not a terminal, the helper
/// cannot be verified, the terminal lacks Kitty graphics or pixel geometry, or
/// control cleanup was not confirmed; an unconfirmed outcome stays pending.
pub async fn monitor(
    client: Client,
    settings: &std::path::Path,
    read_only: bool,
    max_dimension: Option<u32>,
    backend: CaptureBackend,
) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(Error::new(
            Code::Usage,
            "atlas monitor --terminal requires an interactive terminal for both input and output",
        ));
    }
    let display = client.identity().await?.epoch.display;
    let points = (display.width, display.height);
    // With the menu bar running for this helper, the terminal shares its
    // frame service and its control routes, so no second capture runs.
    if backend == CaptureBackend::CoreGraphics
        && max_dimension.is_none()
        && let Some((url, descriptor)) = shared(&client).await
    {
        let frames = Frames::Shared {
            descriptor,
            client: None,
        };
        return connected(url, frames, read_only, max_dimension, points).await;
    }
    let dimension = quality::current(max_dimension, points);
    let service = viewer::TerminalService::start(client, settings, dimension, backend).await?;
    let result = connected(
        service.url.clone(),
        Frames::Private(service.frames()),
        read_only,
        max_dimension,
        points,
    )
    .await;
    let cleanup = service.shutdown().await;
    result.and(cleanup)
}

/// The running menu bar's address and its frame service's description,
/// when the menu bar serves `client`'s helper and runs one.
async fn shared(client: &Client) -> Option<(String, Value)> {
    let record = client.output_dir().ok()?.join(viewer::MENUBAR_RECORD);
    let helper = client.transport.peer_uid;
    tokio::task::spawn_blocking(move || {
        let found = viewer::read_record(&record)?;
        let port = viewer::recorded_port(&found)?;
        let running = viewer::running(&record, port)?;
        let url = viewer::address(&running)?.to_owned();
        let body = viewer::fetch(&url, "pipe.json", Duration::from_secs(2), 16 * 1024).ok()?;
        let descriptor: Value = serde_json::from_slice(&body).ok()?;
        (descriptor["helperUID"].as_u64() == Some(u64::from(helper))).then_some((url, descriptor))
    })
    .await
    .ok()
    .flatten()
}

/// Where the terminal's pictures come from: its own private viewer, or the
/// menu bar's frame service, shared with every other viewer.
enum Frames {
    Private(viewer::TerminalFrames),
    Shared {
        descriptor: Value,
        /// The observer, once paired; a failed one is paired again.
        client: Option<crate::frame_pipe::client::PipeClient>,
    },
}

impl Frames {
    fn set_dimension(&mut self, dimension: u32) {
        if let Self::Private(frames) = self {
            frames.set_dimension(dimension);
        }
    }

    /// The shortest gap between reads: the shared service paces itself.
    fn interval(&self) -> Duration {
        match self {
            Self::Private(frames) => frames.interval(),
            Self::Shared { .. } => Duration::ZERO,
        }
    }

    async fn next(&mut self) -> Result<SharedFrame> {
        match self {
            Self::Private(frames) => frames.next().await,
            Self::Shared { descriptor, client } => {
                let result = shared_next(descriptor, client).await;
                if result.is_err() {
                    *client = None;
                }
                result
            }
        }
    }
}

/// The shared service's next picture, committed, pairing first when needed.
async fn shared_next(
    descriptor: &Value,
    client: &mut Option<crate::frame_pipe::client::PipeClient>,
) -> Result<SharedFrame> {
    let observer = match client {
        Some(observer) => observer,
        None => client
            .insert(crate::frame_pipe::client::PipeClient::open(descriptor, SHARED_LONGEST).await?),
    };
    let picture = observer.next().await?;
    observer.commit(&picture.metadata).await?;
    let metadata = picture.metadata;
    Ok(SharedFrame {
        png: Vec::new(),
        rgb: Some(Arc::new(picture.rgb)),
        pixels: (metadata.width.get(), metadata.height.get()),
        metrics: None,
        key: metadata.frame_key.as_str().to_owned(),
        display: (metadata.display_width.get(), metadata.display_height.get()),
    })
}

/// The detail the terminal asks the shared service for.
const SHARED_LONGEST: u32 = 1600;

async fn connected(
    url: String,
    mut source: Frames,
    read_only: bool,
    max_dimension: Option<u32>,
    mut display: (f64, f64),
) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let (states, receiver) = watch::channel(State {
        revision: 0,
        frame: None,
        controlling: false,
        paused: false,
        phase: ControlPhase::Watching,
        message: "Connecting to the hidden helper...".into(),
        view: viewport::Viewport::default(),
        diagnostics: 0,
        input_ack_ms: None,
        last_frame_error: None,
    });
    let (commands, mut incoming) = mpsc::channel(64);
    let terminal_stop = stop.clone();
    let render_budget = Arc::new(AtomicU64::new(0));
    let screen_budget = render_budget.clone();
    let mut terminal = tokio::task::spawn_blocking(move || {
        screen::run(receiver, commands, terminal_stop, read_only, screen_budget)
    });
    let (frames, mut captured) = mpsc::channel(1);
    let mut session = Session::new(url.clone(), read_only);
    let refresh = session.refresh.clone();
    let frame_stop = stop.clone();
    let view_state = states.subscribe();
    let reader = tokio::spawn(async move {
        let mut next = tokio::time::Instant::now();
        let mut previous_key = None;
        while !frame_stop.load(Ordering::Acquire) {
            tokio::select! { _ = tokio::time::sleep_until(next) => {}, _ = refresh.notified() => {} }
            if frame_stop.load(Ordering::Acquire) {
                break;
            }
            let started = tokio::time::Instant::now();
            let mut dimension = quality::current(max_dimension, display);
            if max_dimension.is_none() {
                dimension = (f64::from(dimension) * view_state.borrow().view.factor())
                    .ceil()
                    .min(4096.0) as u32;
            }
            source.set_dimension(dimension);
            let result = source.next().await;
            next = started
                + source.interval().max(Duration::from_millis(
                    render_budget.load(Ordering::Relaxed).min(500),
                ));
            if let Ok(frame) = &result {
                display = frame.display;
                if previous_key.as_deref() == Some(frame.key.as_str()) {
                    continue;
                }
                previous_key = Some(frame.key.clone());
            } else {
                previous_key = None;
            }
            if frames.send(result).await.is_err() {
                break;
            }
        }
    });
    let mut renew = tokio::time::interval(Duration::from_secs(3));
    renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let signal = crate::watch::stopped();
    tokio::pin!(signal);
    let mut terminal_result = None;
    let mut last_good = None;
    loop {
        tokio::select! {
            result = &mut terminal => {
                terminal_result = Some(result);
                break;
            }
            _ = &mut signal => break,
            command = incoming.recv() => {
                match command {
                    None | Some(Command::Quit) => break,
                    Some(command) => session.command(command, &states),
                }
            }
            Some(frame) = captured.recv() => {
                received(&mut session, &states, &mut last_good, frame);
            }
            Some(result) = session.operations.join_next(), if !session.operations.is_empty() => {
                match result {
                    Ok(result) => session.completed(result, &states),
                    Err(_) => {
                        session.cleanup_error = Some(Error::new(Code::Uncertain, "A viewer request stopped before its result was known").uncertain());
                        break;
                    }
                }
            }
            _ = renew.tick() => session.renew(&states),
        }
    }
    // Cancellation gets its own urgent HTTP request before waiting for input.
    // No in-flight mutation is dropped or repeated; the service keeps unknown
    // native outcomes pending until its existing recovery confirms cleanup.
    stop.store(true, Ordering::Release);
    drop(captured);
    session.closing = true;
    session.release(&states);
    while let Some(result) = session.operations.join_next().await {
        match result {
            Ok(result) => session.completed(result, &states),
            Err(_) => {
                session.cleanup_error = Some(
                    Error::new(
                        Code::Uncertain,
                        "A viewer request ended without a confirmed outcome",
                    )
                    .uncertain(),
                )
            }
        }
    }
    let result = match terminal_result {
        Some(result) => result,
        None => terminal.await,
    };
    // Read-only requests have a finite deadline too; let the current one finish.
    let _ = reader.await;
    result.map_err(|_| Error::new(Code::Io, "The terminal renderer stopped"))??;
    if let Some(error) = session.cleanup_error {
        return Err(error);
    }
    Ok(())
}

/// A late picture is withheld, not promoted to new input authority. Brief
/// capture jitter can retain the last verified view within its existing age
/// guards; a sustained gap or any other failure still revokes ownership.
fn received(
    session: &mut Session,
    states: &watch::Sender<State>,
    last_good: &mut Option<std::time::Instant>,
    result: Result<SharedFrame>,
) {
    let can_keep = last_good.is_some_and(|at| at.elapsed() < Duration::from_secs(2))
        && states.borrow().frame.is_some();
    match result {
        Ok(frame) => {
            *last_good = Some(std::time::Instant::now());
            change(states, |state| {
                state.frame = Some(Arc::new(frame));
                if [
                    "Connecting",
                    "Connection lost",
                    "Picture delayed",
                    "Picture unavailable",
                ]
                .iter()
                .any(|prefix| state.message.starts_with(prefix))
                {
                    state.message = if state.controlling {
                        "You have control. Ctrl+G gives it back; Ctrl+Q exits."
                    } else {
                        "Watching the hidden helper"
                    }
                    .into();
                }
            });
        }
        Err(error) if error.code == Code::StaleFrame && can_keep => {
            change(states, |state| {
                state.message = "Picture delayed; retrying a fresh frame.".into()
            });
        }
        Err(error) => {
            let first_in_outage = last_good.is_some() || states.borrow().last_frame_error.is_none();
            *last_good = None;
            change(states, |state| {
                state.frame = None;
                state.message = format!("Picture unavailable ({}): {}", error.code, error.error);
                if first_in_outage {
                    // Retain the first cause through reconnect attempts and
                    // recovery, rather than replacing it with a later busy.
                    // Only fixed words are kept: the code, and the stage and
                    // reason the live reader names, never a reply's text.
                    let word = |key: &str| {
                        error
                            .details
                            .get(key)
                            .and_then(|value| value.as_str())
                            .filter(|word| {
                                word.len() <= 32
                                    && word
                                        .bytes()
                                        .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
                            })
                    };
                    let stage = word("stage")
                        .zip(word("reason"))
                        .map(|(stage, why)| format!(" at {stage} ({why})"))
                        .unwrap_or_default();
                    state.last_frame_error = Some(format!("{}{stage}", error.code));
                }
            });
            session.release(states);
        }
    }
}

fn change(states: &watch::Sender<State>, update: impl FnOnce(&mut State)) {
    states.send_modify(|state| {
        update(state);
        state.revision = state.revision.wrapping_add(1);
    });
}

struct Session {
    url: String,
    lease: Option<String>,
    pending: bool,
    paused: bool,
    held_buttons: u8,
    pending_buttons: Option<u8>,
    pending_pointer: bool,
    pending_view: Option<viewport::Viewport>,
    input_started: Option<std::time::Instant>,
    renewing: bool,
    releasing: bool,
    closing: bool,
    read_only: bool,
    acquire_cancelled: bool,
    queue: VecDeque<(String, Value)>,
    operations: JoinSet<(Operation, Result<Value>)>,
    cleanup_error: Option<Error>,
    refresh: Arc<Notify>,
}

impl Session {
    fn new(url: String, read_only: bool) -> Self {
        Self {
            url,
            lease: None,
            pending: false,
            paused: false,
            held_buttons: 0,
            pending_buttons: None,
            pending_pointer: false,
            pending_view: None,
            input_started: None,
            renewing: false,
            releasing: false,
            closing: false,
            read_only,
            acquire_cancelled: false,
            queue: VecDeque::new(),
            operations: JoinSet::new(),
            cleanup_error: None,
            refresh: Arc::new(Notify::new()),
        }
    }

    fn request(&mut self, operation: Operation, route: &'static str, body: Value) {
        let url = self.url.clone();
        self.operations.spawn_blocking(move || {
            let result = viewer::post_local(&url, route, &body).map_err(|error| {
                // A lost HTTP reply does not prove a mutation was refused.
                if error.code == Code::Menubar {
                    error.uncertain()
                } else {
                    error
                }
            });
            (operation, result)
        });
    }

    fn command(&mut self, command: Command, states: &watch::Sender<State>) {
        let current_generation = states.borrow().view.generation;
        match command {
            Command::Diagnostics => change(states, |state| {
                state.diagnostics = (state.diagnostics + 1) % 8
            }),
            Command::View(change_view) => {
                let current = self.pending_view.unwrap_or(states.borrow().view);
                let next = current.changed(change_view);
                if next != current {
                    self.pending_view = Some(next);
                    self.release(states);
                    change(states, |state| {
                        state.message = "Changing view; giving back control...".into()
                    });
                    self.apply_view(states);
                }
            }
            Command::TakeView(frame, generation) if generation == current_generation => {
                self.command(Command::Take(frame), states)
            }
            Command::InputView {
                frame,
                action,
                generation,
            } if generation == current_generation => {
                self.command(Command::Input { frame, action }, states)
            }
            Command::Take(frame)
                if !self.read_only
                    && !self.paused
                    && self.pending_view.is_none()
                    && !self.pending
                    && !self.releasing
                    && self.lease.is_none()
                    && self.cleanup_error.is_none() =>
            {
                self.pending = true;
                self.acquire_cancelled = false;
                change(states, |state| state.message = "Taking control...".into());
                self.request(
                    Operation::Acquire,
                    "control/acquire",
                    json!({"frame":frame}),
                );
            }
            Command::GiveBack => {
                self.release(states);
                change(states, |state| {
                    state.message = if self.releasing || self.pending {
                        "Giving back control..."
                    } else {
                        "Watching the hidden helper"
                    }
                    .into()
                });
            }
            Command::FocusPause(paused) => {
                self.paused = paused;
                change(states, |state| state.paused = paused);
                if paused {
                    self.queue.clear();
                    if self.held_buttons != 0
                        || self.pending_pointer
                        || self.pending_buttons.is_some_and(|buttons| buttons != 0)
                    {
                        self.release(states);
                        change(states, |state| {
                            state.message = "Control released: focus lost during a mouse gesture. Take control again when ready.".into()
                        });
                    }
                }
                // Clearing pause never acquires a lease or changes ownership.
            }
            Command::Suspend(reason) => {
                let held = self.lease.is_some() || self.pending;
                self.release(states);
                if held {
                    change(states, |state| state.message = reason.into());
                }
            }
            Command::Stop if !self.read_only => {
                self.release(states);
                self.request(Operation::Stop, "control/hold", json!({"on":true}));
            }
            Command::Input { frame, mut action }
                if self.lease.is_some()
                    && !self.releasing
                    && !self.paused
                    && self.pending_view.is_none() =>
            {
                // Motion is replaceable until sent. Preserve down/up and key
                // boundaries; thousands of hover positions must not crowd
                // the release of a mouse button out of the queue.
                if let Some((last_frame, last)) = self.queue.back_mut() {
                    if *last_frame == frame && coalesce_scroll(last, &mut action) {
                        return;
                    }
                    // Compact committed ASCII before testing queue capacity.
                    // A paste may arrive as hundreds of individual key events
                    // while one request is blocked; each queued type action
                    // still holds at most one short request's typing budget.
                    if *last_frame == frame && merge_text(last, &action, INPUT_BATCH_UNITS) {
                        return;
                    }
                    if replaceable_motion(last, &action) {
                        *last_frame = frame;
                        *last = action;
                        return;
                    }
                }
                if self.queue.len() >= 32 {
                    self.release(states);
                    change(states, |state| {
                        state.message = "Input exceeded the buffer; control released. Take control again when ready.".into()
                    });
                } else {
                    self.queue.push_back((frame, action));
                    self.send_next();
                }
            }
            _ => {}
        }
        self.publish_phase(states);
    }

    fn apply_view(&mut self, states: &watch::Sender<State>) {
        if self.lease.is_none()
            && !self.pending
            && !self.releasing
            && self.cleanup_error.is_none()
            && let Some(view) = self.pending_view.take()
        {
            change(states, |state| {
                state.view = view;
                state.message = "Watching the hidden helper".into();
            });
        }
    }

    fn send_next(&mut self) {
        if self.pending || self.releasing || self.closing || self.paused {
            return;
        }
        if let Some(lease) = self.lease.clone()
            && let Some((frame, actions)) = input_batch(&mut self.queue)
        {
            self.pending = true;
            self.input_started = Some(std::time::Instant::now());
            self.pending_buttons = Some(buttons_after(self.held_buttons, &actions));
            self.pending_pointer = actions.iter().any(|action| action["type"] == "pointer");
            self.request(
                Operation::Input,
                "input",
                json!({"lease":lease, "frame":frame, "actions":actions}),
            );
        }
    }

    fn release(&mut self, states: &watch::Sender<State>) {
        self.acquire_cancelled = true;
        self.queue.clear();
        change(states, |state| state.controlling = false);
        if self.releasing {
            self.publish_phase(states);
            return;
        }
        if let Some(lease) = self.lease.clone() {
            self.releasing = true;
            self.request(
                Operation::Release,
                "control/release",
                json!({"lease":lease}),
            );
        }
        // An acquire still in progress is released as soon as it answers.
        self.publish_phase(states);
    }

    fn publish_phase(&self, states: &watch::Sender<State>) {
        let phase = if self.cleanup_error.is_some() {
            ControlPhase::Uncertain
        } else if self.releasing || (self.acquire_cancelled && self.pending) {
            ControlPhase::Releasing
        } else if self.lease.is_some() && states.borrow().controlling {
            ControlPhase::Controlling
        } else if self.pending {
            ControlPhase::Taking
        } else {
            ControlPhase::Watching
        };
        if states.borrow().phase != phase {
            change(states, |state| state.phase = phase);
        }
    }

    fn renew(&mut self, states: &watch::Sender<State>) {
        if self.renewing || self.releasing || self.closing {
            return;
        }
        if let Some(lease) = self.lease.clone()
            && let Some(frame) = states
                .borrow()
                .frame
                .as_ref()
                .map(|frame| frame.key.clone())
        {
            self.renewing = true;
            self.request(
                Operation::Renew,
                "control/renew",
                json!({"lease":lease, "frame":frame}),
            );
        }
    }

    fn completed(
        &mut self,
        (operation, result): (Operation, Result<Value>),
        states: &watch::Sender<State>,
    ) {
        if matches!(operation, Operation::Acquire | Operation::Input) {
            self.pending = false;
        }
        if operation == Operation::Input {
            self.pending_pointer = false;
            if let Some(started) = self.input_started.take() {
                change(states, |state| {
                    state.input_ack_ms = Some(started.elapsed().as_secs_f64() * 1000.0)
                });
            }
        }
        if operation == Operation::Renew {
            self.renewing = false;
        }
        match (operation, result) {
            (Operation::Acquire, Ok(reply)) => {
                self.lease = reply["lease"].as_str().map(str::to_owned);
                if self.closing
                    || self.releasing
                    || self.acquire_cancelled
                    || states.borrow().frame.is_none()
                {
                    self.release(states);
                } else {
                    change(states, |state| {
                        state.controlling = self.lease.is_some();
                        state.message =
                            "You have control. Ctrl+G gives it back; Ctrl+Q exits.".into();
                    });
                }
            }
            (Operation::Release, result) => {
                self.releasing = false;
                self.lease = None;
                self.held_buttons = 0;
                change(states, |state| {
                    state.controlling = false;
                });
                if let Err(error) = result {
                    change(states, |state| {
                        state.message = format!(
                            "Cleanup unconfirmed: {}. Open Atlas to recover.",
                            error.error
                        )
                    });
                    self.cleanup_error = Some(error);
                } else {
                    change(states, |state| {
                        if state.message.starts_with("Giving back")
                            || state.message.starts_with("Taking control")
                        {
                            state.message = "Watching the hidden helper".into();
                        }
                    });
                }
            }
            (Operation::Stop, Ok(_)) => change(states, |state| {
                state.message = "Agents stopped. Resume from Atlas when ready.".into()
            }),
            (Operation::Input, Ok(_)) => {
                if !self.releasing && self.lease.is_some() {
                    self.held_buttons = self.pending_buttons.take().unwrap_or(self.held_buttons);
                } else {
                    self.pending_buttons = None;
                }
                self.refresh.notify_one();
            }
            (_, Err(error)) => {
                if operation == Operation::Input {
                    self.pending_buttons = None;
                }
                self.release(states);
                change(states, |state| {
                    state.message = format!("{}: {}", error.code.as_str(), error.error)
                });
                if error.uncertain {
                    self.cleanup_error = Some(error);
                }
            }
            _ => {}
        }
        self.send_next();
        self.apply_view(states);
        self.publish_phase(states);
    }
}

/// Track acknowledged native pointer state, not just local Down/Up arrivals.
/// An Up queued behind another request has not yet released the helper button.
fn buttons_after(mut held: u8, actions: &[Value]) -> u8 {
    for action in actions {
        if action["type"] != "pointer" {
            continue;
        }
        let bit = match action["button"].as_str() {
            Some("left") => 1,
            Some("right") => 2,
            Some("middle") => 4,
            _ => 7,
        };
        match action["phase"].as_str() {
            Some("down") => held |= bit,
            Some("up") => held &= !bit,
            _ => {}
        }
    }
    held
}

/// Only adjacent movement with the same held button/modifiers can be
/// superseded; presses, releases and keys always keep their order.
fn replaceable_motion(previous: &Value, next: &Value) -> bool {
    previous["type"] == next["type"]
        && (next["type"] == "move"
            || (next["type"] == "pointer"
                && previous["phase"] == "move"
                && next["phase"] == "move"
                && previous["button"] == next["button"]
                && previous["modifiers"] == next["modifiers"]))
}

/// Wheel events at one observed point accumulate while an earlier request
/// is in flight. Each native scroll deliberately settles the pointer for
/// 50 ms, so replaying every wheel event separately adds avoidable delay.
/// Keep target/frame/direction boundaries, and retain travel beyond the
/// service's 100-line limit as a separate queued action.
fn coalesce_scroll(previous: &mut Value, next: &mut Value) -> bool {
    if previous["type"] != "scroll"
        || next["type"] != "scroll"
        || previous["x"] != next["x"]
        || previous["y"] != next["y"]
    {
        return false;
    }
    let axis = |action: &Value, name: &str| match action.get(name) {
        None => Some(0),
        Some(value) => value.as_i64().filter(|value| (-100..=100).contains(value)),
    };
    let Some((dx, dy, next_dx, next_dy)) = axis(previous, "dx")
        .zip(axis(previous, "dy"))
        .zip(axis(next, "dx").zip(axis(next, "dy")))
        .map(|((dx, dy), (next_dx, next_dy))| (dx, dy, next_dx, next_dy))
    else {
        return false;
    };
    let consistent =
        |before: i64, after: i64| before == 0 || after == 0 || before.signum() == after.signum();
    if !consistent(dx, next_dx) || !consistent(dy, next_dy) {
        return false;
    }
    let combined_dx = dx + next_dx;
    let combined_dy = dy + next_dy;
    previous["dx"] = json!(combined_dx.clamp(-100, 100));
    previous["dy"] = json!(combined_dy.clamp(-100, 100));
    next["dx"] = json!(combined_dx - combined_dx.clamp(-100, 100));
    next["dy"] = json!(combined_dy - combined_dy.clamp(-100, 100));
    next["dx"] == 0 && next["dy"] == 0
}

/// Bound native typing work as well as the number of queued actions.
const INPUT_BATCH_UNITS: usize = 16;

/// Adjacent printable ASCII has the same grapheme boundaries after joining.
/// Unicode stays separate because joining can change native grapheme events.
fn ascii_text(action: &Value) -> Option<&str> {
    action["text"].as_str().filter(|text| {
        action["type"] == "type"
            && !text.is_empty()
            && text.bytes().all(|byte| (b' '..=b'~').contains(&byte))
    })
}

fn merge_text(previous: &mut Value, next: &Value, limit: usize) -> bool {
    let Some((before, after)) = ascii_text(previous).zip(ascii_text(next)) else {
        return false;
    };
    if before.len() + after.len() > limit.min(atlas_core::actions::TEXT_MAX) {
        return false;
    }
    previous["text"] = json!(format!("{before}{after}"));
    true
}

/// Count compacted characters as well as actions, so a short request never
/// grows into sixteen 16-character actions after queue compaction.
fn input_units(action: &Value) -> usize {
    ascii_text(action).map_or(1, str::len)
}

/// Keep a short, ordered request. One aggregate wheel turn is enough:
/// sixteen separate turns would deliberately sleep for at least 800 ms
/// before returning, delaying the next picture and button release.
fn input_batch(queue: &mut VecDeque<(String, Value)>) -> Option<(String, Vec<Value>)> {
    let (frame, first) = queue.pop_front()?;
    let mut scrolling = first["type"] == "scroll";
    let mut gathered = input_units(&first);
    let mut actions = vec![first];
    while gathered < INPUT_BATCH_UNITS {
        let Some((next_frame, next)) = queue.front() else {
            break;
        };
        if next_frame != &frame
            || (scrolling && next["type"] == "scroll")
            || gathered + input_units(next) > INPUT_BATCH_UNITS
        {
            break;
        }
        let (_, action) = queue.pop_front()?;
        scrolling |= action["type"] == "scroll";
        gathered += input_units(&action);
        // Printable ASCII has one grapheme per code point, so concatenating
        // adjacent committed text produces the same native key events and
        // pauses. Leave Unicode/control text and named key boundaries alone.
        if let Some(previous) = actions.last_mut()
            && merge_text(previous, &action, INPUT_BATCH_UNITS)
        {
            continue;
        }
        actions.push(action);
    }
    Some((frame, actions))
}

#[cfg(test)]
mod tests;
