//! A keeper's update of its guest, end to end: a real keeper running from a
//! signed bundle of its own, a stand-in guest of an older build that stages,
//! keeps and settles updates as a machine guest does, and the station's own
//! Stop and pending changes.
use super::*;
use crate::vm::keeper::{
    Keeper,
    change::tests::{admitted, restarted},
    clients::tests::{Serving, asked, serving_of},
    program::Program,
    tests::{Saying, fixture_with, until},
};
use atlas_control::StopWhy;
use atlas_session::{compatibility::UPDATE_BOUND, standin::StandIn};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::atomic::Ordering::SeqCst};

/// A build of Atlas a keeper runs from: a signed executable in an app
/// bundle's place for it.
struct Bundle {
    _folder: tempfile::TempDir,
    executable: PathBuf,
}

/// A bundle whose executable is a copy of the signed program `program`.
fn bundle_of(program: &str) -> Bundle {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("Atlas.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), b"<plist/>").unwrap();
    let executable = app.join("Contents/MacOS/atlas");
    std::fs::copy(program, &executable).unwrap();
    Bundle {
        _folder: folder,
        executable,
    }
}

fn bundle() -> Bundle {
    bundle_of("/usr/bin/true")
}

/// An older build's digest, as a guest's daemon says its own.
fn older() -> String {
    "a0".repeat(32)
}

/// Wait until the keeper `serving` holds read its guest's report in its
/// live generation.
async fn reported(serving: &Serving) {
    let keeper = serving.fixture.keeper.clone();
    until("reported", move || {
        let state = keeper.shared.state();
        state
            .generation
            .is_some_and(|generation| state.reported(generation).is_some())
    })
    .await;
}

/// A keeper running from a bundle of its own, serving a stand-in machine
/// guest of an older build.
struct Updating {
    serving: Serving,
    saying: Arc<Saying>,
    bundle: Bundle,
}

impl Updating {
    async fn new() -> Self {
        let bundle = bundle();
        let fixture = fixture_with(Program::signed_at(&bundle.executable));
        let saying = fixture.guest.proving();
        *saying.daemon.lock().unwrap() = Some((older(), atlas_core::VERSION.to_owned()));
        let serving = serving_of(fixture, true).await;
        reported(&serving).await;
        Self {
            serving,
            saying,
            bundle,
        }
    }

    /// The keeper launchd starts after this one, running `program`, over
    /// the same VM, guest and records.
    async fn restarted_as(self, program: Program) -> Self {
        let Self {
            serving,
            saying,
            bundle,
        } = self;
        let serving = serving_of(serving.end().after(program), true).await;
        reported(&serving).await;
        Self {
            serving,
            saying,
            bundle,
        }
    }

    /// [`Updating::restarted_as`] of this keeper's own build.
    async fn restarted(self) -> Self {
        let program = Program::signed_at(&self.bundle.executable);
        self.restarted_as(program).await
    }

    fn keeper(&self) -> &Arc<Keeper> {
        &self.serving.fixture.keeper
    }

    fn standin(&self) -> &StandIn {
        &self.serving.fixture.guest.standin
    }

    fn session(&self) -> Arc<Session> {
        self.keeper().shared.live().unwrap()
    }

    /// The station's Stop kept for an update, and waited for until the
    /// guest confirmed it.
    async fn stopped(&self) {
        self.keeper()
            .stop
            .note_stop_unless_noted(StopWhy::Update)
            .unwrap();
        *self.serving.fixture.guest.stop.lock().unwrap() = "stopped";
        self.confirmed().await;
    }

    /// Wait until the live generation's guest confirmed the station's Stop.
    async fn confirmed(&self) {
        let keeper = self.keeper().clone();
        until("the guest confirmed the Stop", move || {
            let now = keeper.stop.stop_revision().unwrap();
            now.is_some() && keeper.applied().revision == now
        })
        .await;
    }

    /// The person's Resume, said through the keeper as theirs is: what it
    /// came to.
    async fn resumed(&self) -> Value {
        let revision = self.keeper().stop.stop_revision().unwrap();
        let answer = asked(
            &self.serving.socket,
            &json!({"op": "resume", "capability": self.serving.capability,
                "revision": revision}),
        )
        .await
        .pop()
        .unwrap();
        if answer["outcome"] == "resumed" {
            *self.serving.fixture.guest.stop.lock().unwrap() = "clear";
        }
        answer
    }

    /// What `update-guest` answers.
    async fn asked(&self) -> Value {
        asked(
            &self.serving.socket,
            &json!({"op": "update-guest", "deadline_ms": 30_000}),
        )
        .await
        .pop()
        .unwrap()
    }

    fn record(&self) -> Option<Record> {
        Record::kept(&self.keeper().folder).unwrap()
    }

    fn phase(&self) -> Phase {
        self.record().unwrap().phase
    }

    /// Keep `phase` as the update's, of this keeper's build.
    fn rewrite(&self, phase: Phase) {
        Record {
            target: self.keeper().target().unwrap(),
            phase,
            earlier: Vec::new(),
        }
        .keep(&self.keeper().folder)
        .unwrap();
    }

    /// Wait until the update kept is as `holds` says.
    async fn until_phase(&self, what: &str, holds: impl Fn(&Phase) -> bool + Send + 'static) {
        let folder = self.keeper().folder.clone();
        until(what, move || {
            Record::kept(&folder)
                .ok()
                .flatten()
                .is_some_and(|record| holds(&record.phase))
        })
        .await;
    }

    fn pending(&self) -> Vec<String> {
        self.keeper().stop.pending().unwrap()
    }

    /// Wait until no change is pending: a claim is cleared just after the
    /// answer it waited for is kept.
    async fn nothing_pending(&self) {
        let stop = self.keeper().stop.clone();
        until("nothing pending", move || {
            stop.pending().unwrap().is_empty()
        })
        .await;
    }

    /// Take a stage the guest placed back to where a keeper that ended
    /// before it kept the answer left it: still staging, bound to its
    /// manifest, its claim pending.
    fn unkept_stage(&self) -> Token {
        let Phase::Staged { stage, staged } = self.phase() else {
            panic!("not staged");
        };
        self.rewrite(Phase::Staging {
            stage: stage.clone(),
            digest: Some(staged.digest),
        });
        self.claim(&stage);
        stage
    }

    /// Claim `change` as the guest's, its claim left by a keeper that ended.
    fn claim(&self, change: &Token) {
        drop(
            self.keeper()
                .stop
                .quarantine_by(change.as_str(), Executor::Guest)
                .unwrap(),
        );
    }

    fn updates_asked(&self) -> Vec<Value> {
        self.standin().updates_asked.lock().unwrap().clone()
    }

    /// Every stage lane the guest admitted.
    fn stage_lanes(&self) -> usize {
        admitted(&self.serving)
            .iter()
            .filter(|header| matches!(header.kind, LaneKind::Stage { .. }))
            .count()
    }

    /// The guest says it runs `build` from its next generation on.
    async fn says_build(&self, build: &str) {
        *self.saying.daemon.lock().unwrap() =
            Some((build.to_owned(), atlas_core::VERSION.to_owned()));
        restarted(&self.serving).await;
    }

    /// This keeper's own build's digest.
    fn own(&self) -> String {
        self.keeper().program.build().digest.unwrap()
    }

    /// The update the guest took, once one is asked.
    fn update(&self) -> Token {
        match self.phase() {
            Phase::Asked { update, .. } => update,
            Phase::Settled { update, .. } | Phase::Cleaning { update, .. } => update.unwrap(),
            other => panic!("no update in {other:?}"),
        }
    }

    /// Hold the update's owner, so nothing but the test takes it a step.
    async fn paused(&self) -> tokio::sync::MutexGuard<'_, Owner> {
        self.keeper().owner.lock().await
    }

    /// One step of the update, the owner held by the test.
    async fn step(&self, owner: &mut Owner) -> Progress {
        self.keeper().advance(&self.session(), owner).await
    }

    /// Begin the update and take it `steps` steps on, the owner held.
    async fn begun(&self, owner: &mut Owner, steps: usize) {
        let target = self.keeper().target().unwrap();
        assert!(
            self.keeper()
                .begin(Some(&self.session()), target, Vec::new(), owner)
                .await
                .unwrap()
        );
        for _ in 0..steps {
            assert_eq!(self.step(owner).await, Progress::Moved);
        }
    }

    /// Whether the station folder can be synced: names are still made and
    /// removed in it while it cannot be opened.
    fn syncable(&self, syncable: bool) {
        let mode = if syncable { 0o700 } else { 0o300 };
        std::fs::set_permissions(&self.keeper().folder, std::fs::Permissions::from_mode(mode))
            .unwrap();
    }
}

