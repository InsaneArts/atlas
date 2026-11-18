//! Targeted input on a VM end to end, through a real keeper and a real
//! client: where an agent's input must land (an app, its window, its
//! field, or an element found by its name or a look) is found in the
//! command's one proof and checked by the guest's input service. The guest
//! is a stand-in that lists one app, Notes, as a machine guest lists its
//! windows and apps, keeps a look of its elements, and refuses input whose
//! expectation it does not meet, sending nothing, as a machine guest's
//! input service does.
use super::tests::{live, proven, restarted};
use crate::{
    agent,
    client::Client,
    host_session::{Host, HostSession},
    vm::keeper::{clients::tests::Serving, tests::Saying},
};
use atlas_core::{
    actions::Button,
    error::Code,
    screen::{select::ElementFilter, wait::State},
};
use atlas_session::lane::{Header, LaneKind};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// The guest's one app: Notes, its window 42, its start as the guest gives it.
const NOTES: i64 = 812;
const WINDOW: i64 = 42;

/// The look the guest's input service keeps of Notes.
const LOOK: &str = "8123abc";

/// Finder, which runs with no window.
const FINDER: i64 = 900;

/// Notes's window, in the guest's display points.
fn bounds() -> Value {
    json!({"x": 10, "y": 20, "width": 400, "height": 300})
}

/// Notes's elements, as a look of them lists them: its window, a button
/// called `button` and a search field.
fn elements(button: &str) -> Value {
    json!([
        {"index": 0, "depth": 0, "role": "AXWindow", "title": "Draft", "window": bounds(),
            "frame": bounds()},
        {"index": 1, "depth": 1, "role": "AXButton", "title": button, "enabled": true,
            "window": bounds(), "frame": {"x": 20, "y": 30, "width": 60, "height": 20}},
        {"index": 2, "depth": 1, "role": "AXTextField", "title": "Search", "enabled": true,
            "window": bounds(), "frame": {"x": 100, "y": 30, "width": 200, "height": 22}},
    ])
}

/// One request the guest heard: on which lane, in which generation.
#[derive(Clone, Debug)]
struct Heard {
    kind: LaneKind,
    generation: u64,
    request: Value,
}

/// The guest, as a test keeps it.
#[derive(Default)]
struct Guest {
    heard: Mutex<Vec<Heard>>,
    /// The start the guest gives Notes, in place of `[123, 456]`.
    start: Mutex<Option<Value>>,
    /// How its input service answers an input it does not take, in place of
    /// checking it.
    refusing: Mutex<Option<Value>>,
    /// Whether Notes is relaunched as another process once its windows
    /// are listed.
    relaunched: Mutex<bool>,
    /// The button Notes shows now, in place of OK.
    button: Mutex<Option<&'static str>>,
    /// The button that takes the place of the one a look saw, once it is
    /// looked at.
    moves_to: Mutex<Option<&'static str>>,
    /// The button the last look saw.
    looked: Mutex<Option<&'static str>>,
    /// How long a read of its windows takes, when a test says.
    windows_take: Mutex<Option<Duration>>,
}

