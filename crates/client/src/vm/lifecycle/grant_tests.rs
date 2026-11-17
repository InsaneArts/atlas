use super::*;
use crate::vm::lifecycle::tests::lifecycle;
use std::{cell::Cell, sync::Mutex};

fn operation(kind: Kind) -> Operation {
    Operation {
        kind,
        vm: DesktopId::parse("0123456789abcdef01234567").unwrap(),
        name: "work".to_owned(),
        settings: PathBuf::from("/Users/someone/.config/atlas.toml"),
        cancel: false,
    }
}

/// Time a test moves by hand: each pause moves it on by the pause.
struct Manual {
    now: Cell<u64>,
    /// Done at the first pause, as the person answering while it waits.
    meanwhile: Mutex<Option<Box<dyn FnOnce()>>>,
}

impl Manual {
    fn at(now: u64) -> Self {
        Self {
            now: Cell::new(now),
            meanwhile: Mutex::new(None),
        }
    }

    fn meanwhile(self, done: impl FnOnce() + 'static) -> Self {
        *self.meanwhile.lock().unwrap() = Some(Box::new(done));
        self
    }
}

impl Clock for Manual {
    fn now(&self) -> u64 {
        self.now.get()
    }

    fn pause(&self, pause: Duration) {
        if let Some(done) = self.meanwhile.lock().unwrap().take() {
            done();
        }
        self.now.set(self.now.get() + millis(pause));
    }
}

/// Atlas.app as a test has it: it records what it was opened for, and
/// shows it at once unless told not to.
struct Opened {
    ids: Mutex<Vec<Token>>,
    lifecycle: Lifecycle,
    shows: bool,
    fails: bool,
}

impl Opened {
    fn new(lifecycle: &Lifecycle) -> Self {
        Self {
            ids: Mutex::new(Vec::new()),
            lifecycle: lifecycle.clone(),
            shows: true,
            fails: false,
        }
    }

    fn id(&self) -> Token {
        self.ids.lock().unwrap()[0].clone()
    }
}

impl Opener for Opened {
    fn open(&self, id: &Token) -> Result<()> {
        self.ids.lock().unwrap().push(id.clone());
        if self.fails {
            return Err(refusal("Atlas.app could not be opened."));
        }
        if self.shows {
            request_at(&self.lifecycle, id, 0).unwrap();
        }
        Ok(())
    }
}

/// A grant the person allowed through the window, for `kind`, as a test
/// asks for it.
fn granted(lifecycle: &Lifecycle, kind: Kind) -> Grant {
    let opened = Opened::new(lifecycle);
    let answering = lifecycle.clone();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let clock = Manual::at(0).meanwhile(move || {
        let id = only_request(&requests);
        answer_at(&answering, &id, true, 0).unwrap();
    });
    ask_with(
        lifecycle,
        &opened,
        &clock,
        operation(kind),
        "Claude Code in work",
    )
    .unwrap()
}

fn only_request(requests: &Path) -> Token {
    let names: Vec<_> = std::fs::read_dir(requests)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".json").and_then(Token::parse)
        })
        .collect();
    assert_eq!(names.len(), 1);
    names[0].clone()
}

#[test]
fn an_allowed_request_is_a_grant_for_its_operation_once() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Add);
    let change = grant.change().clone();

    assert_eq!(grant.take_at(&operation(Kind::Add), 1).unwrap(), change);
}

#[test]
fn a_request_with_no_answer_is_no_grant_and_nothing_is_taken() {
    let (_home, lifecycle) = lifecycle();
    let grant = Grant {
        id: Token::new(),
        operation: operation(Kind::Add),
        change: Token::new(),
        lifecycle: lifecycle.clone(),
    };

    let refused = grant.take_at(&operation(Kind::Add), 0).unwrap_err();

    assert_eq!(refused.code, Code::Denied);
    assert!(
        refused.error.contains("has not allowed"),
        "{}",
        refused.error
    );
}

#[test]
fn a_grant_is_refused_for_another_vm() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Remove);
    let other = Operation {
        vm: DesktopId::parse("89abcdef0123456789abcdef").unwrap(),
        ..operation(Kind::Remove)
    };

    let refused = grant.take_at(&other, 1).unwrap_err();

    assert!(refused.error.contains("another VM"), "{}", refused.error);
}