/// A test of a guest's update as the person's or the update's real flow
/// takes it, every lane on its own thread.
macro_rules! updating_test {
    ($name:ident, $body:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn $name() {
            $body.await;
        }
    };
}

/// Take a fresh update to the guest's pending answer.
async fn asked_update() -> Updating {
    let updating = Updating::new().await;
    updating.stopped().await;
    let answer = updating.asked().await;
    assert_eq!(answer["state"], "updating", "{answer}");
    updating
}

updating_test!(
    a_guest_on_an_older_build_is_staged_this_keepers_own_bundle_and_asked_to_update,
    async {
        let updating = asked_update().await;

        let record = updating.record().unwrap();
        assert_eq!(record.target.build, updating.own());
        let Phase::Asked {
            stage,
            staged,
            update,
        } = record.phase
        else {
            panic!("{record:?}");
        };
        let copy = updating.standin().copies().get(&stage).cloned().unwrap();
        assert_eq!(copy.purpose.as_deref(), Some("update"));
        let app = updating.bundle.executable.ancestors().nth(3).unwrap();
        assert_eq!(copy.snapshot, atlas_core::stage::Snapshot::of(app).unwrap());
        assert_eq!(copy.digest, staged.digest);
        let asked = updating.updates_asked();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(asked[0]["change"], json!(update));
        assert_eq!(asked[0]["staged"], json!(staged.path));
        assert_eq!(asked[0]["expect"], json!(copy.digest));
        // The stage's answer kept, its claim cleared; the update's kept.
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
    }
);

updating_test!(
    nothing_is_staged_while_the_stations_stop_is_not_confirmed_by_the_guest,
    async {
        let updating = Updating::new().await;

        let unstopped = updating.asked().await;
        updating.standin().unkept.store(usize::MAX, SeqCst);
        updating
            .keeper()
            .stop
            .note_stop_unless_noted(StopWhy::Update)
            .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let unconfirmed = updating.asked().await;

        for refused in [&unstopped, &unconfirmed] {
            assert_eq!(refused["code"], "stopped", "{refused}");
            assert_eq!(
                refused["error"],
                "Agents on work are not stopped for an update, so its Atlas is not updated."
            );
            assert_eq!(refused["next"], "Run atlas update to stop them for it.");
        }
        assert!(updating.record().is_none());
        assert!(updating.standin().copies().is_empty());
        assert_eq!(updating.stage_lanes(), 0);
        assert!(updating.pending().is_empty());
    }
);

updating_test!(
    a_bundle_whose_executable_is_not_the_keepers_own_image_is_never_staged,
    async {
        // Its file changed in place: the same file, other bytes.
        let updating = Updating::new().await;
        updating.stopped().await;
        std::fs::write(&updating.bundle.executable, b"#!/bin/sh\n").unwrap();

        let changed = updating.asked().await;

        assert_eq!(changed["state"], "refused", "{changed}");
        assert_eq!(
            changed["why"],
            "The Atlas on this Mac is not the build this keeper runs, so nothing was staged."
        );
        assert!(matches!(updating.phase(), Phase::Done { .. }));
        assert_eq!(updating.stage_lanes(), 0);
        assert!(updating.standin().copies().is_empty());
        assert!(updating.pending().is_empty(), "claimed only once it sends");

        // Replaced by another file: refused before anything is kept.
        let replaced = Updating::new().await;
        replaced.stopped().await;
        let other = replaced.bundle.executable.with_file_name("atlas.new");
        std::fs::copy("/usr/bin/false", &other).unwrap();
        std::fs::rename(&other, &replaced.bundle.executable).unwrap();

        let refused = replaced.asked().await;

        assert_eq!(refused["code"], "busy", "{refused}");
        assert_eq!(
            refused["error"],
            "This keeper runs a build Atlas has since replaced; the new keeper updates work."
        );
        assert!(replaced.record().is_none());
        assert_eq!(replaced.stage_lanes(), 0);
    }
);

updating_test!(
    a_lost_stage_reply_is_answered_from_the_guests_record_and_the_bytes_are_never_sent_again,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        updating.standin().dropping.store(1, SeqCst);

        updating.asked().await;
        updating
            .until_phase("asked", |phase| matches!(phase, Phase::Asked { .. }))
            .await;

        assert_eq!(updating.standin().received.load(SeqCst), 1);
        assert_eq!(updating.stage_lanes(), 1);
        assert_eq!(updating.updates_asked().len(), 1);
    }
);

updating_test!(
    an_update_answered_pending_keeps_its_claim_across_the_guests_restart_until_it_is_released,
    async {
        let updating = asked_update().await;
        let update = updating.update();

        restarted(&updating.serving).await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        assert!(matches!(updating.phase(), Phase::Asked { .. }));
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
        assert!(updating.standin().settle_update(&update, "released"));
        updating.says_build(&updating.own()).await;
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        updating.nothing_pending().await;
        assert_eq!(updating.phase().outcome().unwrap()["update"], "released");
        let answer = updating.asked().await;
        assert_eq!(answer["state"], "updated", "{answer}");
    }
);

updating_test!(
    a_keeper_restarted_mid_update_resends_the_same_changes_and_the_guest_answers_from_its_records,
    async {
        let updating = asked_update().await;
        let update = updating.update();

        let updating = updating.restarted().await;
        let standin = updating.serving.fixture.guest.standin.clone();
        until("asked again", move || {
            standin.updates_asked.lock().unwrap().len() == 2
        })
        .await;

        let asked = updating.updates_asked();
        assert_eq!(asked[0], asked[1], "the same change, said again");
        assert_eq!(updating.update(), update);
        assert_eq!(updating.standin().received.load(SeqCst), 1, "staged once");
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
        let answer = updating.asked().await;
        assert_eq!(answer["state"], "updating", "{answer}");
    }
);

updating_test!(
    an_update_owed_while_the_vm_is_off_runs_at_its_next_generation_while_the_stop_holds,
    async {
        let bundle = bundle();
        let fixture = fixture_with(Program::signed_at(&bundle.executable));
        let saying = fixture.guest.proving();
        *saying.daemon.lock().unwrap() = Some((older(), atlas_core::VERSION.to_owned()));

        let owed = super::asked(&fixture.keeper, Duration::from_secs(1), false).await;
        assert_eq!(owed["state"], "owed", "{owed}");
        assert_eq!(
            owed["why"],
            "work's Atlas is updated once it runs and its agents are stopped for it."
        );
        assert!(matches!(
            Record::kept(&fixture.keeper.folder).unwrap().unwrap().phase,
            Phase::Intended
        ));
        fixture
            .keeper
            .stop
            .note_stop_unless_noted(StopWhy::Update)
            .unwrap();
        *fixture.guest.stop.lock().unwrap() = "stopped";
        let updating = Updating {
            serving: serving_of(fixture, true).await,
            saying,
            bundle,
        };

        let standin = updating.serving.fixture.guest.standin.clone();
        until("asked", move || {
            standin.updates_asked.lock().unwrap().len() == 1
        })
        .await;
        assert!(matches!(updating.phase(), Phase::Asked { .. }));
    }
);

