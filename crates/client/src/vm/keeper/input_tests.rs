//! An agent's input on a VM end to end, through a real keeper and a real
//! client, its guest a stand-in whose input service holds one owner at a
//! time to the generation it took input in, as a machine guest's does:
//! each batch taken, sent and let go of as three changes on input lanes,
//! each under a token of its own, and the guest's looks read on its input
//! lane. The person's own control goes the same way, under one owner for
//! as long as they hold it, whatever Stop holds agents.
use super::tests::{admitted, live, proven, record, restarted, settled};
use crate::{
    agent,
    client::Client,
    doing::Doing,
    vm::keeper::{
        clients::tests::{Serving, asked, client_of},
        tests::{Saying, input_hello, until},
    },
};
use atlas_control::{Controller, interrupt::Interrupt};
use atlas_core::{
    actions::{Button, Expect},
    error::Code,
    files::Token,
};
use atlas_session::{lane::LaneKind, standin::StandIn};
use nix::sys::signal::Signal;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, atomic::Ordering::SeqCst},
    time::Duration,
};

/// What the guest's input service still has to be allowed, as its hello
/// says it.
const ACCESSIBILITY: &str =
    "Allow Atlas in Privacy & Security, Accessibility, in the VM's window: agents wait for it.";

/// A hook a test runs as the input service answers an operation.
type Hook = Arc<dyn Fn(&str) + Send + Sync>;

/// One request on an input lane, as the guest's input service heard it.
#[derive(Clone, Debug)]
struct Heard {
    lane: LaneKind,
    generation: u64,
    request: Value,
    /// The changes the station held pending as it was answered.
    pending: Vec<String>,
}

/// A machine guest's input service, as a test keeps it.
#[derive(Default)]
struct Service {
    heard: Mutex<Vec<Heard>>,
    /// The owner holding input, and the generation it took it in.
    owner: Mutex<Option<(String, u64)>>,
    /// Run as each operation is answered, before its answer.
    on: Mutex<Option<Hook>>,
    /// How many acquires to come it refuses as busy with another agent.
    busy: std::sync::atomic::AtomicUsize,
    /// Whether the grant its owner holds ran out: kept until let go of,
    /// but neither renewed nor acted under.
    expired: std::sync::atomic::AtomicBool,
}

impl Service {
    /// Answer `request` on a lane of `kind` in `generation`, as the guest's
    /// input service does, `controller` the station's.
    fn answer(
        &self,
        standin: &StandIn,
        controller: &Controller,
        (kind, generation): (&LaneKind, u64),
        request: &Value,
    ) -> Value {
        let op = request["op"].as_str().unwrap_or_default().to_owned();
        self.heard.lock().unwrap().push(Heard {
            lane: kind.clone(),
            generation,
            request: request.clone(),
            pending: controller.pending().unwrap_or_default(),
        });
        let hook = self.on.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook(&op);
        }
        let owner = request["owner"].as_str().unwrap_or_default().to_owned();
        let mut holding = self.owner.lock().unwrap();
        let owns = holding.as_ref() == Some(&(owner.clone(), generation));
        // The person's own lane is answered as an input lane's change, but
        // for the Stop.
        let person = matches!(kind, LaneKind::Person { .. });
        let kind = match kind {
            LaneKind::Person { change } => &LaneKind::Input {
                change: Some(change.clone()),
            },
            kind => kind,
        };
        match (kind, op.as_str()) {
            // The guest refuses changes while its Stop holds, but on the
            // person's own lane.
            (LaneKind::Input { change: Some(_) }, _)
                if standin.stops.load(SeqCst) > 0 && !person =>
            {
                json!({"ok": false, "code": "stopped",
                    "error": "Agents were stopped; nothing was done"})
            }
            (LaneKind::Input { change: Some(_) }, "acquire")
                if self
                    .busy
                    .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
                    .is_ok() =>
            {
                json!({"ok": false, "code": "busy", "holder": "agent",
                    "error": "The native input service already has a controller"})
            }
            (LaneKind::Input { change: Some(_) }, "acquire") => {
                *holding = Some((owner, generation));
                json!({"ok": true})
            }
            (LaneKind::Input { change: Some(_) }, "input" | "renew")
                if owns && self.expired.load(SeqCst) =>
            {
                json!({"ok": false, "code": "lease-lost",
                    "error": "Native control was released or expired"})
            }
            (LaneKind::Input { change: Some(_) }, "renew") if owns => json!({"ok": true}),
            (LaneKind::Input { change: Some(_) }, "input") if owns => {
                let performed = request["actions"].as_array().map_or(0, Vec::len);
                let mut reply =
                    json!({"ok": true, "performed": performed, "app": null, "bundleId": null});
                // As a machine guest's says an action on an element went.
                let actions: Vec<atlas_core::actions::Action> =
                    serde_json::from_value(request["actions"].clone()).unwrap_or_default();
                if actions
                    .iter()
                    .any(|action| action.element_action().is_some())
                {
                    reply["mechanism"] = json!("accessibility");
                    reply["verification"] = json!("not-checked");
                }
                reply
            }
            (LaneKind::Input { change: Some(_) }, "release") if owns => {
                *holding = None;
                json!({"ok": true, "released": true})
            }
            (LaneKind::Input { change: Some(_) }, "input" | "renew" | "release") => {
                json!({"ok": false, "code": "lease-lost",
                    "error": "This token cannot release another controller"})
            }
            (LaneKind::Input { change: None }, "elements") => {
                json!({"ok": true, "look": "8123abc", "elements": [], "truncated": false,
                    "reasons": [], "errors": {}})
            }
            _ => json!({"ok": false, "code": "bad-request",
                "error": "The request is not what its lane was opened for"}),
        }
    }

    /// Every request it heard on a lane of a change, in order.
    fn changes(&self) -> Vec<Heard> {
        self.heard
            .lock()
            .unwrap()
            .iter()
            .filter(|heard| {
                matches!(
                    heard.lane,
                    LaneKind::Input { change: Some(_) } | LaneKind::Person { .. }
                )
            })
            .cloned()
            .collect()
    }

    /// Run `hook` as each operation is answered from now.
    fn on(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
        *self.on.lock().unwrap() = Some(Arc::new(hook));
    }
}