#[test]
fn a_grant_is_refused_for_another_kind() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Add);

    let refused = grant.take_at(&operation(Kind::Uninstall), 1).unwrap_err();

    assert!(refused.error.contains("another kind"), "{}", refused.error);
}

#[test]
fn a_grant_is_refused_after_it_was_used() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Add);
    let again = Grant {
        id: grant.id.clone(),
        operation: grant.operation.clone(),
        change: grant.change.clone(),
        lifecycle: lifecycle.clone(),
    };
    grant.take_at(&operation(Kind::Add), 1).unwrap();

    let refused = again.take_at(&operation(Kind::Add), 2).unwrap_err();

    assert!(refused.error.contains("used already"), "{}", refused.error);
}

#[test]
fn a_grant_is_refused_after_it_expires() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Add);

    let refused = grant
        .take_at(&operation(Kind::Add), millis(LIFE))
        .unwrap_err();

    assert!(refused.error.contains("expired"), "{}", refused.error);
}

#[test]
fn a_refused_request_is_no_grant_and_reopens_as_refused_even_past_its_time() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened::new(&lifecycle);
    let answering = lifecycle.clone();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let seen = requests.clone();
    let clock = Manual::at(0).meanwhile(move || {
        answer_at(&answering, &only_request(&seen), false, 0).unwrap();
    });

    let refused = ask_with(&lifecycle, &opened, &clock, operation(Kind::Add), "a").unwrap_err();

    assert!(refused.error.contains("did not allow"), "{}", refused.error);
    let shown = request_at(&lifecycle, &opened.id(), millis(ANSWERING) + 1).unwrap();
    assert_eq!(shown.answered, Some(Answered::Refused));
}

#[test]
fn a_request_nobody_answers_is_refused_once_its_time_is_up() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened::new(&lifecycle);

    let refused = ask_with(
        &lifecycle,
        &opened,
        &Manual::at(0),
        operation(Kind::Add),
        "a",
    )
    .unwrap_err();

    assert!(refused.error.contains("in time"), "{}", refused.error);
    let late = answer_at(&lifecycle, &opened.id(), true, 1).unwrap_err();
    assert!(late.error.contains("no longer waiting"), "{}", late.error);
}

#[test]
fn a_request_atlas_app_never_shows_is_refused_and_says_so() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened {
        shows: false,
        ..Opened::new(&lifecycle)
    };

    let refused = ask_with(
        &lifecycle,
        &opened,
        &Manual::at(0),
        operation(Kind::Add),
        "a",
    )
    .unwrap_err();

    assert!(refused.error.contains("did not show"), "{}", refused.error);
}

#[test]
fn atlas_app_missing_refuses_the_request_before_anything_waits() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened {
        fails: true,
        ..Opened::new(&lifecycle)
    };

    let refused = ask_with(
        &lifecycle,
        &opened,
        &Manual::at(0),
        operation(Kind::Add),
        "a",
    )
    .unwrap_err();

    assert!(
        refused.error.contains("could not be opened"),
        "{}",
        refused.error
    );
    let requests = lifecycle.inner(REQUESTS).unwrap();
    assert!(!file(&requests, &opened.id(), "json").exists());
}

#[test]
fn atlas_app_reads_what_which_vm_and_from_where_and_answers_once() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened {
        shows: false,
        ..Opened::new(&lifecycle)
    };
    let shown_by = lifecycle.clone();
    let shown = std::rc::Rc::new(std::cell::RefCell::new(None));
    let seen = shown.clone();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let clock = Manual::at(5).meanwhile(move || {
        let id = only_request(&requests);
        *seen.borrow_mut() = Some(request_at(&shown_by, &id, 6).unwrap());
        answer_at(&shown_by, &id, true, 6).unwrap();
        let twice = answer_at(&shown_by, &id, false, 7).unwrap_err();
        assert!(twice.error.contains("answered already"), "{}", twice.error);
    });

    ask_with(
        &lifecycle,
        &opened,
        &clock,
        operation(Kind::Remove),
        "Codex in atlas",
    )
    .unwrap();

    let shown = shown.borrow_mut().take().unwrap();
    assert_eq!(shown.kind, Kind::Remove);
    assert_eq!(shown.name, "work");
    assert_eq!(shown.vm.as_str(), "0123456789abcdef01234567");
    assert_eq!(shown.session, "Codex in atlas");
    assert_eq!(shown.until, 5 + millis(ANSWERING));
    let json = serde_json::to_value(&shown).unwrap();
    for field in ["id", "kind", "name", "vm", "settings", "session", "until"] {
        assert!(json.get(field).is_some(), "{field}");
    }
}