updating_test!(
    an_update_owed_when_the_person_resumed_first_is_dropped_and_status_says_one_is_available,
    async {
        let bundle = bundle();
        let fixture = fixture_with(Program::signed_at(&bundle.executable));
        let saying = fixture.guest.proving();
        *saying.daemon.lock().unwrap() = Some((older(), atlas_core::VERSION.to_owned()));
        super::asked(&fixture.keeper, Duration::from_secs(1), false).await;
        let folder = fixture.keeper.folder.clone();

        let updating = Updating {
            serving: serving_of(fixture, true).await,
            saying,
            bundle,
        };
        until("dropped", move || Record::kept(&folder).unwrap().is_none()).await;

        assert_eq!(updating.stage_lanes(), 0);
        let facts = updating.keeper().update_facts(&updating.session());
        assert_eq!(
            crate::vm::update::said("work", &facts)["state"],
            "available"
        );
    }
);

updating_test!(
    a_rolled_back_update_is_kept_as_its_outcome_and_never_retried_by_itself,
    async {
        let updating = asked_update().await;
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "rolledBack")
        );

        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        let again = updating.asked().await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        assert_eq!(updating.phase().outcome().unwrap()["update"], "rolledBack");
        assert_eq!(again["state"], "refused", "{again}");
        assert_eq!(updating.updates_asked().len(), 1);
        assert_eq!(updating.stage_lanes(), 1);
        assert!(updating.pending().is_empty());
    }
);

updating_test!(
    a_guest_released_onto_another_build_than_this_keepers_is_said_so,
    async {
        let updating = asked_update().await;
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "released")
        );

        updating.says_build(&"c0".repeat(32)).await;
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        let answer = updating.asked().await;

        assert_eq!(answer["state"], "refused", "{answer}");
        assert_eq!(
            answer["why"],
            "work's Atlas was updated, but it does not say it runs this keeper's build."
        );
    }
);

updating_test!(
    the_staged_bundle_is_removed_as_a_change_once_agents_resume,
    async {
        let updating = asked_update().await;
        let refused = updating.resumed().await;
        assert_eq!(
            refused["outcome"]["refused"]["why"], "update",
            "an update unsettled: {refused}"
        );
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "released")
        );
        updating.says_build(&updating.own()).await;
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(updating.standin().copies().len(), 1, "kept while stopped");

        updating.confirmed().await;
        let resumed = updating.resumed().await;
        assert_eq!(resumed["outcome"], "resumed", "{resumed}");
        updating
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;

        assert!(updating.standin().copies().is_empty());
        assert_eq!(updating.phase().outcome().unwrap()["cleanup"], "removed");
        updating.nothing_pending().await;
    }
);

/// A second build: a keeper running it is not the one that began an
/// update of the first.
fn another_build(updating: &Updating) -> (Bundle, Program) {
    let other = bundle_of("/usr/bin/false");
    let program = Program::signed_at(&other.executable);
    assert_ne!(
        program.build().digest,
        updating.keeper().program.build().digest
    );
    (other, program)
}

updating_test!(
    an_update_another_build_began_and_never_asked_is_never_asked_by_the_next_keeper,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let first = {
            let mut owner = updating.paused().await;
            updating.begun(&mut owner, 2).await;
            updating.phase()
        };
        let Phase::Staged { staged, .. } = first else {
            panic!("{first:?}");
        };
        let (_other, program) = another_build(&updating);

        let updating = updating.restarted_as(program).await;
        updating.confirmed().await;
        let fresh = updating.asked().await;

        assert_eq!(fresh["state"], "updating", "{fresh}");
        let asked = updating.updates_asked();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_ne!(
            asked[0]["staged"],
            json!(staged.path),
            "never the first build's"
        );
        assert_eq!(updating.record().unwrap().target.build, updating.own());
    }
);

updating_test!(
    an_update_another_build_asked_with_its_outcome_unknown_is_only_read_by_the_next_keeper,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        let (_other, program) = another_build(&updating);

        let updating = updating.restarted_as(program).await;
        updating.confirmed().await;
        let owed = updating.asked().await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        assert_eq!(owed["state"], "owed", "{owed}");
        assert_eq!(updating.updates_asked().len(), 1, "never said again");
        assert_eq!(updating.update(), update);
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
        // The guest settles it as it settles any; only read here.
        assert!(updating.standin().settle_update(&update, "refused"));
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        updating.nothing_pending().await;
        assert_eq!(updating.updates_asked().len(), 1);
    }
);

/// Where a crash, or a sync that fails, leaves an answer of the guest's.
#[derive(Clone, Copy, Debug)]
enum Boundary {
    /// The answer came; the keeper ended before it kept it.
    BeforeKept,
    /// Kept, its folder's sync failing.
    SyncFails,
    /// Kept for good; the keeper ended before it cleared the claim.
    BeforeCleared,
}

const BOUNDARIES: [Boundary; 3] = [
    Boundary::BeforeKept,
    Boundary::SyncFails,
    Boundary::BeforeCleared,
];

updating_test!(
    a_stage_answer_survives_a_crash_or_a_failed_sync_at_each_boundary_and_nothing_is_sent_twice,
    async {
        for boundary in BOUNDARIES {
            let updating = Updating::new().await;
            updating.stopped().await;
            let restart = {
                let mut owner = updating.paused().await;
                updating.begun(&mut owner, 1).await;
                let Phase::Staging { stage, .. } = updating.phase() else {
                    panic!("{boundary:?}");
                };
                match boundary {
                    Boundary::SyncFails => {
                        // Its name bound to its snapshot first; then its
                        // answer's write, whose folder sync fails.
                        let folder = updating.keeper().folder.clone();
                        record::writes::hold_after(&folder, 1);
                        let (stepped, ()) = tokio::join!(updating.step(&mut owner), async {
                            let waiting = folder.clone();
                            until("the answer's write", move || {
                                record::writes::waiting(&waiting)
                            })
                            .await;
                            updating.syncable(false);
                            record::writes::release(&folder);
                        });
                        assert_eq!(stepped, Progress::Waiting);
                        updating.syncable(true);
                        assert!(!owner.durable, "{boundary:?}");
                        assert!(updating.pending().contains(&stage.as_str().to_owned()));
                        false
                    }
                    Boundary::BeforeKept => {
                        assert_eq!(updating.step(&mut owner).await, Progress::Moved);
                        updating.unkept_stage();
                        true
                    }
                    Boundary::BeforeCleared => {
                        assert_eq!(updating.step(&mut owner).await, Progress::Moved);
                        updating.claim(&stage);
                        true
                    }
                }
            };
            let updating = if restart {
                updating.restarted().await
            } else {
                updating
            };

            updating
                .until_phase("asked", |phase| matches!(phase, Phase::Asked { .. }))
                .await;
            let stage = match updating.phase() {
                Phase::Asked { stage, .. } => stage,
                other => panic!("{boundary:?}: {other:?}"),
            };
            assert_eq!(updating.standin().received.load(SeqCst), 1, "{boundary:?}");
            assert_eq!(updating.stage_lanes(), 1, "{boundary:?}");
            assert!(
                !updating.pending().contains(&stage.as_str().to_owned()),
                "{boundary:?}"
            );
        }
    }
);