impl Guest {
    /// Notes's start, as the guest gives it now.
    fn start(&self) -> Value {
        self.start
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| json!([123, 456]))
    }

    /// Whether `expect` names Notes as the guest finds it now.
    fn is_notes(&self, expect: &Value) -> bool {
        expect["pid"] == NOTES && expect["start"] == self.start()
    }

    /// Answer `request` on a lane of `kind` in `generation`.
    fn answer(&self, kind: &LaneKind, generation: u64, request: &Value) -> Value {
        self.heard.lock().unwrap().push(Heard {
            kind: kind.clone(),
            generation,
            request: request.clone(),
        });
        let starts = json!({NOTES.to_string(): self.start()});
        match (kind, request["op"].as_str()) {
            (LaneKind::Read, Some("windows")) => {
                if *self.relaunched.lock().unwrap() {
                    *self.start.lock().unwrap() = Some(json!([999, 1]));
                }
                if let Some(takes) = *self.windows_take.lock().unwrap() {
                    std::thread::sleep(takes);
                }
                let windows = vec![json!({"number": WINDOW, "pid": NOTES, "owner": "Notes",
                    "title": "Draft", "layer": 0, "alpha": 1.0, "bounds": bounds()})];
                json!({"ok": true, "starts": starts, "windows": windows})
            }
            (LaneKind::Read, Some("apps")) => json!({"ok": true, "starts": starts,
                "apps": [{"pid": NOTES, "name": "Notes", "active": true},
                    {"pid": FINDER, "name": "Finder"}]}),
            (LaneKind::Input { change: None }, Some("elements")) => {
                if !self.is_notes(&request["expect"]) {
                    return json!({"ok": false, "code": "elements",
                        "error": "That process is no longer running"});
                }
                let shown = self.button.lock().unwrap().unwrap_or("OK");
                *self.looked.lock().unwrap() = Some(shown);
                if let Some(moved) = self.moves_to.lock().unwrap().take() {
                    *self.button.lock().unwrap() = Some(moved);
                }
                json!({"ok": true, "look": LOOK, "elements": elements(shown), "truncated": false,
                    "reasons": [], "errors": {}})
            }
            (LaneKind::Input { change: Some(_) }, Some("acquire")) => json!({"ok": true}),
            (LaneKind::Input { change: Some(_) }, Some("release")) => {
                json!({"ok": true, "released": true})
            }
            (LaneKind::Input { change: Some(_) }, Some("input")) => self.input(request),
            _ => json!({"ok": false, "code": "bad-request",
                "error": "The request is not what its lane was opened for"}),
        }
    }

    /// What its input service answers `request`, an input: refused, nothing
    /// sent, unless what it expects is met; else every action performed,
    /// its expectation echoed as checked, and an action on an element said
    /// to have gone through Accessibility.
    fn input(&self, request: &Value) -> Value {
        if let Some(refused) = self.refusing.lock().unwrap().clone() {
            return refused;
        }
        let expect = &request["expect"];
        let met = expect.is_null()
            || (self.is_notes(expect)
                && (expect["window"].is_null() || expect["window"] == WINDOW)
                && (expect["field"].is_null() || expect["field"] == "Search"));
        if !met {
            return not_in_front();
        }
        // An element named by a look is found again first: one that is no
        // longer what the look saw is refused, nothing sent.
        let now = self.button.lock().unwrap().unwrap_or("OK");
        let seen = *self.looked.lock().unwrap();
        let changed = request["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|action| action["ref"] == format!("{LOOK}.1") && seen != Some(now));
        if changed {
            return json!({"ok": false, "code": "elements",
                "error": "That element changed since it was looked at; nothing was sent",
                "next": "Look again", "performed": 0, "partial": false, "uncertain": false});
        }
        let actions: Vec<atlas_core::actions::Action> =
            serde_json::from_value(request["actions"].clone()).unwrap();
        let mut reply = json!({"ok": true, "app": "Notes", "bundleId": "com.apple.Notes",
            "performed": actions.len()});
        if expect.is_object() {
            reply["expect"] = expect.clone();
        }
        if actions
            .iter()
            .any(|action| action.element_action().is_some())
        {
            reply["mechanism"] = json!("accessibility");
            reply["verification"] = json!("not-checked");
        }
        reply
    }

    /// Every request it heard of operation `op`, in order.
    fn heard(&self, op: &str) -> Vec<Heard> {
        self.heard
            .lock()
            .unwrap()
            .iter()
            .filter(|heard| heard.request["op"] == op)
            .cloned()
            .collect()
    }

    /// The one input it heard.
    fn input_heard(&self) -> Heard {
        let inputs = self.heard("input");
        assert_eq!(inputs.len(), 1, "{inputs:?}");
        inputs[0].clone()
    }
}

/// A machine input service's refusal of input whose app is not in front.
fn not_in_front() -> Value {
    json!({"ok": false, "code": "focus-changed",
        "error": "Another app is in front, so bring Notes forward and look again; nothing was sent",
        "reason": "app", "performed": 0, "partial": false, "uncertain": false})
}

/// A VM whose guest is a [`Guest`], and an agent's client of it.
struct Targeting {
    serving: Serving,
    client: Client,
    guest: Arc<Guest>,
    saying: Arc<Saying>,
    _directory: tempfile::TempDir,
}

async fn targeting() -> Targeting {
    let (serving, saying, client, directory) = proven().await;
    let guest = Arc::new(Guest::default());
    let answering = guest.clone();
    *saying.otherwise.lock().unwrap() = Some(Arc::new(move |header: &Header, request: Value| {
        answering.answer(&header.kind, header.generation.0, &request)
    }));
    Targeting {
        serving,
        client,
        guest,
        saying,
        _directory: directory,
    }
}