#[test]
fn a_request_past_its_time_is_never_shown_or_answered() {
    let (_home, lifecycle) = lifecycle();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let request = Request {
        id: Token::new(),
        operation: operation(Kind::Add),
        change: Token::new(),
        session: "a".to_owned(),
        asked: 0,
        until: 10,
    };
    create(&file(&requests, &request.id, "json"), &request).unwrap();

    let shown = request_at(&lifecycle, &request.id, 10).unwrap_err();
    let answered = answer_at(&lifecycle, &request.id, true, 10).unwrap_err();

    assert!(shown.error.contains("expired"), "{}", shown.error);
    assert!(answered.error.contains("expired"), "{}", answered.error);
    assert!(!file(&requests, &request.id, "answer").exists());
}

#[test]
fn an_allowed_request_reopens_as_allowed_and_a_used_one_as_used_never_asking_again() {
    let (_home, lifecycle) = lifecycle();
    let grant = granted(&lifecycle, Kind::Add);
    let id = grant.id.clone();

    let allowed = request_at(&lifecycle, &id, 1).unwrap();
    let again = answer_at(&lifecycle, &id, false, 1).unwrap_err();
    grant.take_at(&operation(Kind::Add), 2).unwrap();
    let used = request_at(&lifecycle, &id, millis(ANSWERING) + 1).unwrap();
    let late = answer_at(&lifecycle, &id, true, millis(ANSWERING) + 1).unwrap_err();

    assert_eq!(allowed.answered, Some(Answered::Allowed));
    assert_eq!(used.answered, Some(Answered::Used));
    for refused in [again, late] {
        assert!(
            refused.error.contains("answered already"),
            "{}",
            refused.error
        );
    }
    let json = serde_json::to_value(&used).unwrap();
    assert_eq!(json["answered"], "used");
}

#[test]
fn a_request_not_answered_reads_with_no_answer() {
    let (_home, lifecycle) = lifecycle();
    let opened = Opened::new(&lifecycle);
    let reading = lifecycle.clone();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
    let kept = seen.clone();
    let clock = Manual::at(0).meanwhile(move || {
        *kept.borrow_mut() = Some(request_at(&reading, &only_request(&requests), 1).unwrap());
    });

    ask_with(&lifecycle, &opened, &clock, operation(Kind::Add), "a").unwrap_err();

    let shown = seen.borrow_mut().take().unwrap();
    assert_eq!(shown.answered, None);
    assert!(
        serde_json::to_value(&shown)
            .unwrap()
            .get("answered")
            .is_none()
    );
}

#[test]
fn a_grant_to_call_an_uninstall_off_is_never_one_to_uninstall_and_atlas_app_reads_which() {
    let (_home, lifecycle) = lifecycle();
    let calling_off = Operation {
        cancel: true,
        ..operation(Kind::Uninstall)
    };
    let opened = Opened::new(&lifecycle);
    let answering = lifecycle.clone();
    let requests = lifecycle.inner(REQUESTS).unwrap();
    let shown = std::rc::Rc::new(std::cell::RefCell::new(None));
    let seen = shown.clone();
    let clock = Manual::at(0).meanwhile(move || {
        let id = only_request(&requests);
        *seen.borrow_mut() = Some(request_at(&answering, &id, 0).unwrap());
        answer_at(&answering, &id, true, 0).unwrap();
    });
    let grant = ask_with(&lifecycle, &opened, &clock, calling_off, "a terminal").unwrap();

    let refused = grant.take_at(&operation(Kind::Uninstall), 1).unwrap_err();

    assert!(refused.error.contains("another kind"), "{}", refused.error);
    let shown = serde_json::to_value(shown.borrow_mut().take().unwrap()).unwrap();
    assert_eq!(shown["cancel"], true);
    let (_elsewhere, other) = crate::vm::lifecycle::tests::lifecycle();
    let uninstalling = granted(&other, Kind::Uninstall);
    assert!(
        serde_json::to_value(uninstalling.operation())
            .unwrap()
            .get("cancel")
            .is_none(),
        "said only when it calls one off"
    );
}