updating_test!(
    an_update_answer_survives_a_crash_or_a_failed_sync_at_each_boundary_and_nothing_is_sent_twice,
    async {
        for boundary in BOUNDARIES {
            let updating = asked_update().await;
            let update = updating.update();
            let restart = {
                let mut owner = updating.paused().await;
                assert!(updating.standin().settle_update(&update, "released"));
                match boundary {
                    Boundary::BeforeKept => true,
                    Boundary::SyncFails => {
                        updating.syncable(false);
                        assert_eq!(updating.step(&mut owner).await, Progress::Waiting);
                        updating.syncable(true);
                        assert!(!owner.durable, "{boundary:?}");
                        assert!(updating.pending().contains(&update.as_str().to_owned()));
                        false
                    }
                    Boundary::BeforeCleared => {
                        assert_eq!(updating.step(&mut owner).await, Progress::Moved);
                        updating.claim(&update);
                        true
                    }
                }
            };
            let updating = if restart {
                updating.restarted().await
            } else {
                updating
            };

            updating
                .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
                .await;
            updating.nothing_pending().await;
            assert_eq!(updating.updates_asked().len(), 1, "{boundary:?}");
            assert_eq!(
                updating.phase().outcome().unwrap()["update"],
                "released",
                "{boundary:?}"
            );
        }
    }
);

/// Where an update stands when the person resumes agents.
#[derive(Clone, Copy, Debug)]
enum Resumed {
    Intended,
    StagedUnkept,
    Staged,
    AskedUnsent,
    Settled,
    Cleaning,
}

updating_test!(
    a_resume_with_a_keeper_restart_at_each_phase_never_asks_the_update_and_cleans_up,
    async {
        for at in [
            Resumed::Intended,
            Resumed::StagedUnkept,
            Resumed::Staged,
            Resumed::AskedUnsent,
            Resumed::Settled,
            Resumed::Cleaning,
        ] {
            let updating = Updating::new().await;
            updating.stopped().await;
            // Held until the keeper ended, so it takes no step of its own.
            let keeper = updating.keeper().clone();
            let mut owner = keeper.owner.lock().await;
            {
                match at {
                    Resumed::Intended => updating.begun(&mut owner, 0).await,
                    Resumed::StagedUnkept => {
                        updating.begun(&mut owner, 2).await;
                        updating.unkept_stage();
                    }
                    Resumed::Staged => updating.begun(&mut owner, 2).await,
                    Resumed::AskedUnsent => updating.begun(&mut owner, 3).await,
                    Resumed::Settled | Resumed::Cleaning => {
                        updating.begun(&mut owner, 3).await;
                        assert_eq!(updating.step(&mut owner).await, Progress::Waiting);
                        assert!(
                            updating
                                .standin()
                                .settle_update(&updating.update(), "released")
                        );
                        assert_eq!(updating.step(&mut owner).await, Progress::Moved);
                        if let (
                            Resumed::Cleaning,
                            Phase::Settled {
                                stage,
                                update,
                                outcome,
                            },
                        ) = (at, updating.phase())
                        {
                            let node = updating.standin().copies()[&stage].node.clone();
                            let remove = Token::new();
                            updating.rewrite(Phase::Cleaning {
                                stage,
                                update,
                                outcome,
                                node,
                                remove: remove.clone(),
                            });
                            updating.claim(&remove);
                        }
                    }
                }
            }
            let asked_before = updating.updates_asked().len();
            let resumed = updating.resumed().await;
            assert_eq!(resumed["outcome"], "resumed", "{at:?}: {resumed}");

            let updating = updating.restarted().await;
            drop(owner);
            let folder = updating.keeper().folder.clone();
            until("done or dropped", move || {
                Record::kept(&folder)
                    .unwrap()
                    .is_none_or(|record| matches!(record.phase, Phase::Done { .. }))
            })
            .await;

            match at {
                Resumed::Intended => assert!(updating.record().is_none(), "{at:?}"),
                _ => assert!(matches!(updating.phase(), Phase::Done { .. }), "{at:?}"),
            }
            assert_eq!(updating.updates_asked().len(), asked_before, "{at:?}");
            assert!(updating.standin().copies().is_empty(), "{at:?}");
            updating.nothing_pending().await;
        }
    }
);

updating_test!(
    another_bundle_swapped_into_the_guests_staged_copy_is_refused_changed_and_never_asked_again,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        {
            let mut owner = updating.paused().await;
            updating.begun(&mut owner, 2).await;
            let Phase::Staged { stage, .. } = updating.phase() else {
                panic!("not staged");
            };
            // Another signed build placed where the guest keeps the
            // station's copy.
            let other = bundle_of("/usr/bin/false");
            let swapped =
                atlas_core::stage::Snapshot::of(other.executable.ancestors().nth(3).unwrap())
                    .unwrap();
            updating
                .standin()
                .copies()
                .get_mut(&stage)
                .unwrap()
                .snapshot = swapped;
        }

        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let answer = updating.asked().await;

        assert_eq!(updating.phase().outcome().unwrap()["update"], "changed");
        assert_eq!(updating.updates_asked().len(), 1, "never asked again");
        assert!(
            updating.standin().updates.lock().unwrap().is_empty(),
            "nothing taken"
        );
        assert_eq!(answer["state"], "refused", "{answer}");
        assert_eq!(updating.stage_lanes(), 1);
    }
);

updating_test!(
    a_guest_whose_updates_are_not_bound_to_their_stage_is_never_asked_one,
    async {
        let updating = Updating::new().await;
        updating
            .standin()
            .undeclared
            .lock()
            .unwrap()
            .push(UPDATE_BOUND.to_owned());
        restarted(&updating.serving).await;
        updating.stopped().await;

        let refused = updating.asked().await;

        assert_eq!(refused["code"], "update", "{refused}");
        assert_eq!(
            refused["error"],
            "work's Atlas cannot be updated from this Mac, so nothing was staged."
        );
        assert_eq!(
            refused["next"],
            "work's Atlas cannot be updated from this Mac. Remove the VM and add it again."
        );
        assert!(updating.record().is_none());
        assert_eq!(updating.stage_lanes(), 0);
        assert!(updating.updates_asked().is_empty());
    }
);

updating_test!(
    an_update_is_refused_while_a_maintenance_is_kept_in_words_saying_to_end_it_first,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        {
            let mut owner = updating.paused().await;
            updating.begun(&mut owner, 2).await;
        }
        let keeper = updating.keeper();
        crate::test_support::under_maintenance(
            keeper.lifecycle.home(),
            &keeper.vm,
            crate::vm::lifecycle::record::Phase::Open,
        );

        let refused = updating.asked().await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        assert_eq!(refused["code"], "busy", "{refused}");
        assert_eq!(
            refused["error"],
            "work is under maintenance, so its Atlas is not updated."
        );
        assert_eq!(
            refused["next"],
            "End it with atlas desktop maintain work --end, then run atlas update."
        );
        assert!(matches!(updating.phase(), Phase::Staged { .. }), "held");
        assert!(updating.updates_asked().is_empty());
    }
);

updating_test!(
    an_ordinary_recovery_leaves_every_change_an_update_names_to_its_owner,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        let _owner = updating.paused().await;
        assert!(updating.standin().settle_update(&update, "released"));

        let recovered = updating
            .keeper()
            .recovered(&updating.session(), Duration::from_secs(2), true)
            .await
            .unwrap();

        assert!(recovered.cleared.is_empty(), "{recovered:?}");
        assert_eq!(recovered.kept, vec![update.as_str().to_owned()]);
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
    }
);

updating_test!(
    an_update_record_that_cannot_be_read_leaves_every_unheld_claim_untouched,
    async {
        let updating = Updating::new().await;
        let change = Token::new();
        updating.standin().ask(&change);
        updating.standin().done(&change, &json!({"ok": true}));
        updating.claim(&change);
        let _owner = updating.paused().await;
        std::fs::write(updating.keeper().folder.join(record::RECORD), b"{damaged").unwrap();

        let recovered = updating
            .keeper()
            .recovered(&updating.session(), Duration::from_secs(2), true)
            .await
            .unwrap();

        assert!(recovered.cleared.is_empty(), "{recovered:?}");
        assert_eq!(recovered.kept, vec![change.as_str().to_owned()]);
        assert_eq!(updating.pending(), vec![change.as_str().to_owned()]);
        std::fs::remove_file(updating.keeper().folder.join(record::RECORD)).unwrap();
        let recovered = updating
            .keeper()
            .recovered(&updating.session(), Duration::from_secs(2), true)
            .await
            .unwrap();
        assert_eq!(recovered.cleared, vec![change]);
    }
);