fn notes() -> agent::Target {
    agent::Target::Name("Notes".into())
}

/// Notes, and within it, `field` when given.
fn in_notes(window: Option<i64>, field: Option<&str>) -> agent::Expected {
    agent::Expected::new(Some(notes()), window, field.map(str::to_owned))
        .unwrap()
        .unwrap()
}

fn click_at(x: f64, y: f64) -> Value {
    agent::click(x, y, Button::Left, 1, &[])
}

/// What the station holds pending.
fn pending(client: &Client) -> Vec<String> {
    client.controller.pending().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_in_an_app_sends_the_app_its_guest_found_and_the_guest_checks_it() {
    let vm = targeting().await;
    let generation = live(&vm.serving);

    let reply = agent::act(
        &vm.client,
        Some(&in_notes(None, None)),
        &click_at(30.0, 40.0),
    )
    .await
    .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    let input = vm.guest.input_heard();
    assert_eq!(input.generation, generation);
    assert_eq!(
        input.request["expect"],
        json!({"pid": NOTES, "start": [123, 456]})
    );
    let line = serde_json::to_vec(&input.request).unwrap();
    assert!(matches!(
        atlas_native::native::service::classify(&line),
        Some(atlas_native::native::service::Kind::Mutation(Some(_)))
    ));
    // Notes was found among the guest's windows, in the proof's generation
    // and session.
    let windows = vm.guest.heard("windows");
    assert_eq!(windows.len(), 1, "{windows:?}");
    assert_eq!(windows[0].kind, LaneKind::Read);
    assert_eq!(windows[0].generation, generation);
    assert_eq!(
        windows[0].request["session"],
        json!({"securitySessionId": 42, "displayId": 7})
    );
    let ops: Vec<Value> = vm
        .guest
        .heard
        .lock()
        .unwrap()
        .iter()
        .filter(|heard| matches!(heard.kind, LaneKind::Input { change: Some(_) }))
        .map(|heard| heard.request["op"].clone())
        .collect();
    assert_eq!(ops, ["acquire", "input", "release"]);
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_targeted_click_its_guest_refuses_changes_nothing_stays_certain_and_keeps_its_words() {
    let vm = targeting().await;
    *vm.guest.refusing.lock().unwrap() = Some(not_in_front());

    let outcome = agent::act(
        &vm.client,
        Some(&in_notes(None, None)),
        &click_at(30.0, 40.0),
    )
    .await;

    let refused = match outcome {
        Ok(reply) => reply,
        Err(error) => json!(error),
    };
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["code"], "focus-changed", "{refused}");
    assert_eq!(refused["error"], not_in_front()["error"], "{refused}");
    assert_ne!(refused["uncertain"], true, "{refused}");
    assert!(!refused.to_string().contains("helper"), "{refused}");
    let input = vm.guest.input_heard();
    assert_eq!(input.request["expect"]["pid"], NOTES, "its expect was sent");
    assert_eq!(vm.guest.heard("release").len(), 1, "let go of");
    assert!(pending(&vm.client).is_empty(), "nothing left pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_in_an_app_the_guest_no_longer_finds_in_front_is_refused_unsent() {
    let vm = targeting().await;
    let generation = live(&vm.serving);
    // Found as Notes among the guest's windows, and another process by the
    // time its input service checks.
    *vm.guest.relaunched.lock().unwrap() = true;

    let outcome = agent::act(
        &vm.client,
        Some(&in_notes(None, None)),
        &click_at(30.0, 40.0),
    )
    .await;

    let refused = outcome.unwrap_or_else(|error| json!(error));
    assert_eq!(refused["code"], "focus-changed", "{refused}");
    assert_ne!(refused["uncertain"], true, "{refused}");
    let input = vm.guest.input_heard();
    assert_eq!(input.generation, generation);
    assert_eq!(input.request["expect"]["start"], json!([123, 456]));
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apps_elements_are_looked_at_on_the_input_lane_and_one_found_by_name_is_pressed() {
    let vm = targeting().await;
    let generation = live(&vm.serving);
    let asked = agent::Asked {
        role: None,
        names: vec!["OK".into()],
        window: None,
        page: false,
        max: 50,
        form: agent::Form::Compact,
    };

    let listed = agent::elements(&vm.client, &notes(), &asked).await.unwrap();
    let element = &listed["elements"][0];
    let reference = format!("{LOOK}.{}", element["index"]);
    let pressed = agent::element(
        &vm.client,
        Some(&notes()),
        &json!({"type": "press", "ref": reference}),
    )
    .await
    .unwrap();

    assert_eq!(listed["look"], LOOK, "{listed}");
    assert_eq!(element["name"], "OK", "{listed}");
    let looks = vm.guest.heard("elements");
    assert_eq!(looks.len(), 1, "{looks:?}");
    assert_eq!(looks[0].kind, LaneKind::Input { change: None });
    assert_eq!(looks[0].generation, generation);
    assert_eq!(
        looks[0].request["expect"],
        json!({"pid": NOTES, "start": [123, 456]}),
        "the very process the guest gave"
    );
    assert_eq!(pressed["ok"], true, "{pressed}");
    assert_eq!(pressed["performed"], 1, "{pressed}");
    assert_eq!(pressed["mechanism"], "accessibility", "{pressed}");
    let input = vm.guest.input_heard();
    assert_eq!(input.request["actions"][0]["type"], "press");
    assert_eq!(input.request["actions"][0]["ref"], reference);
    assert_eq!(input.request["expect"]["pid"], NOTES);
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_on_an_element_by_its_name_is_the_guests_click_on_that_element_of_its_look() {
    let vm = targeting().await;
    let generation = live(&vm.serving);
    let filter = ElementFilter {
        role: None,
        name: Some("OK".into()),
    };

    let reply = agent::click_element(&vm.client, &notes(), &filter, Button::Left, 1, &[])
        .await
        .unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["element"]["name"], "OK", "{reply}");
    let input = vm.guest.input_heard();
    assert_eq!(
        input.request["expect"],
        json!({"pid": NOTES, "start": [123, 456], "window": WINDOW})
    );
    let action = &input.request["actions"][0];
    assert_eq!(
        action["type"], "reference",
        "never a click at a point: {action}"
    );
    assert_eq!(action["ref"], format!("{LOOK}.1"), "{action}");
    assert!(action.get("x").is_none(), "{action}");
    assert_eq!(
        vm.guest.heard("elements")[0].request["expect"]["start"],
        json!([123, 456])
    );
    for heard in vm.guest.heard.lock().unwrap().iter() {
        assert_eq!(heard.generation, generation, "{heard:?}");
    }
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_control_found_by_name_that_another_takes_the_place_of_before_the_click_is_never_clicked()
{
    let vm = targeting().await;
    *vm.guest.button.lock().unwrap() = Some("Save");
    *vm.guest.moves_to.lock().unwrap() = Some("Delete");
    let filter = ElementFilter {
        role: None,
        name: Some("Save".into()),
    };

    let outcome = agent::click_element(&vm.client, &notes(), &filter, Button::Left, 1, &[]).await;

    let refused = outcome.unwrap_or_else(|error| json!(error));
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["code"], "elements", "{refused}");
    assert_ne!(refused["uncertain"], true, "{refused}");
    assert_ne!(refused["performed"], 1, "{refused}");
    let input = vm.guest.input_heard();
    assert_eq!(input.request["actions"][0]["type"], "reference");
    assert_eq!(input.request["actions"][0]["ref"], format!("{LOOK}.1"));
    assert_eq!(vm.guest.heard("release").len(), 1, "let go of");
    assert!(pending(&vm.client).is_empty(), "nothing left pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn text_typed_into_a_named_field_goes_where_the_guest_checks_its_focus() {
    let vm = targeting().await;

    let typed = agent::type_text(&vm.client, "hi", Some(&in_notes(None, Some("Search"))))
        .await
        .unwrap();

    assert_eq!(typed["ok"], true, "{typed}");
    let input = vm.guest.input_heard();
    assert_eq!(
        input.request["expect"],
        json!({"pid": NOTES, "start": [123, 456], "field": "Search"})
    );
    assert_eq!(input.request["actions"][0]["text"], "hi");
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn text_typed_into_a_field_the_guest_does_not_find_focused_is_not_typed() {
    let vm = targeting().await;

    let typed = agent::type_text(&vm.client, "hi", Some(&in_notes(None, Some("Title"))))
        .await
        .unwrap();

    assert_eq!(typed["ok"], false, "{typed}");
    assert_eq!(typed["code"], "focus-changed", "{typed}");
    assert_eq!(typed["typed"], 0, "{typed}");
    assert_ne!(typed["uncertain"], true, "{typed}");
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiting_for_an_element_by_its_name_finds_it_in_a_kept_look() {
    let vm = targeting().await;
    let filter = ElementFilter {
        role: None,
        name: Some("OK".into()),
    };

    let waited = agent::wait_for_element(
        &vm.client,
        &notes(),
        &filter,
        &State::Exists,
        Duration::ZERO,
    )
    .await
    .unwrap();

    assert_eq!(waited["ok"], true, "{waited}");
    assert_eq!(waited["matched"], true, "{waited}");
    assert_eq!(waited["look"], LOOK, "{waited}");
    assert_eq!(waited["element"]["name"], "OK", "{waited}");
    assert!(vm.guest.heard("input").is_empty(), "waiting sends nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_window_is_inferred_in_the_generation_the_click_was_proved_in() {
    let mut vm = targeting().await;
    vm.client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());

    let listed = agent::list(&vm.client, &agent::WindowFilter::default())
        .await
        .unwrap();
    restarted(&vm.serving).await;
    let generation = live(&vm.serving);
    let reply = agent::act(&vm.client, None, &click_at(30.0, 40.0))
        .await
        .unwrap();

    assert_eq!(listed[0].number, WINDOW, "{listed:?}");
    assert_eq!(reply["ok"], true, "{reply}");
    let input = vm.guest.input_heard();
    assert_eq!(input.generation, generation);
    assert_eq!(
        input.request["expect"],
        json!({"pid": NOTES, "start": [123, 456], "window": WINDOW}),
        "the plain click is held to the window remembered there"
    );
    let windows = vm.guest.heard("windows");
    let resolved = windows.last().unwrap();
    assert_eq!(resolved.generation, generation, "{windows:?}");
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_window_whose_app_the_guest_now_gives_another_start_is_refused_unsent() {
    let mut vm = targeting().await;
    vm.client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
    agent::list(&vm.client, &agent::WindowFilter::default())
        .await
        .unwrap();
    // Notes quit and a new process took its number.
    *vm.guest.start.lock().unwrap() = Some(json!([124, 0]));

    let refused = agent::act(&vm.client, None, &click_at(30.0, 40.0))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Windows, "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert!(vm.guest.heard("acquire").is_empty(), "nothing taken");
    assert!(pending(&vm.client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_named_by_its_process_on_a_vm_is_the_guests_and_one_it_does_not_run_is_said_of_the_vm()
 {
    let vm = targeting().await;
    let by_pid = agent::Expected::new(Some(agent::Target::Pid(NOTES)), None, None)
        .unwrap()
        .unwrap();
    let absent = agent::Expected::new(Some(agent::Target::Pid(999)), None, None)
        .unwrap()
        .unwrap();

    agent::act(&vm.client, Some(&by_pid), &click_at(30.0, 40.0))
        .await
        .unwrap();
    let refused = agent::act(&vm.client, Some(&absent), &click_at(30.0, 40.0))
        .await
        .unwrap_err();

    assert_eq!(
        vm.guest.input_heard().request["expect"],
        json!({"pid": NOTES, "start": [123, 456]}),
        "told apart by the start the guest gave"
    );
    assert_eq!(refused.code, Code::Elements, "{refused:?}");
    assert_eq!(
        refused.error,
        "No app in work has the process ID 999; run atlas windows to see them"
    );
    assert_eq!(
        vm.guest.heard("acquire").len(),
        1,
        "only the first was taken"
    );
}

fn finder() -> agent::WindowFilter {
    agent::WindowFilter {
        app: Some("Finder".into()),
        ..agent::WindowFilter::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wait_for_an_app_with_no_window_on_a_slow_vm_runs_out_as_a_wait_and_never_as_transport() {
    let vm = targeting().await;
    *vm.saying.proof_takes.lock().unwrap() = Some(Duration::from_millis(800));
    *vm.guest.windows_take.lock().unwrap() = Some(Duration::from_millis(700));

    let waited = agent::wait(&vm.client, &finder(), false, Duration::from_secs(2))
        .await
        .unwrap();

    assert_eq!(waited["ok"], false, "{waited}");
    assert_eq!(waited["code"], "wait-failed", "{waited}");
    assert_eq!(
        waited["error"], "Finder is running but shows no window",
        "{waited}"
    );
    assert_eq!(waited["timedOut"], true, "{waited}");
    assert!(
        vm.guest.heard("windows").len() >= 2,
        "looked more than once"
    );
}
