use super::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

const HELPER: u32 = 503;
const PERSON: u32 = 501;

#[test]
fn only_processes_of_the_helper_are_muted() {
    let pids = HashMap::from([(10, 100), (11, 101), (12, 102)]);
    let owners = HashMap::from([(100, HELPER), (101, PERSON), (102, HELPER)]);

    let owned = owned_by(
        &[12, 11, 10],
        HELPER,
        |object| Ok(pids.get(&object).copied()),
        |pid| Ok(owners.get(&pid).copied()),
    );

    assert_eq!(owned.unwrap(), [10, 12]);
}

#[test]
fn a_process_that_vanished_or_has_no_owner_plays() {
    let pids = HashMap::from([(10, 100), (11, 0), (12, 102)]);
    let owners = HashMap::from([(100, HELPER)]);

    let owned = owned_by(
        &[10, 11, 12, 13],
        HELPER,
        |object| Ok(pids.get(&object).copied()),
        |pid| Ok(owners.get(&pid).copied()),
    );

    assert_eq!(owned.unwrap(), [10]);
}

#[test]
fn an_object_that_names_another_process_after_the_check_plays() {
    let reads = RefCell::new(0);
    let pid_of = |_| {
        *reads.borrow_mut() += 1;
        Ok(Some(if *reads.borrow() == 1 { 100 } else { 200 }))
    };

    let owned = owned_by(&[10], HELPER, pid_of, |_| Ok(Some(HELPER)));

    assert!(owned.unwrap().is_empty());
}

#[test]
fn a_process_that_could_not_be_read_is_unknown_never_left_out() {
    let failed = || Error::new(Code::Daemon, "unreadable");

    let unread_object = owned_by(&[10], HELPER, |_| Err(failed()), |_| Ok(Some(HELPER)));
    let unread_owner = owned_by(&[10], HELPER, |_| Ok(Some(100)), |_| Err(failed()));

    assert!(unread_object.is_err());
    assert!(unread_owner.is_err());
}

#[test]
fn a_tap_description_never_restores_an_ended_process_by_its_bundle() {
    if CoreAudio::new(HELPER, Arc::default()).is_none() {
        return;
    }

    let tap = description(&[7]);

    if tap.respondsToSelector(objc2::sel!(isProcessRestoreEnabled)) {
        // SAFETY: a property read this macOS answers to.
        assert!(!unsafe { tap.isProcessRestoreEnabled() });
    }
}

#[test]
fn a_tap_description_is_private_muted_and_names_only_its_processes() {
    if CoreAudio::new(HELPER, Arc::default()).is_none() {
        return;
    }

    let tap = description(&[7, 9]);

    // SAFETY: plain property reads of a description made here.
    let (private, muted, exclusive, processes) = unsafe {
        (
            tap.isPrivate(),
            tap.isMuted(),
            tap.isExclusive(),
            tap.processes()
                .iter()
                .map(|number| number.as_u32())
                .collect::<Vec<_>>(),
        )
    };
    assert!(private);
    assert_eq!(muted, CATapMuteBehavior::Muted);
    assert!(!exclusive);
    assert_eq!(processes, [7, 9]);
}

/// What the fake Core Audio saw, in order.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Built(u32, Vec<AudioObjectID>),
    TornDown(u32),
    /// Its objects were proven gone, so nothing was destroyed.
    LetGo(u32),
}

#[derive(Default)]
struct Fake {
    objects: RefCell<Vec<AudioObjectID>>,
    steps: Rc<RefCell<Vec<Step>>>,
    next: Cell<u32>,
    refuse: Cell<bool>,
    /// A refused build made part of a mute first.
    made_before_refusing: Cell<bool>,
    /// Listing the processes fails.
    unlisted: Cell<bool>,
    /// The permission as a fresh read says; allowed unless set.
    access: Cell<Option<Access>>,
    /// Core Audio's restarts so far.
    generation: Cell<u64>,
    /// Core Audio restarts while the permission is read, or while a mute
    /// is built.
    restart_on_access: Cell<bool>,
    restart_on_build: Cell<bool>,
    /// Mutes whose objects are gone, and mutes whose objects cannot be read.
    gone: RefCell<Vec<u32>>,
    unreadable: RefCell<Vec<u32>>,
    /// A listed process belongs to someone else by the time IO would start.
    reassigned: Cell<bool>,
    /// The mutes whose IO started.
    started: RefCell<Vec<u32>>,
}

impl Fake {
    fn restart(&self) {
        self.generation.set(self.generation.get() + 1);
    }