#[test]
fn nothing_an_update_says_of_a_vm_says_helper() {
    let said = [
        serde_json::to_string(
            &crate::vm::maintenance::Kept::Unreadable
                .update_refused("work")
                .unwrap(),
        )
        .unwrap(),
        not_asked("Agents resumed before the update was asked.").to_string(),
        not_staged(&json!({"error": "Nothing was staged: the station hung up"})).to_string(),
    ];

    for said in said {
        assert!(!said.to_lowercase().contains("helper"), "{said}");
    }
}

/// The guest's answers to reads of what a change came to, held while a test
/// holds them, as a guest slow to answer would.
#[derive(Default)]
struct Reads {
    holding: std::sync::atomic::AtomicBool,
    held: std::sync::atomic::AtomicUsize,
}

impl Updating {
    /// Hold the guest's answer to each read of a change it never heard of,
    /// from now until the test lets them go.
    fn holding_outcomes(&self) -> Arc<Reads> {
        let reads = Arc::new(Reads::default());
        reads.holding.store(true, SeqCst);
        let held = reads.clone();
        let answer: atlas_session::standin::serve::Answer = Arc::new(move |_, request: Value| {
            if request["op"] != "outcome" {
                return json!({"ok": true, "echo": request});
            }
            if held.holding.load(SeqCst) {
                held.held.fetch_add(1, SeqCst);
                // Off the runtime's other work, and bounded, so a test that
                // fails never holds its runtime.
                tokio::task::block_in_place(|| {
                    for _ in 0..2000 {
                        if !held.holding.load(SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                });
            }
            json!({"ok": true, "outcome": "unknown"})
        });
        *self.saying.otherwise.lock().unwrap() = Some(answer);
        reads
    }

    /// Wait until a read the test holds is being answered.
    async fn until_held(&self, reads: &Arc<Reads>) {
        let reads = reads.clone();
        until("a read held", move || reads.held.load(SeqCst) > 0).await;
    }

    /// Take the update to its update kept and not said, the owner held only
    /// while it is, and hold the guest's answers to reads of what that
    /// update came to before the owner goes on.
    async fn asked_unsent(&self) -> Arc<Reads> {
        let mut owner = self.paused().await;
        self.begun(&mut owner, 3).await;
        assert!(matches!(self.phase(), Phase::Asked { .. }));
        self.holding_outcomes()
    }
}

updating_test!(
    a_generation_lost_mid_write_has_the_next_wait_for_it_and_nothing_of_the_old_lands_after,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        {
            let mut owner = updating.paused().await;
            updating.begun(&mut owner, 2).await;
        }
        let folder = updating.keeper().folder.clone();
        // The record kept again, then the update named: that write is slow.
        record::writes::hold_after(&folder, 1);
        let mut owner = updating.paused().await;
        owner.durable = false;
        drop(owner);
        updating.keeper().owner_waking.notify_one();
        let waiting = folder.clone();
        until("a write under way", move || {
            record::writes::waiting(&waiting)
        })
        .await;

        restarted(&updating.serving).await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(
            matches!(updating.phase(), Phase::Staged { .. }),
            "nothing lands meanwhile"
        );
        assert!(updating.updates_asked().is_empty(), "the next waits for it");
        record::writes::release(&folder);

        let standin = updating.serving.fixture.guest.standin.clone();
        until("asked", move || {
            standin.updates_asked.lock().unwrap().len() == 1
        })
        .await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let asked = updating.updates_asked();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(
            asked[0]["change"],
            json!(updating.update()),
            "the one the record names"
        );
    }
);

updating_test!(
    a_keeper_replaced_while_it_reads_what_its_update_came_to_never_says_it,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let reads = updating.asked_unsent().await;
        updating.until_held(&reads).await;

        let other = updating.bundle.executable.with_file_name("atlas.new");
        std::fs::copy("/usr/bin/false", &other).unwrap();
        std::fs::rename(&other, &updating.bundle.executable).unwrap();
        reads.holding.store(false, SeqCst);

        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        assert!(updating.updates_asked().is_empty(), "never said");
        assert_eq!(updating.phase().outcome().unwrap()["update"], "notAsked");
        assert!(
            updating
                .standin()
                .refused_as_voided(&updating.update())
                .is_some()
        );
        updating.nothing_pending().await;
    }
);

updating_test!(
    a_maintenance_or_a_resume_begun_while_the_update_is_about_to_be_said_keeps_it_unsaid,
    async {
        // A maintenance: held, unsaid.
        let updating = Updating::new().await;
        updating.stopped().await;
        let reads = updating.asked_unsent().await;
        updating.until_held(&reads).await;
        let keeper = updating.keeper();
        crate::test_support::under_maintenance(
            keeper.lifecycle.home(),
            &keeper.vm,
            crate::vm::lifecycle::record::Phase::Open,
        );
        reads.holding.store(false, SeqCst);
        tokio::time::sleep(Duration::from_secs(4)).await;

        assert!(updating.updates_asked().is_empty());
        assert!(matches!(updating.phase(), Phase::Asked { .. }));

        // A Resume: never said, then voided.
        let resumed = Updating::new().await;
        resumed.stopped().await;
        let reads = resumed.asked_unsent().await;
        resumed.until_held(&reads).await;
        let answer = resumed.resumed().await;
        assert_eq!(answer["outcome"], "resumed", "{answer}");
        reads.holding.store(false, SeqCst);

        resumed
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;
        assert!(resumed.updates_asked().is_empty());
        assert!(resumed.standin().copies().is_empty(), "its copy removed");
    }
);

updating_test!(
    a_maintenances_hold_cancels_a_stage_under_way_and_answers_once_its_lane_is_gone,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        // More than the bulk credit, so its bytes wait on the guest.
        let app = updating.bundle.executable.ancestors().nth(2).unwrap();
        std::fs::create_dir_all(app.join("Resources")).unwrap();
        std::fs::write(app.join("Resources/large"), vec![7_u8; 4 << 20]).unwrap();
        updating.standin().withholding.store(true, SeqCst);
        let staging = asked(
            &updating.serving.socket,
            &json!({"op": "update-guest", "deadline_ms": 2_000}),
        )
        .await
        .pop()
        .unwrap();
        assert_eq!(staging["state"], "staging", "{staging}");
        let keeper = updating.keeper();
        let change = crate::test_support::under_maintenance(
            keeper.lifecycle.home(),
            &keeper.vm,
            crate::vm::lifecycle::record::Phase::Holding,
        );

        let held = asked(
            &updating.serving.socket,
            &json!({"op": "hold", "change": change, "deadline_ms": 10_000}),
        )
        .await
        .pop()
        .unwrap();

        assert_eq!(held["ok"], true, "{held}");
        assert_eq!(held["cancelled"]["update"], 1, "{held}");
        assert!(updating.standin().copies().is_empty(), "nothing placed");
        assert!(updating.updates_asked().is_empty());
        updating.standin().give_credit_back();
    }
);