/// The guest of `serving`, saying what `saying` says, its input service
/// kept by the test, and ready for agents.
fn serving_input(serving: &Serving, saying: &Saying) -> Arc<Service> {
    let service = Arc::new(Service::default());
    let (kept, standin, controller) = (
        service.clone(),
        serving.fixture.guest.standin.clone(),
        serving.fixture.keeper.stop.clone(),
    );
    *saying.otherwise.lock().unwrap() = Some(Arc::new(move |header, request| {
        kept.answer(
            &standin,
            &controller,
            (&header.kind, header.generation.0),
            &request,
        )
    }));
    service
}

/// The change a request heard on a lane of a change names, which must be
/// its lane's own.
fn change_of(heard: &Heard) -> Token {
    let (LaneKind::Input {
        change: Some(change),
    }
    | LaneKind::Person { change }) = &heard.lane
    else {
        panic!("not a change: {heard:?}");
    };
    assert_eq!(heard.request["change"], json!(change), "{heard:?}");
    change.clone()
}

/// The change the guest holds that its input service has not heard yet,
/// the owners it was asked for apart.
fn held_unheard(standin: &StandIn, service: &Service) -> Option<Token> {
    let heard = service.changes();
    let owners: Vec<&str> = heard
        .iter()
        .filter_map(|heard| heard.request["owner"].as_str())
        .collect();
    let changes: Vec<Token> = heard.iter().map(change_of).collect();
    standin
        .changes
        .lock()
        .unwrap()
        .iter()
        .find(|(change, record)| {
            record.is_none() && !changes.contains(change) && !owners.contains(&change.as_str())
        })
        .map(|(change, _)| change.clone())
}

/// The owner the guest's input service heard input taken for.
fn owner_of(service: &Service) -> Token {
    Token::parse(service.changes()[0].request["owner"].as_str().unwrap()).unwrap()
}

/// What the guest keeps once it proves the work of a change ended.
fn proven_ended() -> Value {
    json!({"ok": false, "code": "uncertain", "error": "Its work is proven gone",
        "uncertain": true, "ended": true})
}

/// The station's pending changes, in order.
fn pending(client: &Client) -> Vec<String> {
    let mut pending = client.controller.pending().unwrap();
    pending.sort();
    pending
}

/// `tokens`, as the station names its pending changes, in order.
fn named(tokens: &[&Token]) -> Vec<String> {
    let mut named: Vec<String> = tokens
        .iter()
        .map(|token| token.as_str().to_owned())
        .collect();
    named.sort();
    named
}

fn click_at(x: f64, y: f64) -> Value {
    agent::click(x, y, Button::Left, 1, &[])
}