    fn make(&self, objects: &[AudioObjectID]) -> u32 {
        let mute = self.next.get() + 1;
        self.next.set(mute);
        self.steps
            .borrow_mut()
            .push(Step::Built(mute, objects.to_vec()));
        mute
    }
}

impl Backend for Fake {
    type Mute = u32;

    fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn objects(&self) -> Result<Vec<AudioObjectID>> {
        if self.unlisted.get() {
            return Err(Error::new(Code::Daemon, "unlisted"));
        }
        Ok(self.objects.borrow().clone())
    }

    fn access(&self) -> Access {
        if self.restart_on_access.get() {
            self.restart();
        }
        self.access.get().unwrap_or(Access::Granted)
    }

    fn still_ours(&self, _: &[AudioObjectID]) -> Result<bool> {
        Ok(!self.reassigned.get())
    }

    fn start(&self, mute: &mut u32, generation: u64) -> Result<()> {
        assert_eq!(
            generation,
            self.generation.get(),
            "started across a restart"
        );
        self.started.borrow_mut().push(*mute);
        Ok(())
    }

    fn build(&self, objects: &[AudioObjectID]) -> std::result::Result<u32, Box<Unbuilt<u32>>> {
        if self.refuse.get() {
            let made = self.made_before_refusing.get().then(|| self.make(objects));
            return Err(Box::new(Unbuilt {
                error: Error::new(Code::Daemon, "refused"),
                made,
            }));
        }
        let mute = self.make(objects);
        if self.restart_on_build.get() {
            self.restart();
        }
        Ok(mute)
    }

    fn tear_down(&self, mute: u32) -> Option<u32> {
        if self.unreadable.borrow().contains(&mute) {
            return Some(mute);
        }
        let step = if self.gone.borrow().contains(&mute) {
            Step::LetGo(mute)
        } else {
            Step::TornDown(mute)
        };
        self.steps.borrow_mut().push(step);
        None
    }
}

fn engine(objects: &[AudioObjectID]) -> (Engine<Fake>, Rc<RefCell<Vec<Step>>>) {
    let fake = Fake::default();
    fake.objects.replace(objects.to_vec());
    let steps = fake.steps.clone();
    (Engine::new(fake), steps)
}

#[test]
fn a_new_mute_is_built_before_the_old_one_ends() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.objects.replace(vec![1, 2]);
    engine.refresh().unwrap();

    assert_eq!(
        *steps.borrow(),
        [
            Step::Built(1, vec![1]),
            Step::Built(2, vec![1, 2]),
            Step::TornDown(1)
        ]
    );
}

#[test]
fn unchanged_processes_keep_the_mute() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.refresh().unwrap();

    assert_eq!(*steps.borrow(), [Step::Built(1, vec![1])]);
}

#[test]
fn after_a_restart_a_mute_proven_gone_is_let_go_and_one_still_ours_ended() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.restart();
    engine.backend.gone.borrow_mut().push(1);
    engine.refresh().unwrap();

    engine.backend.restart();
    engine.refresh().unwrap();

    assert_eq!(
        *steps.borrow(),
        [
            Step::Built(1, vec![1]),
            Step::LetGo(1),
            Step::Built(2, vec![1]),
            Step::TornDown(2),
            Step::Built(3, vec![1])
        ]
    );
}

#[test]
fn a_restart_during_the_permission_read_makes_nothing_from_the_stale_list() {
    let (mut engine, steps) = engine(&[1]);
    engine.backend.restart_on_access.set(true);

    assert!(matches!(engine.refresh(), Err(Refusal::Stale)));
    assert!(steps.borrow().is_empty());
}

#[test]
fn a_mute_built_across_a_restart_is_ended_never_dropped() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.objects.replace(vec![1, 2]);
    engine.backend.restart_on_build.set(true);

    assert!(matches!(engine.refresh(), Err(Refusal::Stale)));
    assert_eq!(
        *engine.backend.started.borrow(),
        [1],
        "its IO never started"
    );
    engine.backend.restart_on_build.set(false);
    engine.stop();

    assert_eq!(
        *steps.borrow(),
        [
            Step::Built(1, vec![1]),
            Step::Built(2, vec![1, 2]),
            Step::TornDown(2),
            Step::TornDown(1)
        ]
    );
}

#[test]
fn a_failed_build_ends_what_it_made_and_keeps_the_mute_already_running() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.objects.replace(vec![1, 2]);
    engine.backend.refuse.set(true);
    engine.backend.made_before_refusing.set(true);

    assert!(matches!(engine.refresh(), Err(Refusal::Failed(_))));
    assert!(engine.running());
    assert_eq!(
        *steps.borrow(),
        [
            Step::Built(1, vec![1]),
            Step::Built(2, vec![1, 2]),
            Step::TornDown(2)
        ]
    );
}