updating_test!(
    an_update_the_guest_took_is_never_voided_whatever_a_resume_says,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        // The station's Stop lifted, and a guest that says it resumed.
        let revision = updating.keeper().stop.stop_revision().unwrap();
        updating.keeper().stop.lift_stop(revision.as_ref()).unwrap();
        *updating.serving.fixture.guest.stop.lock().unwrap() = "clear";

        let updating = updating.restarted().await;
        tokio::time::sleep(Duration::from_secs(4)).await;

        assert!(
            matches!(updating.phase(), Phase::Asked { .. }),
            "still pending"
        );
        assert_eq!(updating.pending(), vec![update.as_str().to_owned()]);
        assert!(
            updating.standin().unsettled_update(&update),
            "the guest's own"
        );
        assert!(updating.standin().refused_as_voided(&update).is_none());
    }
);

updating_test!(
    a_stage_never_said_before_a_resume_is_voided_so_none_comes_later,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let keeper = updating.keeper().clone();
        let stage = {
            let mut owner = keeper.owner.lock().await;
            updating.begun(&mut owner, 1).await;
            let Phase::Staging { stage, .. } = updating.phase() else {
                panic!("not staging");
            };
            updating.claim(&stage);
            let answer = updating.resumed().await;
            assert_eq!(answer["outcome"], "resumed", "{answer}");
            stage
        };

        let updating = updating.restarted().await;
        updating
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;

        assert_eq!(updating.phase().outcome().unwrap()["update"], "notStaged");
        assert!(updating.standin().refused_as_voided(&stage).is_some());
        assert_eq!(updating.stage_lanes(), 0);
        updating.nothing_pending().await;
    }
);

updating_test!(
    a_failed_sync_then_a_restart_keeps_the_claim_until_the_record_is_made_to_last,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        let keeper = updating.keeper().clone();
        {
            let mut owner = keeper.owner.lock().await;
            assert!(updating.standin().settle_update(&update, "released"));
            updating.syncable(false);
            assert_eq!(updating.step(&mut owner).await, Progress::Waiting);
            assert!(
                matches!(updating.phase(), Phase::Settled { .. }),
                "in place"
            );
            assert!(updating.pending().contains(&update.as_str().to_owned()));
        }

        let updating = updating.restarted().await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(
            updating.pending().contains(&update.as_str().to_owned()),
            "kept while the record is not known to last"
        );
        updating.syncable(true);

        updating.nothing_pending().await;
        assert_eq!(updating.updates_asked().len(), 1);
    }
);

updating_test!(
    a_fresh_update_in_place_of_another_builds_keeps_its_copy_to_remove_until_that_is_answered,
    async {
        let updating = asked_update().await;
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "released")
        );
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        // Its removal named, never said, before the keeper was replaced.
        let earlier = {
            let keeper = updating.keeper().clone();
            let _owner = keeper.owner.lock().await;
            let Phase::Settled {
                stage,
                update,
                outcome,
            } = updating.phase()
            else {
                unreachable!()
            };
            let node = updating.standin().copies()[&stage].node.clone();
            let remove = Token::new();
            updating.rewrite(Phase::Cleaning {
                stage: stage.clone(),
                update,
                outcome,
                node,
                remove: remove.clone(),
            });
            updating.claim(&remove);
            stage
        };
        let (_other, program) = another_build(&updating);

        let updating = updating.restarted_as(program).await;
        updating.confirmed().await;
        let fresh = updating.asked().await;

        assert_eq!(fresh["state"], "updating", "{fresh}");
        let record = updating.record().unwrap();
        assert_eq!(record.earlier.len(), 1, "{record:?}");
        assert_eq!(record.earlier[0].stage, earlier);
        assert_eq!(updating.standin().copies().len(), 2, "both copies kept");

        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "released")
        );
        updating.says_build(&updating.own()).await;
        updating.confirmed().await;
        let resumed = updating.resumed().await;
        assert_eq!(resumed["outcome"], "resumed", "{resumed}");
        updating
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;
        let folder = updating.keeper().folder.clone();
        until("the earlier copy removed", move || {
            Record::kept(&folder)
                .unwrap()
                .is_some_and(|record| record.earlier.is_empty())
        })
        .await;

        assert!(updating.standin().copies().is_empty(), "both removed");
        updating.nothing_pending().await;
    }
);

updating_test!(the_persons_own_stop_never_lets_an_update_be_asked, async {
    let updating = Updating::new().await;
    updating.keeper().stop.note_stop(true).unwrap();
    *updating.serving.fixture.guest.stop.lock().unwrap() = "stopped";
    updating.confirmed().await;

    let refused = updating.asked().await;

    assert_eq!(refused["code"], "stopped", "{refused}");
    assert_eq!(
        refused["error"],
        "Agents on work are not stopped for an update, so its Atlas is not updated."
    );
    assert!(updating.record().is_none());
    assert_eq!(updating.stage_lanes(), 0);
});

updating_test!(
    a_keeper_replaced_while_its_updates_lane_opens_never_says_it,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let reads = updating.asked_unsent().await;
        reads.holding.store(false, SeqCst);
        // The outcome read's lane opens; the update's waits.
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        let guest = updating.serving.fixture.guest.clone();
        guest.lanes_waiting.store(0, SeqCst);
        *guest.lanes_gate.lock().unwrap() = Some(gate.clone());
        updating.keeper().owner_waking.notify_one();
        until("the update's lane opening", move || {
            guest.lanes_waiting.load(SeqCst) >= 2
        })
        .await;

        let other = updating.bundle.executable.with_file_name("atlas.new");
        std::fs::copy("/usr/bin/false", &other).unwrap();
        std::fs::rename(&other, &updating.bundle.executable).unwrap();
        *updating.serving.fixture.guest.lanes_gate.lock().unwrap() = None;
        gate.add_permits(64);

        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        assert!(updating.updates_asked().is_empty(), "never said");
        assert_eq!(updating.phase().outcome().unwrap()["update"], "notAsked");
    }
);

updating_test!(
    a_maintenance_begun_between_the_tickets_admission_and_the_claim_keeps_the_update_unsaid,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let reads = updating.asked_unsent().await;
        reads.holding.store(false, SeqCst);
        // A maintenance being begun holds the desktop's exclusion, and
        // writes its record under it.
        let exclusion = updating.keeper().stop.acquire_for_executor().unwrap();
        updating.keeper().owner_waking.notify_one();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let keeper = updating.keeper();
        crate::test_support::under_maintenance(
            keeper.lifecycle.home(),
            &keeper.vm,
            crate::vm::lifecycle::record::Phase::Holding,
        );
        drop(exclusion);

        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(updating.updates_asked().is_empty(), "never said");
        assert!(matches!(updating.phase(), Phase::Asked { .. }));
        assert!(
            !updating
                .pending()
                .contains(&updating.update().as_str().to_owned()),
            "not even claimed"
        );
    }
);

updating_test!(
    a_generation_lost_while_a_write_whose_sync_fails_is_under_way_keeps_the_claim,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        let folder = updating.keeper().folder.clone();
        record::writes::hold_after(&folder, 0);
        assert!(updating.standin().settle_update(&update, "released"));
        updating.keeper().owner_waking.notify_one();
        let waiting = folder.clone();
        until("the answer's write under way", move || {
            record::writes::waiting(&waiting)
        })
        .await;
        updating.syncable(false);

        restarted(&updating.serving).await;
        record::writes::release(&folder);
        tokio::time::sleep(Duration::from_secs(3)).await;

        assert!(
            matches!(updating.phase(), Phase::Settled { .. }),
            "in place"
        );
        assert!(
            updating.pending().contains(&update.as_str().to_owned()),
            "kept while the record is not known to last"
        );
        let answer = updating.asked().await;
        assert_eq!(answer["state"], "updating", "no outcome said yet: {answer}");
        updating.syncable(true);
        updating.nothing_pending().await;
    }
);