async fn clicked(client: &Client) -> atlas_core::error::Result<Value> {
    agent::act(client, None, &click_at(10.0, 10.0)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_on_a_vm_without_accessibility_is_refused_in_the_guests_words_and_nothing_is_claimed()
{
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["input"]["held"] = json!(ACCESSIBILITY);
        hello["permissions"]["accessibility"] = json!(false);
    });
    let mut unready = input_hello();
    unready["ready"] = json!(false);
    unready["accessibility"] = json!(false);
    *saying.input.lock().unwrap() = Some(unready);
    let before = admitted(&serving).len();

    let refused = [
        clicked(&client).await.unwrap_err(),
        agent::act(&client, None, &agent::keys(&["cmd+s".to_owned()]).unwrap())
            .await
            .unwrap_err(),
        agent::act(&client, None, &agent::move_to(1.0, 2.0))
            .await
            .unwrap_err(),
    ];
    // Typing reports how much it typed: none.
    let typed = agent::type_text(&client, "hi", None).await.unwrap();

    let allow = "Ask the person to open work's window with atlas desktop maintain work and allow Atlas \
         in Privacy & Security, Accessibility, there.";
    for refused in refused {
        assert_eq!(refused.code, Code::Permission, "{refused:?}");
        assert_eq!(refused.error, ACCESSIBILITY);
        assert_eq!(refused.details["next"], allow, "{refused:?}");
        assert!(!refused.uncertain);
    }
    assert_eq!(typed["next"], allow, "{typed}");
    assert_eq!(typed["ok"], false, "{typed}");
    assert_eq!(typed["code"], "permission", "{typed}");
    assert_eq!(typed["error"], ACCESSIBILITY);
    assert_eq!(typed["typed"], 0);
    assert_ne!(typed["uncertain"], true);
    assert!(service.changes().is_empty(), "nothing sent");
    assert!(
        admitted(&serving)[before..]
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Input { change: Some(_) })),
        "no input lane opened"
    );
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_takes_sends_and_lets_go_of_input_under_one_owner_each_its_own_change_and_claim() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);

    let reply = clicked(&client).await.unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["performed"], 1, "{reply}");
    let heard = service.changes();
    let ops: Vec<&str> = heard
        .iter()
        .map(|heard| heard.request["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["acquire", "input", "release"]);
    let owner = heard[0].request["owner"].as_str().unwrap().to_owned();
    let mut changes = Vec::new();
    for heard in &heard {
        assert_eq!(heard.request["owner"], owner, "one owner throughout");
        assert_eq!(heard.generation, generation, "the proof's generation");
        let change = change_of(heard);
        let mut pending = heard.pending.clone();
        pending.sort();
        let mut claimed = vec![owner.clone(), change.as_str().to_owned()];
        claimed.sort();
        assert_eq!(
            pending, claimed,
            "the owner's claim throughout, and its own claim by its own token"
        );
        changes.push(change);
    }
    changes.dedup();
    assert_eq!(changes.len(), 3, "a change token for each");
    let epoch = &heard[0].request["epoch"];
    assert_eq!(epoch["uid"], 502);
    assert_eq!(epoch["securitySessionId"], 42);
    assert_eq!(epoch["processId"], 456, "the guest's input service");
    assert_eq!(&heard[1].request["epoch"], epoch);
    assert_eq!(heard[0].request["person"], false);
    // Each is what the guest's own input lane takes as a change naming that
    // lane's token, held to an epoch and actions as its service reads them.
    for (heard, change) in heard.iter().zip(&changes) {
        let line = serde_json::to_vec(&heard.request).unwrap();
        assert_eq!(
            atlas_native::native::service::classify(&line),
            Some(atlas_native::native::service::Kind::Mutation(Some(
                change.clone()
            ))),
            "{heard:?}"
        );
    }
    serde_json::from_value::<atlas_core::session::InputEpoch>(epoch.clone()).unwrap();
    serde_json::from_value::<Vec<atlas_core::actions::Action>>(heard[1].request["actions"].clone())
        .unwrap();
    assert!(client.controller.pending().unwrap().is_empty());
    let owner = Token::parse(&owner).unwrap();
    assert!(
        !changes.contains(&owner),
        "the owner is none of its changes"
    );
    for change in changes.iter().chain([&owner]) {
        until("settled with the guest", || {
            settled(&serving).contains(change)
        })
        .await;
    }
    assert!(service.owner.lock().unwrap().is_none(), "let go of");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keys_typed_text_a_move_a_drag_and_a_scroll_each_go_under_an_owner_of_their_own() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);

    let replies = [
        agent::act(&client, None, &agent::keys(&["cmd+s".to_owned()]).unwrap())
            .await
            .unwrap(),
        agent::act(&client, None, &agent::move_to(1.0, 2.0))
            .await
            .unwrap(),
        agent::act(
            &client,
            None,
            &agent::drag((1.0, 1.0), (5.0, 5.0), Button::Left),
        )
        .await
        .unwrap(),
        agent::act(&client, None, &agent::scroll((3.0, 3.0), (0, 2)))
            .await
            .unwrap(),
        agent::type_text(&client, "hello", None).await.unwrap(),
    ];

    for reply in &replies {
        assert_eq!(reply["ok"], true, "{reply}");
    }
    assert_eq!(replies[4]["typed"], 5, "{}", replies[4]);
    let heard = service.changes();
    assert_eq!(heard.len(), 15, "{heard:?}");
    for batch in heard.chunks(3) {
        let ops: Vec<&str> = batch
            .iter()
            .map(|heard| heard.request["op"].as_str().unwrap())
            .collect();
        assert_eq!(ops, ["acquire", "input", "release"]);
        assert!(
            batch
                .iter()
                .all(|heard| heard.request["owner"] == batch[0].request["owner"])
        );
    }
    let owners: std::collections::BTreeSet<String> = heard
        .iter()
        .map(|heard| heard.request["owner"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(owners.len(), 5, "an owner for each command");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_inputs_lost_reply_is_recovered_from_what_the_guest_kept() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let standin = serving.fixture.guest.standin.clone();
    service.on(move |op| {
        if op == "input" {
            standin.dropping.store(1, SeqCst);
        }
    });

    let reply = clicked(&client).await.unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["performed"], 1, "{reply}");
    let heard = service.changes();
    assert_eq!(heard.len(), 3, "sent once: {heard:?}");
    let input = change_of(&heard[1]);
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&input)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_release_is_recovered_from_what_the_guest_kept() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let standin = serving.fixture.guest.standin.clone();
    service.on(move |op| {
        if op == "release" {
            standin.dropping.store(1, SeqCst);
        }
    });

    let reply = clicked(&client).await.unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    let heard = service.changes();
    assert_eq!(heard.len(), 3, "let go of once: {heard:?}");
    assert!(service.owner.lock().unwrap().is_none());
    let release = change_of(&heard[2]);
    assert!(client.controller.pending().unwrap().is_empty());
    until("settled with the guest", || {
        settled(&serving).contains(&release)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_mid_input_leaves_it_uncertain_and_pending() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let standin = serving.fixture.guest.standin.clone();
    let holding = standin.clone();
    // Taken, then held before its input is carried out.
    service.on(move |op| {
        if op == "acquire" {
            holding.holding.store(true, SeqCst);
        }
    });
    let stopping = async {
        until("its input asked for", || {
            held_unheard(&standin, &service).is_some()
        })
        .await;
        client.controller.note_stop(true).unwrap();
        let changes = || standin.changes.lock().unwrap().clone();
        let cut = |reply: &Option<Value>| {
            reply
                .as_ref()
                .is_some_and(|reply| reply["code"] == "stopped")
        };
        until("its input cut short", || changes().values().any(cut)).await;
        standin.holding.store(false, SeqCst);
        changes()
            .into_iter()
            .find(|(_, record)| cut(record))
            .unwrap()
            .0
    };

    let (stopped, input) = tokio::join!(clicked(&client), stopping);

    let stopped = stopped.unwrap_err();

    assert!(stopped.uncertain, "{stopped:?}");
    assert!(
        stopped.details["next"]
            .as_str()
            .is_some_and(|next| next.contains("stays pending")),
        "{stopped:?}"
    );
    assert!(!stopped.error.contains("helper"), "{stopped:?}");
    // The release the guest refused under its Stop let go of nothing for
    // certain: the owner's claim stays with the input's.
    let owner = owner_of(&service);
    assert_eq!(pending(&client), named(&[&input, &owner]));
    assert_eq!(
        record(&serving, &input).flatten().unwrap()["uncertain"],
        true
    );
    assert!(!settled(&serving).contains(&input), "never settled");
    assert!(!settled(&serving).contains(&owner), "never settled");
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_in_a_generation_that_ended_is_refused_unsaid_and_its_claim_cleared() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let verified = client.verify_vm().await.unwrap();
    let proved = verified.machine().unwrap().generation;
    restarted(&serving).await;

    let ownership = client.controller.acquire_beside_commands().unwrap();
    let refused = client
        .input_owned_in(
            &ownership,
            atlas_core::actions::Batch::decode(&click_at(10.0, 10.0)),
            &verified,
            None,
        )
        .await
        .unwrap_err();
    drop(ownership);

    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
    assert!(!refused.uncertain, "{refused:?}");
    assert!(service.changes().is_empty(), "nothing said to the guest");
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Input { change: Some(_) })),
        "no input lane opened in either generation"
    );
    assert_ne!(live(&serving), proved);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_started_anew_between_taking_input_and_sending_it_never_gets_the_input() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let (taken, go_on) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(std::sync::Barrier::new(2)),
    );
    let (told, waiting) = (taken.clone(), go_on.clone());
    // The input service answers the acquire only once the guest started
    // anew, as if the restart came between the two.
    service.on(move |op| {
        if op == "acquire" {
            told.notify_one();
            waiting.wait();
        }
    });
    let restarting = async {
        taken.notified().await;
        restarted(&serving).await;
        *service.on.lock().unwrap() = None;
        go_on.wait();
    };

    let (reply, ()) = tokio::join!(clicked(&client), restarting);

    // The release could not be said in the generation the owner took input
    // in: whether its input service let go of it yet is not known.
    let unknown = reply.unwrap_err();
    assert!(unknown.uncertain, "{unknown:?}");
    assert!(unknown.error.contains("has ended"), "{unknown:?}");
    assert!(!unknown.error.contains("helper"), "{unknown:?}");
    let ops: Vec<String> = service
        .changes()
        .iter()
        .map(|heard| heard.request["op"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ops, ["acquire"], "the input never reached the guest");
    assert!(
        admitted(&serving).iter().all(|header| {
            !matches!(header.kind, LaneKind::Input { change: Some(_) })
                || header.generation.0 == generation
        }),
        "nothing went in the new generation"
    );
    let owner = owner_of(&service);
    assert_eq!(
        pending(&client),
        named(&[&owner]),
        "the owner's claim alone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_a_signal_cut_short_keeps_its_owner_held_until_the_guest_proves_it_let_go() {
    let (serving, saying, mut client, directory) = proven().await;
    let interrupt = Interrupt::new_for_test(Duration::from_millis(200));
    client.controller = client.controller.clone().interrupted_by(interrupt.clone());
    let service = serving_input(&serving, &saying);
    let standin = serving.fixture.guest.standin.clone();
    let holding = standin.clone();
    // Its input answered, then its release held before it is carried out.
    service.on(move |op| {
        if op == "input" {
            holding.holding.store(true, SeqCst);
        }
    });
    let carried = || held_unheard(&standin, &service);
    let signalling = async {
        until("its release asked for", || {
            service.changes().len() == 2 && carried().is_some()
        })
        .await;
        interrupt.ask_for_test(Signal::SIGTERM);
        carried().unwrap()
    };

    let (cut, release) = tokio::join!(clicked(&client), signalling);

    let cut = cut.unwrap_err();

    assert_eq!(cut.code, Code::Interrupted, "{cut:?}");
    assert!(cut.uncertain, "{cut:?}");
    let owner = owner_of(&service);
    assert_eq!(pending(&client), named(&[&release, &owner]));
    // The guest cuts it short too, as its lane went: nothing proves it let go.
    until("its release cut short on the guest too", || {
        record(&serving, &release).is_some_and(|record| record.is_some())
    })
    .await;
    standin.holding.store(false, SeqCst);
    *service.on.lock().unwrap() = None;
    assert_eq!(
        record(&serving, &release).flatten().unwrap()["uncertain"],
        true
    );
    // Its owner is kept: no other agent's input goes meanwhile.
    let other = client_of(&serving, directory.path()).doing(Doing::new("click"));
    let held = clicked(&other).await.unwrap_err();
    assert_eq!(held.code, Code::Uncertain, "{held:?}");
    assert_eq!(service.changes().len(), 2, "only its own acquire and input");
    assert!(service.owner.lock().unwrap().is_some(), "never let go of");
    let kept = other.recover_helper().await.unwrap();
    assert_eq!(kept["cleared"], json!([]), "{kept}");
    assert_eq!(pending(&other), named(&[&release, &owner]));

    // The release proven ended clears its own claim alone: the owner's
    // waits for proof of its own.
    standin.done(&release, &proven_ended());
    let recovered = other.recover_helper().await.unwrap();
    assert_eq!(
        recovered["cleared"],
        json!([{"change": release, "by": "guest"}]),
        "{recovered}"
    );
    assert_eq!(pending(&other), named(&[&owner]));
    assert_eq!(clicked(&other).await.unwrap_err().code, Code::Uncertain);

    // The guest proves its owner let go of, as its input service says it
    // holds nothing: recovery clears it, and input goes again.
    *service.owner.lock().unwrap() = None;
    standin.done(&owner, &proven_ended());
    let recovered = other.recover_helper().await.unwrap();
    assert_eq!(
        recovered["cleared"],
        json!([{"change": owner, "by": "guest"}]),
        "{recovered}"
    );
    assert!(other.controller.pending().unwrap().is_empty());
    let after = clicked(&other).await.unwrap();
    assert_eq!(after["ok"], true, "{after}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_gone_once_its_pointer_went_down_leaves_its_owner_claimed_until_the_guest_proves_it_let_go()
 {
    let (serving, saying, client, directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let standin = serving.fixture.guest.standin.clone();
    let holding = standin.clone();
    // The pointer is down, then its release held before it is carried out.
    service.on(move |op| {
        if op == "input" {
            holding.holding.store(true, SeqCst);
        }
    });
    let down = json!([{"type": "pointer", "phase": "down", "x": 10.0, "y": 10.0}]);
    let release_asked = async {
        until("its release asked for", || {
            service.changes().len() == 2 && held_unheard(&standin, &service).is_some()
        })
        .await;
        held_unheard(&standin, &service).unwrap()
    };

    // The command is gone there, as if it was killed.
    let release = tokio::select! {
        ended = agent::act(&client, None, &down) => panic!("it ended: {ended:?}"),
        release = release_asked => release,
    };

    let owner = owner_of(&service);
    assert_eq!(service.changes()[1].request["actions"][0]["phase"], "down");
    assert!(pending(&client).contains(&owner.as_str().to_owned()));
    until("its release cut short on the guest", || {
        record(&serving, &release).is_some_and(|record| record.is_some())
    })
    .await;
    standin.holding.store(false, SeqCst);
    *service.on.lock().unwrap() = None;
    let other = client_of(&serving, directory.path()).doing(Doing::new("click"));
    let held = clicked(&other).await.unwrap_err();
    assert_eq!(held.code, Code::Uncertain, "{held:?}");
    let kept = other.recover_helper().await.unwrap();
    assert!(
        pending(&other).contains(&owner.as_str().to_owned()),
        "{kept}"
    );
    assert_eq!(service.changes().len(), 2, "nothing more reached the guest");

    // Once the guest proves the owner let go of, recovery clears it.
    *service.owner.lock().unwrap() = None;
    standin.done(&release, &proven_ended());
    standin.done(&owner, &proven_ended());
    other.recover_helper().await.unwrap();
    assert!(pending(&other).is_empty());
    assert_eq!(clicked(&other).await.unwrap()["ok"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_release_refused_for_certain_or_never_said_leaves_its_owner_claimed() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    // Its input service lost the owner before the release came.
    let losing = service.clone();
    service.on(move |op| {
        if op == "input" {
            *losing.owner.lock().unwrap() = None;
        }
    });

    let refused = clicked(&client).await.unwrap_err();

    assert!(refused.uncertain, "{refused:?}");
    assert!(!refused.error.contains("helper"), "{refused:?}");
    let owner = owner_of(&service);
    assert_eq!(
        pending(&client),
        named(&[&owner]),
        "the owner's claim alone"
    );

    // A keeper that cannot be reached for the release keeps it too.
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let socket = serving.socket.clone();
    service.on(move |op| {
        if op == "input" {
            std::fs::remove_file(&socket).unwrap();
        }
    });

    let unsaid = clicked(&client).await.unwrap_err();

    assert!(unsaid.uncertain, "{unsaid:?}");
    let heard = service.changes();
    assert_eq!(heard.len(), 2, "the release never reached the guest");
    assert_eq!(
        pending(&client),
        named(&[&owner_of(&service)]),
        "the owner's claim alone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_naming_an_element_goes_whole_under_an_owner_of_its_own() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let batches = [
        json!([{"type": "reference", "ref": "8123abc.12"}]),
        json!([{"type": "press", "ref": "8123abc.12"}]),
        json!([{"type": "set-value", "ref": "8123abc.12", "value": "x"},
            {"type": "click", "x": 1.0, "y": 1.0}]),
        json!([{"type": "pick", "ref": "8123abc.12", "option": "8123abc.13"}]),
    ];

    for batch in &batches {
        let reply = agent::act(&client, None, batch).await.unwrap();
        assert_eq!(reply["ok"], true, "{batch}: {reply}");
    }

    let sent: Vec<Heard> = service
        .changes()
        .into_iter()
        .filter(|heard| heard.request["op"] == "input")
        .collect();
    assert_eq!(sent.len(), batches.len());
    for (heard, batch) in sent.iter().zip(&batches) {
        assert_eq!(heard.generation, generation);
        for (action, asked) in heard.request["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(batch.as_array().unwrap())
        {
            assert_eq!(action["type"], asked["type"], "{heard:?}");
            assert_eq!(action["ref"], asked["ref"], "{heard:?}");
            assert_eq!(action["option"], asked["option"], "{heard:?}");
        }
        let line = serde_json::to_vec(&heard.request).unwrap();
        assert!(
            matches!(
                atlas_native::native::service::classify(&line),
                Some(atlas_native::native::service::Kind::Mutation(Some(_)))
            ),
            "{heard:?}"
        );
    }
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_its_guest_could_not_act_on_or_check_is_refused_before_anything_is_claimed() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let mut older = input_hello();
    older["capabilities"] = json!(
        atlas_native::native::service::CAPABILITIES
            .iter()
            .filter(|name| {
                ![
                    atlas_core::session::ELEMENT_ACTIONS,
                    atlas_core::session::FOCUS_WINDOW,
                ]
                .contains(name)
            })
            .collect::<Vec<_>>()
    );
    *saying.input.lock().unwrap() = Some(older);
    let before = admitted(&serving).len();
    let in_window = Expect::new(812).unwrap().in_window(42).unwrap();

    let on_element = agent::act(
        &client,
        None,
        &json!([{"type": "press", "ref": "8123abc.12"}]),
    )
    .await
    .unwrap_err();
    // A ref after what would already have been sent is refused by its batch.
    let misordered = agent::act(
        &client,
        None,
        &json!([{"type": "click", "x": 1.0, "y": 1.0},
            {"type": "reference", "ref": "8123abc.12"}]),
    )
    .await
    .unwrap_err();
    let verified = client.verify_vm().await.unwrap();
    let ownership = client.controller.acquire_beside_commands().unwrap();
    let unchecked = client
        .input_owned_in(
            &ownership,
            atlas_core::actions::Batch::decode(&click_at(1.0, 1.0)),
            &verified,
            Some(in_window),
        )
        .await
        .unwrap_err();
    drop(ownership);
    let refused = [on_element, unchecked, misordered];

    assert_eq!(refused[0].code, Code::Update, "{:?}", refused[0]);
    assert!(
        refused[0].error.contains("act on elements"),
        "{:?}",
        refused[0]
    );
    assert_eq!(refused[1].code, Code::Update, "{:?}", refused[1]);
    assert!(
        refused[1].error.contains("check where input lands"),
        "{:?}",
        refused[1]
    );
    assert_eq!(refused[2].code, Code::Input, "{:?}", refused[2]);
    for refused in &refused {
        assert!(!refused.uncertain, "{refused:?}");
        assert!(!refused.error.contains("helper"), "{refused:?}");
    }
    assert!(service.changes().is_empty(), "nothing sent");
    assert!(
        admitted(&serving)[before..]
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Input { change: Some(_) })),
        "no input lane opened"
    );
    assert!(pending(&client).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_look_at_an_apps_elements_goes_on_the_input_lane_in_the_proofs_generation_and_epoch() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let verified = client.verify_vm().await.unwrap();

    let looked = client
        .look_at(&verified, &Expect::new(812).unwrap())
        .await
        .unwrap();

    assert_eq!(looked["look"], "8123abc", "{looked}");
    let heard = service.heard.lock().unwrap().clone();
    let looks: Vec<&Heard> = heard
        .iter()
        .filter(|heard| heard.request["op"] == "elements")
        .collect();
    assert_eq!(looks.len(), 1, "{heard:?}");
    assert_eq!(looks[0].lane, LaneKind::Input { change: None });
    assert_eq!(looks[0].generation, generation);
    assert_eq!(
        looks[0].request["epoch"],
        json!(verified.machine().unwrap().epoch.input_epoch())
    );
    assert_eq!(looks[0].request["expect"]["pid"], 812);
    let line = serde_json::to_vec(&looks[0].request).unwrap();
    assert_eq!(
        atlas_native::native::service::classify(&line),
        Some(atlas_native::native::service::Kind::Read)
    );
    assert!(client.controller.pending().unwrap().is_empty());

    // In a generation that ended, nothing is read.
    restarted(&serving).await;
    let refused = client
        .look_at(&verified, &Expect::new(812).unwrap())
        .await
        .unwrap_err();
    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_look_is_read_on_the_input_lane_and_every_other_read_there_is_refused() {
    let (serving, saying, client, _directory) = proven().await;
    let _service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let before = admitted(&serving).len();
    let read = |request: Value| json!({"op": "read", "lane": "input", "generation": generation, "request": request});
    let epoch = json!({"uid": 502});

    for request in [
        json!({"op": "hello"}),
        json!({"op": "acquire", "owner": "x", "epoch": epoch}),
        json!({"op": "reset", "epoch": epoch}),
        json!({"op": "hold", "on": false}),
        json!({"op": "elements", "epoch": epoch}),
    ] {
        let answered = asked(&serving.socket, &read(request.clone())).await;
        assert_eq!(answered[0]["code"], "denied", "{request}: {answered:?}");
    }
    let lane = json!({"op": "read", "lane": "elsewhere", "request": {"op": "hello"}});
    assert_eq!(asked(&serving.socket, &lane).await[0]["code"], "denied");
    let refused = client
        .link
        .connect_change(
            &crate::link::Route::Input(None),
            &json!({"op": "reset", "epoch": epoch}),
            generation,
        )
        .await
        .err()
        .unwrap();
    assert_eq!(refused.error, client.unreachable().error);
    assert!(
        admitted(&serving)[before..]
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Input { .. })),
        "no input lane opened"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_input_is_refused_in_the_one_wording() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let lease = atlas_control::SharedLease::default();
    let epoch = client.verify_vm().await.unwrap().session().epoch.clone();

    let person = client
        .input(
            &click_at(1.0, 1.0),
            Some((&epoch, std::time::Instant::now())),
            Some((&lease, "person")),
        )
        .await
        .unwrap_err();

    assert_eq!(person.error, client.unreachable().error, "{person:?}");
    assert!(service.changes().is_empty());
    assert!(client.controller.pending().unwrap().is_empty());
}

/// Take input on the guest of `serving` for a new owner, its claim held on
/// the station as a command would hold it, through the keeper's own
/// protocol; then the guest acknowledges the owner let go, as a release it
/// answered would leave it. The owner, and its claim.
async fn owner_let_go(serving: &Serving) -> (Token, atlas_control::Pending) {
    let (owner, change) = (Token::new(), Token::new());
    let claim = serving
        .fixture
        .keeper
        .stop
        .quarantine_by(owner.as_str(), atlas_control::executors::Executor::Guest)
        .unwrap();
    let acquire = json!({"op": "change", "lane": "input", "change": change,
        "generation": live(serving),
        "request": {"op": "acquire", "owner": owner, "epoch": {"uid": 502},
            "person": false, "change": change}});
    let answered = asked(&serving.socket, &acquire).await;
    assert_eq!(answered.last().unwrap()["ok"], true, "{answered:?}");
    let standin = &serving.fixture.guest.standin;
    assert!(
        matches!(record(serving, &owner), Some(None)),
        "kept by the guest"
    );
    standin.done(&owner, &json!({"ok": true, "released": true}));
    (owner, claim)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_owner_whose_command_went_between_clearing_and_settling_is_settled_on_a_later_tick() {
    let (serving, saying, _client, _directory) = proven().await;
    let _service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let (owner, claim) = owner_let_go(&serving).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!settled(&serving).contains(&owner), "kept while claimed");

    // Its command cleared the claim and went before it told the guest.
    serving.fixture.keeper.stop.clear_if(&claim).unwrap();
    drop(claim);

    until("settled on a later tick", || {
        settled(&serving).contains(&owner)
    })
    .await;
    assert_eq!(record(&serving, &owner), None, "the guest forgot it");
    assert_eq!(live(&serving), generation, "with no reconnect");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_owner_whose_settling_met_a_busy_marker_is_settled_on_a_later_tick() {
    let (serving, saying, _client, _directory) = proven().await;
    let _service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let stop = serving.fixture.keeper.stop.clone();
    let (owner, claim) = owner_let_go(&serving).await;
    stop.clear_if(&claim).unwrap();
    drop(claim);
    // The marker's lock held by another process's control.
    let _other = stop
        .quarantine_by(
            Token::new().as_str(),
            atlas_control::executors::Executor::Guest,
        )
        .unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(stop.marker_lock_path())
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();

    let told = asked(
        &serving.socket,
        &json!({"op": "settle", "changes": [owner]}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let kept_while_busy = !settled(&serving).contains(&owner);
    fs2::FileExt::unlock(&lock).unwrap();

    assert_eq!(told[0]["code"], "busy", "{told:?}");
    assert!(kept_while_busy);
    until("settled on a later tick", || {
        settled(&serving).contains(&owner)
    })
    .await;
    assert_eq!(record(&serving, &owner), None, "the guest forgot it");
    assert_eq!(live(&serving), generation, "with no reconnect");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_busy_acquire_is_tried_again_under_a_new_owner_and_the_refused_one_is_settled() {
    let (serving, saying, mut client, _directory) = proven().await;
    client.controller = client.controller.clone().waiting(Duration::from_secs(5));
    let service = serving_input(&serving, &saying);
    service.busy.store(1, SeqCst);

    let reply = clicked(&client).await.unwrap();

    assert_eq!(reply["ok"], true, "{reply}");
    let heard = service.changes();
    let ops: Vec<&str> = heard
        .iter()
        .map(|heard| heard.request["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["acquire", "acquire", "input", "release"]);
    let (refused, taken) = (
        Token::parse(heard[0].request["owner"].as_str().unwrap()).unwrap(),
        Token::parse(heard[1].request["owner"].as_str().unwrap()).unwrap(),
    );
    assert_ne!(refused, taken, "each try under an owner of its own");
    assert_ne!(change_of(&heard[0]), change_of(&heard[1]));
    assert!(pending(&client).is_empty());
    for owner in [&refused, &taken] {
        until("settled with the guest", || {
            settled(&serving).contains(owner)
        })
        .await;
        assert_eq!(record(&serving, owner), None, "nothing left of it");
    }
}

/// A lease of the person's own on `client`'s controller, taken beside no
/// agent's command as the viewer takes one, and its token.
fn persons_lease(client: &Client) -> (atlas_control::SharedLease, String) {
    let lease = Arc::new(tokio::sync::Mutex::new(atlas_control::Lease::new(
        Duration::from_secs(10),
    )));
    let token = lease
        .try_lock()
        .unwrap()
        .acquire_beside(&client.controller, atlas_control::Beside::Commands)
        .unwrap();
    (lease, token)
}

/// A click the person sends, as the viewer passes it on.
fn persons_click() -> Value {
    json!([{"type": "click", "x": 10, "y": 10}])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_control_of_a_vm_goes_under_one_owner_of_their_lease_in_the_proofs_generation()
{
    let (serving, saying, mut client, _directory) = proven().await;
    client.actor = crate::client::Actor::Viewer;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let (lease, owner) = persons_lease(&client);
    let capability = &serving.capability;

    let proof = client.person_proof_on_vm().await.unwrap();
    let taken = client
        .person_acquire_on_vm(&owner, &proof, capability)
        .await
        .unwrap();
    let sent = client
        .person_input_on_vm(
            (&owner, taken.generation()),
            (&persons_click(), proof.epoch(), std::time::Instant::now()),
            &lease,
            capability,
        )
        .await
        .unwrap();
    let kept = client
        .person_renew_on_vm(&owner, taken.generation(), capability)
        .await
        .unwrap();
    let held = pending(&client);
    let released = client
        .person_release_on_vm(taken, capability)
        .await
        .unwrap();

    assert_eq!(sent["performed"], 1, "{sent}");
    assert_eq!(kept["ok"], true, "{kept}");
    assert_eq!(released["released"], true, "{released}");
    let heard = service.changes();
    let ops: Vec<&str> = heard
        .iter()
        .map(|heard| heard.request["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["acquire", "input", "renew", "release"]);
    for heard in &heard {
        assert_eq!(heard.request["owner"], owner, "the lease's own token");
        assert_eq!(heard.request["person"], true, "{heard:?}");
        assert_eq!(heard.generation, generation, "the proof's generation");
        let change = change_of(heard);
        let mut pending = heard.pending.clone();
        pending.sort();
        assert_eq!(pending, named(&[&Token::parse(&owner).unwrap(), &change]));
    }
    assert_eq!(
        heard[1].request["app"], false,
        "never checked against an app"
    );
    assert!(heard[1].request.get("expect").is_none());
    assert_eq!(
        held,
        std::slice::from_ref(&owner),
        "the owner's claim while held"
    );
    assert!(client.controller.pending().unwrap().is_empty());
    let owner = Token::parse(&owner).unwrap();
    until("the owner settled with the guest", || {
        settled(&serving).contains(&owner)
    })
    .await;
    assert!(service.owner.lock().unwrap().is_none(), "let go of");
    // Taking, keeping and giving back control is no row of its own.
    let rows =
        std::fs::read_to_string(client.output_dir().unwrap().join(crate::activity_log::FILE))
            .unwrap_or_default();
    assert!(
        !rows.contains("acquire") && !rows.contains("renew"),
        "{rows}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_control_of_a_vm_goes_on_while_agents_stay_stopped() {
    let (serving, saying, agents, _directory) = proven().await;
    let mut client = agents.clone();
    client.actor = crate::client::Actor::Viewer;
    let service = serving_input(&serving, &saying);
    let (lease, owner) = persons_lease(&client);
    let capability = &serving.capability;
    // The person's Stop, kept here and held by the guest; and the guest's
    // own after it.
    client.controller.note_stop(true).unwrap();
    serving.fixture.guest.standin.stops.fetch_add(1, SeqCst);
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["stop"] = json!({"state": "stopped", "why": "control-lost"});
        hello["agents"]["stopped"] = json!(true);
    });

    let proof = client.person_proof_on_vm().await.unwrap();
    let taken = client
        .person_acquire_on_vm(&owner, &proof, capability)
        .await
        .unwrap();
    let sent = client
        .person_input_on_vm(
            (&owner, taken.generation()),
            (&persons_click(), proof.epoch(), std::time::Instant::now()),
            &lease,
            capability,
        )
        .await
        .unwrap();
    let released = client
        .person_release_on_vm(taken, capability)
        .await
        .unwrap();
    drop(lease);
    let agents = clicked(&agents).await.unwrap_err();

    assert_eq!(sent["performed"], 1, "{sent}");
    assert_eq!(released["released"], true, "{released}");
    assert_eq!(agents.code, Code::Stopped, "{agents:?}");
    assert_eq!(service.changes().len(), 3, "the agent's never reached it");
    assert!(client.controller.pending().unwrap().is_empty());
    let revision = client.controller.stop_revision().unwrap();
    client.controller.lift_stop(revision.as_ref()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_guest_grant_refuses_renewal_for_certain_and_is_still_let_go_of() {
    let (serving, saying, mut client, _directory) = proven().await;
    client.actor = crate::client::Actor::Viewer;
    let service = serving_input(&serving, &saying);
    let (lease, owner) = persons_lease(&client);
    let capability = &serving.capability;
    let proof = client.person_proof_on_vm().await.unwrap();
    let taken = client
        .person_acquire_on_vm(&owner, &proof, capability)
        .await
        .unwrap();
    service.expired.store(true, SeqCst);

    let renewed = client
        .person_renew_on_vm(&owner, taken.generation(), capability)
        .await
        .unwrap_err();
    let sent = client
        .person_input_on_vm(
            (&owner, taken.generation()),
            (&persons_click(), proof.epoch(), std::time::Instant::now()),
            &lease,
            capability,
        )
        .await
        .unwrap();
    let held = pending(&client);
    let released = client.person_release_on_vm(taken, capability).await;

    assert_eq!(renewed.code, Code::LeaseLost, "{renewed:?}");
    assert!(!renewed.uncertain, "{renewed:?}");
    assert_eq!(sent["ok"], false, "{sent}");
    assert_eq!(sent["code"], "lease-lost", "{sent}");
    assert_eq!(
        held,
        [owner],
        "only the owner's claim, the refusals settled"
    );
    assert_eq!(released.unwrap()["released"], true);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_generation_refuses_the_persons_control_unsaid_and_leaves_its_owner_pending() {
    let (serving, saying, mut client, _directory) = proven().await;
    client.actor = crate::client::Actor::Viewer;
    let service = serving_input(&serving, &saying);
    let capability = &serving.capability;
    let (_lease, owner) = persons_lease(&client);
    let proof = client.person_proof_on_vm().await.unwrap();
    let taken = client
        .person_acquire_on_vm(&owner, &proof, capability)
        .await
        .unwrap();
    restarted(&serving).await;

    let renewed = client
        .person_renew_on_vm(&owner, taken.generation(), capability)
        .await
        .unwrap_err();
    let released = client
        .person_release_on_vm(taken, capability)
        .await
        .unwrap_err();

    for refused in [&renewed, &released] {
        assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
        assert!(!refused.uncertain, "{refused:?}");
    }
    assert_eq!(service.changes().len(), 1, "only its acquire was heard");
    assert_eq!(
        pending(&client),
        [owner],
        "the owner, for the keeper's recovery to prove let go of"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_persons_control_is_carried_only_with_the_keepers_capability_and_never_for_an_agent() {
    let (serving, saying, client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let change = Token::new();
    let asking = |lane: &str, capability: Option<&str>| {
        let mut asked = json!({"op": "change", "lane": lane, "change": change,
            "generation": generation, "request": {"op": "acquire", "owner": Token::new(),
                "epoch": {"uid": 502}, "person": true, "change": change}});
        if let Some(capability) = capability {
            asked["capability"] = json!(capability);
        }
        asked
    };
    let wrong = "0".repeat(serving.capability.len());

    let mut refused = Vec::new();
    for capability in [None, Some(wrong.as_str())] {
        refused.extend(asked(&serving.socket, &asking("person", capability)).await);
    }
    // The mark on an ordinary input lane is never carried, capability or
    // not.
    let mut unlaned = Vec::new();
    for capability in [None, Some(serving.capability.as_str())] {
        unlaned.extend(asked(&serving.socket, &asking("input", capability)).await);
    }
    let agents = [
        client.person_proof_on_vm().await.unwrap_err(),
        client
            .person_renew_on_vm(change.as_str(), generation, &serving.capability)
            .await
            .unwrap_err(),
    ];

    for refused in &refused {
        assert_eq!(refused["code"], "denied", "{refused}");
        assert_eq!(refused["sent"], false, "{refused}");
        assert!(!refused.to_string().contains("helper"), "{refused}");
    }
    for refused in &unlaned {
        assert_eq!(refused["code"], "bad-request", "{refused}");
        assert_eq!(refused["sent"], false, "{refused}");
    }
    for refused in &agents {
        assert_eq!(refused.code, Code::Denied, "{refused:?}");
        assert!(!refused.error.contains("helper"), "{refused:?}");
    }
    assert!(service.changes().is_empty(), "nothing reached the guest");
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn the_persons_page_is_carried_only_on_their_lane_with_the_keepers_capability() {
    let (serving, saying, _client, _directory) = proven().await;
    let service = serving_input(&serving, &saying);
    let generation = live(&serving);
    let asking = |lane: &str, capability: Option<&str>| {
        let change = Token::new();
        let mut asked = json!({"op": "change", "lane": lane, "change": change,
            "generation": generation, "request": {"op": "person-open",
                "app": "Google Chrome", "url": "https://github.com", "lease": Token::new(),
                "person": true, "change": change}});
        if let Some(capability) = capability {
            asked["capability"] = json!(capability);
        }
        asked
    };
    let capability = serving.capability.as_str();

    let unheld = asked(&serving.socket, &asking("person", None)).await;
    let elsewhere = [
        asked(&serving.socket, &asking("input", Some(capability))).await,
        asked(&serving.socket, &asking("change", Some(capability))).await,
    ];
    let carried = asked(&serving.socket, &asking("person", Some(capability))).await;

    for refused in &unheld {
        assert_eq!(refused["code"], "denied", "{refused}");
        assert_eq!(refused["sent"], false, "{refused}");
    }
    for refused in elsewhere.iter().flatten() {
        assert_eq!(refused["code"], "bad-request", "{refused}");
        assert_eq!(refused["sent"], false, "{refused}");
    }
    assert!(!carried.is_empty());
    let heard = service.changes();
    assert_eq!(heard.len(), 1, "only the one carried reached the guest");
    assert!(matches!(heard[0].lane, LaneKind::Person { .. }));
    assert_eq!(heard[0].request["op"], "person-open");
}