#[test]
fn a_mute_not_fully_ended_is_kept_and_tried_again_later_until_it_ends() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.unreadable.borrow_mut().push(1);
    engine.stop();
    let first = engine.next_retry().expect("never dropped unended");

    engine.sweep(first - Duration::from_millis(1));
    assert_eq!(engine.next_retry(), Some(first), "not before its time");

    let mut now = first;
    for _ in 0..20 {
        engine.sweep(now);
        now = engine.next_retry().expect("it is never given up");
    }
    assert!(now - first >= LONGEST_RETRY, "the wait grows");
    assert!(
        now - first <= LONGEST_RETRY * 20,
        "and stops growing at its longest"
    );

    engine.backend.unreadable.borrow_mut().clear();
    engine.sweep(now);
    assert_eq!(engine.next_retry(), None);
    assert_eq!(
        *steps.borrow(),
        [Step::Built(1, vec![1]), Step::TornDown(1)]
    );
}

#[test]
fn a_teardown_stage_runs_only_on_an_object_proven_ours_and_counts_only_if_it_worked() {
    let ran = Cell::new(false);
    let ran = &ran;
    let call = |status| {
        move || {
            ran.set(true);
            status
        }
    };

    assert_eq!(stage(|| Ownership::Ours, call(0)), Stage::Done);
    assert_eq!(stage(|| Ownership::Ours, call(1)), Stage::Pending);

    let checks = Cell::new(0);
    let gone_after = || {
        checks.set(checks.get() + 1);
        if checks.get() == 1 {
            Ownership::Ours
        } else {
            Ownership::Gone
        }
    };
    assert_eq!(stage(gone_after, call(1)), Stage::Gone);

    ran.set(false);
    assert_eq!(stage(|| Ownership::Gone, call(0)), Stage::Gone);
    assert_eq!(stage(|| Ownership::Unknown, call(0)), Stage::Pending);
    assert!(!ran.get(), "nothing runs on an object not proven ours");
}

#[test]
fn a_failed_process_listing_is_no_empty_list_and_keeps_the_mute() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.unlisted.set(true);

    assert!(matches!(engine.refresh(), Err(Refusal::Failed(_))));
    assert_eq!(*steps.borrow(), [Step::Built(1, vec![1])]);
}

#[test]
fn a_rebuild_without_a_fresh_yes_ends_the_mute_and_starts_nothing() {
    let (mut engine, steps) = engine(&[1]);
    engine.refresh().unwrap();
    engine.backend.objects.replace(vec![1, 2]);

    for access in [Access::Denied, Access::Undetermined] {
        engine.backend.access.set(Some(access));
        assert!(matches!(engine.refresh(), Err(Refusal::Access(said)) if said == access));
    }

    assert_eq!(
        *steps.borrow(),
        [Step::Built(1, vec![1]), Step::TornDown(1)]
    );
}

#[test]
fn no_helper_audio_needs_no_mute_and_stopping_ends_it() {
    let (mut engine, steps) = engine(&[]);
    engine.refresh().unwrap();
    assert!(steps.borrow().is_empty());

    engine.backend.objects.replace(vec![3]);
    engine.refresh().unwrap();
    engine.stop();

    assert_eq!(
        *steps.borrow(),
        [Step::Built(1, vec![3]), Step::TornDown(1)]
    );
}

#[test]
fn a_process_given_to_someone_else_before_io_starts_ends_the_mute_unstarted() {
    let (mut engine, steps) = engine(&[1]);
    engine.backend.reassigned.set(true);

    assert!(matches!(engine.refresh(), Err(Refusal::Stale)));
    assert!(engine.backend.started.borrow().is_empty());
    assert!(!engine.running());
    assert_eq!(
        *steps.borrow(),
        [Step::Built(1, vec![1]), Step::TornDown(1)]
    );
}

#[test]
fn only_a_process_the_mute_does_not_cover_counts_as_uncovered() {
    let (mut engine, _steps) = engine(&[1]);
    assert!(engine.uncovered().unwrap(), "nothing is muted yet");
    engine.refresh().unwrap();
    assert!(!engine.uncovered().unwrap());

    engine.backend.objects.replace(vec![1, 2]);
    assert!(engine.uncovered().unwrap());

    engine.backend.objects.replace(vec![]);
    assert!(!engine.uncovered().unwrap(), "an ended app is not new");
}