updating_test!(
    an_outcome_is_answered_only_from_a_record_known_to_last,
    async {
        let updating = asked_update().await;
        let update = updating.update();
        let keeper = updating.keeper().clone();
        {
            let mut owner = keeper.owner.lock().await;
            assert!(updating.standin().settle_update(&update, "released"));
            updating.syncable(false);
            assert_eq!(updating.step(&mut owner).await, Progress::Waiting);
        }
        updating.says_build(&updating.own()).await;

        let unsure = asked(
            &updating.serving.socket,
            &json!({"op": "update-guest", "deadline_ms": 1_000}),
        )
        .await
        .pop()
        .unwrap();
        updating.syncable(true);
        updating.nothing_pending().await;
        let sure = updating.asked().await;

        assert_eq!(unsure["state"], "updating", "{unsure}");
        assert_eq!(sure["state"], "updated", "{sure}");
    }
);

updating_test!(
    a_void_whose_reply_was_lost_is_read_back_as_a_void_after_a_restart_too,
    async {
        // An update never said, voided once agents resumed, its reply lost.
        let updating = Updating::new().await;
        updating.stopped().await;
        let reads = updating.asked_unsent().await;
        reads.holding.store(false, SeqCst);
        let keeper = updating.keeper().clone();
        let owner = keeper.owner.lock().await;
        let answer = updating.resumed().await;
        assert_eq!(answer["outcome"], "resumed", "{answer}");
        updating.standin().dropping.store(1, SeqCst);
        drop(owner);
        let updating = updating.restarted().await;
        updating
            .until_phase("settled or done", |phase| {
                matches!(phase, Phase::Settled { .. } | Phase::Done { .. })
            })
            .await;
        assert_eq!(updating.phase().outcome().unwrap()["update"], "notAsked");
        assert_ne!(updating.phase().outcome().unwrap()["ok"], true);
        updating
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;
        assert!(updating.standin().copies().is_empty());

        // A removal never said, voided, its reply lost: the copy stays for a
        // removal of the keeper's own.
        let cleaning = asked_update().await;
        assert!(
            cleaning
                .standin()
                .settle_update(&cleaning.update(), "released")
        );
        cleaning
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        cleaning.says_build(&cleaning.own()).await;
        cleaning.confirmed().await;
        let keeper = cleaning.keeper().clone();
        let owner = keeper.owner.lock().await;
        let Phase::Settled {
            stage,
            update,
            outcome,
        } = cleaning.phase()
        else {
            unreachable!()
        };
        let node = cleaning.standin().copies()[&stage].node.clone();
        cleaning.rewrite(Phase::Cleaning {
            stage,
            update,
            outcome,
            node,
            remove: Token::new(),
        });
        let resumed = cleaning.resumed().await;
        assert_eq!(resumed["outcome"], "resumed", "{resumed}");
        cleaning.standin().dropping.store(1, SeqCst);
        let (_other, program) = another_build(&cleaning);
        drop(owner);
        let cleaning = cleaning.restarted_as(program).await;

        cleaning
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;
        assert!(cleaning.standin().copies().is_empty(), "removed afresh");
        assert_eq!(cleaning.phase().outcome().unwrap()["cleanup"], "removed");
        cleaning.nothing_pending().await;
    }
);

updating_test!(
    a_stage_bound_to_another_snapshot_is_never_said_again_under_its_name,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let keeper = updating.keeper().clone();
        let stage = {
            let _owner = keeper.owner.lock().await;
            let stage = Token::new();
            updating.rewrite(Phase::Staging {
                stage: stage.clone(),
                digest: Some("ab".repeat(32)),
            });
            updating.claim(&stage);
            stage
        };

        let updating = updating.restarted().await;
        updating
            .until_phase("done", |phase| matches!(phase, Phase::Done { .. }))
            .await;

        assert_eq!(updating.stage_lanes(), 0, "never said under its name");
        assert_eq!(updating.phase().outcome().unwrap()["update"], "notStaged");
        assert!(updating.standin().refused_as_voided(&stage).is_some());
        updating.nothing_pending().await;
    }
);

updating_test!(
    a_stage_whose_lane_never_opened_is_voided_in_the_same_generation_once_agents_resume,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let generation = updating.session().control.generation();
        let keeper = updating.keeper().clone();
        let mut owner = keeper.owner.lock().await;
        updating.begun(&mut owner, 1).await;
        let Phase::Staging { stage, .. } = updating.phase() else {
            panic!("not staging");
        };
        // The outcome read's lane opens; the stage's fails to.
        let guest = updating.serving.fixture.guest.clone();
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        guest.lanes_waiting.store(0, SeqCst);
        *guest.lanes_gate.lock().unwrap() = Some(gate.clone());

        let (stepped, ()) = tokio::join!(updating.step(&mut owner), async {
            let waiting = guest.clone();
            until("the stage's lane opening", move || {
                waiting.lanes_waiting.load(SeqCst) >= 2
            })
            .await;
            guest.lanes_failing.store(1, SeqCst);
            *guest.lanes_gate.lock().unwrap() = None;
            gate.add_permits(64);
        });

        assert_eq!(stepped, Progress::Waiting);
        assert!(owner.said.is_none(), "its lane never opened");
        assert_eq!(updating.stage_lanes(), 0);
        let answer = updating.resumed().await;
        assert_eq!(answer["outcome"], "resumed", "{answer}");

        assert_eq!(updating.step(&mut owner).await, Progress::Moved);

        assert_eq!(updating.session().control.generation(), generation);
        assert_eq!(updating.phase().outcome().unwrap()["update"], "notStaged");
        assert!(updating.standin().refused_as_voided(&stage).is_some());
    }
);

/// The keeper of `updating` as the station asks it, over its own socket.
struct Through(crate::link::Keeper);

impl Through {
    fn of(updating: &Updating, directory: &std::path::Path) -> Self {
        let selected = crate::test_support::vm_client(directory, None).selected;
        Self(crate::link::Keeper::at(
            Arc::new(selected),
            updating.keeper().vm.clone(),
            updating.serving.socket.clone(),
            Duration::from_secs(10),
            Arc::new(crate::vm::keeper::clients::tests::Trusting),
        ))
    }
}

impl crate::vm::handover::Keepers for Through {
    fn loaded(&self, _: &atlas_core::desktop::DesktopId) -> atlas_core::error::Result<bool> {
        Ok(true)
    }

    fn runs(&self, _: &atlas_core::desktop::DesktopId) -> bool {
        true
    }

    fn ask(
        &self,
        _: &atlas_core::desktop::DesktopId,
        request: &Value,
        deadline: Duration,
    ) -> atlas_core::error::Result<Vec<Value>> {
        self.0.exchange(request, deadline)
    }

    fn paired(&self, _: &atlas_core::desktop::DesktopId) -> atlas_core::error::Result<bool> {
        Ok(true)
    }

    fn ended(&self, _: &atlas_core::process::Instance) -> bool {
        false
    }

    fn stopped(&self, _: &atlas_core::desktop::DesktopId) -> bool {
        false
    }
}

updating_test!(
    the_stations_step_has_the_keeper_bring_its_guest_to_its_own_build_once_the_updates_stop_holds,
    async {
        let updating = Updating::new().await;
        let directory = tempfile::tempdir().unwrap();
        let keeper = updating.keeper().clone();
        // The update's own Stop, as `atlas update` keeps it; the keeper says
        // it to its guest by itself, and the step asks until it is confirmed.
        keeper.stop.note_stop_unless_noted(StopWhy::Update).unwrap();
        *updating.serving.fixture.guest.stop.lock().unwrap() = "stopped";
        let through = Through::of(&updating, directory.path());
        let desk = crate::vm::handover::Desk::at(
            "work",
            keeper.vm.clone(),
            keeper.stop.clone(),
            keeper.folder.clone(),
        );
        let lifecycle = keeper.lifecycle.clone();

        let answered = tokio::task::spawn_blocking(move || {
            crate::vm::handover::update_guest_through(
                &through,
                &desk,
                &lifecycle,
                false,
                Duration::from_secs(20),
            )
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(answered["state"], "updating", "{answered}");
        assert_eq!(answered["desktop"], "work");
        let Phase::Asked { staged, update, .. } = updating.phase() else {
            panic!("not asked: {:?}", updating.phase());
        };
        let asked = updating.updates_asked();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(asked[0]["change"], json!(update));
        assert_eq!(asked[0]["expect"], json!(staged.digest));
        assert_eq!(updating.record().unwrap().target.build, updating.own());
    }
);

updating_test!(
    status_says_an_update_under_way_with_its_phase_and_the_copies_earlier_updates_left,
    async {
        let updating = asked_update().await;
        let directory = tempfile::tempdir().unwrap();
        let client =
            crate::vm::keeper::clients::tests::client_of(&updating.serving, directory.path());
        let target = json!({"build": updating.own(), "version": atlas_core::VERSION});

        let under_way = client.status().await;

        assert_eq!(
            under_way["guest"]["update"],
            json!({"state": "updating", "phase": "asked", "target": target}),
            "{under_way}"
        );

        // Settled, a copy an earlier build's update left still to remove,
        // kept by the owner so it is known to last.
        let mut owner = updating.paused().await;
        let (stage, update, earlier) = (Token::new(), updating.update(), Token::new());
        let outcome = json!({"ok": false, "op": "update", "update": "rolledBack"});
        let settled = Record {
            target: updating.keeper().target().unwrap(),
            phase: Phase::Settled {
                stage,
                update: Some(update),
                outcome: outcome.clone(),
            },
            earlier: vec![Leftover {
                stage: earlier.clone(),
                removal: None,
            }],
        };
        assert!(updating.keeper().keep(&mut owner, &settled).await);

        let settled = client.status().await;

        assert_eq!(
            settled["guest"]["update"],
            json!({"state": "available",
                "next": "Run atlas update to bring work's Atlas to this Mac's build.",
                "phase": "settled", "outcome": outcome,
                "earlier": [{"stage": earlier, "removal": null}]}),
            "{settled}"
        );
        assert_eq!(settled["keeper"]["update"]["phase"], "settled");
        assert!(
            !settled.to_string().to_lowercase().contains("helper"),
            "{settled}"
        );
    }
);

updating_test!(
    the_stations_cleanup_leaves_a_copy_a_superseded_update_left_to_its_owner_which_removes_it,
    async {
        let updating = asked_update().await;
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "released")
        );
        updating.says_build(&updating.own()).await;
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        updating.nothing_pending().await;
        let owner = updating.paused().await;
        let Phase::Settled { stage, outcome, .. } = updating.phase() else {
            panic!("not settled");
        };
        // The copy is now one an earlier update left, as a fresh update
        // begun in place of a superseded one keeps it.
        Record {
            target: updating.keeper().target().unwrap(),
            phase: Phase::Done {
                issued: Vec::new(),
                outcome,
            },
            earlier: vec![Leftover {
                stage: stage.clone(),
                removal: None,
            }],
        }
        .keep(&updating.keeper().folder)
        .unwrap();
        updating.confirmed().await;
        let resumed = updating.resumed().await;
        assert_eq!(resumed["outcome"], "resumed", "{resumed}");
        let directory = tempfile::tempdir().unwrap();
        let client =
            crate::vm::keeper::clients::tests::client_of(&updating.serving, directory.path());
        let verified = client.verify_vm().await.unwrap();

        let cleaned = crate::vm::copies::removal(&client, &stage, (None, &verified), &[]).await;

        assert!(
            matches!(cleaned, crate::handoff::CopyRemoval::Unreadable),
            "never removed around its owner: {cleaned:?}"
        );
        assert_eq!(updating.standin().copies().len(), 1);

        drop(owner);
        let folder = updating.keeper().folder.clone();
        until("the earlier copy removed", move || {
            Record::kept(&folder)
                .ok()
                .flatten()
                .is_some_and(|record| record.earlier.is_empty())
        })
        .await;
        assert!(updating.standin().copies().is_empty());
        updating.nothing_pending().await;
    }
);

updating_test!(
    status_says_a_settlement_whose_record_may_not_last_is_still_settling,
    async {
        let updating = asked_update().await;
        let directory = tempfile::tempdir().unwrap();
        let client =
            crate::vm::keeper::clients::tests::client_of(&updating.serving, directory.path());
        let update = updating.update();
        {
            let mut owner = updating.paused().await;
            assert!(updating.standin().settle_update(&update, "rolledBack"));
            updating.syncable(false);
            assert_eq!(updating.step(&mut owner).await, Progress::Waiting);
            assert!(
                matches!(updating.phase(), Phase::Settled { .. }),
                "in place"
            );

            let unsure = client.status().await;

            assert_eq!(
                unsure["guest"]["update"],
                json!({"state": "updating", "phase": "settling",
                    "target": {"build": updating.own(), "version": atlas_core::VERSION}}),
                "{unsure}"
            );
            assert!(unsure["keeper"]["update"].get("outcome").is_none());
        }
        updating.syncable(true);
        updating.nothing_pending().await;

        let settled = client.status().await;

        assert_eq!(settled["guest"]["update"]["phase"], "settled", "{settled}");
        assert_eq!(
            settled["guest"]["update"]["outcome"]["update"],
            "rolledBack"
        );
    }
);

updating_test!(
    a_stage_under_way_is_answered_staging_in_plain_words,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        let _owner = updating.paused().await;
        let staging = Record {
            target: updating.keeper().target().unwrap(),
            phase: Phase::Staging {
                stage: Token::new(),
                digest: None,
            },
            earlier: Vec::new(),
        };

        let said = updating.keeper().said(Some(&staging), true);

        assert_eq!(
            said,
            json!({"ok": true, "op": "update-guest", "state": "staging",
                "why": "This Mac's Atlas is being copied into work; its Atlas is updated once \
                        the copy is in place."})
        );
    }
);

updating_test!(
    an_update_of_this_build_that_failed_is_begun_again_only_when_the_person_asks_to_retry,
    async {
        let updating = asked_update().await;
        assert!(
            updating
                .standin()
                .settle_update(&updating.update(), "rolledBack")
        );
        updating
            .until_phase("settled", |phase| matches!(phase, Phase::Settled { .. }))
            .await;
        updating.nothing_pending().await;
        let first = updating.update();

        let again = updating.asked().await;
        assert_eq!(again["state"], "refused", "{again}");
        assert_eq!(updating.updates_asked().len(), 1);

        let retried = asked(
            &updating.serving.socket,
            &json!({"op": "update-guest", "retry": true, "deadline_ms": 30_000}),
        )
        .await
        .pop()
        .unwrap();

        assert_eq!(retried["state"], "updating", "{retried}");
        let asked = updating.updates_asked();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert_ne!(asked[1]["change"], json!(first), "under a fresh name");
        let record = updating.record().unwrap();
        assert_eq!(
            record.earlier.len(),
            1,
            "the failed one's copy kept to remove"
        );
    }
);

updating_test!(
    an_update_of_this_build_never_tried_in_the_guest_is_begun_again_by_a_plain_ask,
    async {
        let updating = Updating::new().await;
        updating.stopped().await;
        {
            let mut owner = updating.paused().await;
            let never = Record {
                target: updating.keeper().target().unwrap(),
                phase: Phase::Done {
                    issued: vec![Token::new()],
                    outcome: not_asked("Agents resumed before the update was asked."),
                },
                earlier: Vec::new(),
            };
            assert!(updating.keeper().keep(&mut owner, &never).await);
        }

        let answer = updating.asked().await;

        assert_eq!(answer["state"], "updating", "{answer}");
        assert_eq!(updating.updates_asked().len(), 1);
        assert!(matches!(updating.phase(), Phase::Asked { .. }));
    }
);
