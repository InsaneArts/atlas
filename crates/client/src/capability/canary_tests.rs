use super::*;
use std::cell::Cell;

/// A desktop whose fixture starts but never says it is ready, or never
/// starts: what the check asked of it is kept.
struct Stalled {
    display: Display,
    starts: bool,
    archives: bool,
    /// Whether cleanup was asked to remove the canary folder, once asked.
    removed: Cell<Option<bool>>,
    keys: Cell<usize>,
}

impl Stalled {
    fn new(starts: bool, archives: bool) -> Self {
        Self {
            display: Display {
                width: 1920.0,
                height: 1200.0,
                scale: 1.0,
            },
            starts,
            archives,
            removed: Cell::new(None),
            keys: Cell::new(0),
        }
    }
}

impl Desk for Stalled {
    fn display(&self) -> &Display {
        &self.display
    }

    async fn launch(&self) -> Result<Fixture> {
        if !self.starts {
            return Err(problem("The fixture could not be launched."));
        }
        Ok(Fixture {
            pid: 4242,
            program: PathBuf::from("/private/tmp/DesktopFixture"),
            directory: PathBuf::from("/private/tmp/atlas-canary.Ab12Cd"),
        })
    }

    async fn text(&self, _fixture: &Fixture, name: &str) -> Option<String> {
        // An older fixture: it runs, but says no panel instrumentation.
        (name == "state.json").then(|| json!({"pid": 4242, "windowX": 10.0}).to_string())
    }

    async fn observed(&self, _fixture: &Fixture, _attempt: &str) -> Result<Option<PanelScan>> {
        Ok(None)
    }

    async fn keys(&self, _actions: Value) -> Result<()> {
        self.keys.set(self.keys.get() + 1);
        Ok(())
    }

    async fn windows(&self) -> Result<Shown> {
        Err(problem("No windows here."))
    }

    async fn capture(&self, _name: &str) -> Option<PathBuf> {
        None
    }

    async fn archive(&self, _fixture: &Fixture) -> Result<Vec<PathBuf>> {
        if self.archives {
            Ok(vec![PathBuf::from("/records/native-1")])
        } else {
            Err(problem("The fixture's files could not be kept."))
        }
    }

    async fn cleanup(&self, _fixture: &Fixture, remove: bool) -> Result<()> {
        self.removed.set(Some(remove));
        Ok(())
    }

    async fn same_path(&self, _fixture: &Fixture, _path: Option<&str>, _expected: &Path) -> bool {
        false
    }

    fn timed_out(&self, stage: &str) -> Error {
        problem(format!("This desktop's own words: {stage}"))
    }
}

#[tokio::test]
async fn a_fixture_that_never_becomes_ready_is_archived_and_cleaned_up_in_the_desks_words() {
    let kept = Stalled::new(true, true);
    let unkept = Stalled::new(true, false);

    let (ran, unarchived) = tokio::join!(run(&kept), run(&unkept));
    let (ran, unarchived) = (ran.unwrap(), unarchived.unwrap());

    assert_eq!(
        ran.checks,
        [Check {
            kind: "launch".to_owned(),
            passed: false,
            stage: Some("fixture becomes ready".to_owned()),
            error: Some("This desktop's own words: fixture becomes ready".to_owned()),
        }]
    );
    assert_eq!(ran.evidence, [PathBuf::from("/records/native-1")]);
    assert!(ran.archived.is_ok() && ran.cleanup.is_ok());
    assert_eq!(kept.removed.get(), Some(true));
    assert_eq!(kept.keys.get(), 0, "no input before the fixture is ready");
    assert!(unarchived.archived.is_err());
    assert_eq!(
        unkept.removed.get(),
        Some(false),
        "its folder is kept when its evidence was not"
    );
    assert_eq!(
        unarchived.directory,
        PathBuf::from("/private/tmp/atlas-canary.Ab12Cd")
    );
}

#[tokio::test]
async fn a_fixture_that_does_not_launch_is_neither_driven_nor_cleaned_up() {
    let desk = Stalled::new(false, true);

    let refused = run(&desk).await.err().expect("no launch");

    assert_eq!(refused.error, "The fixture could not be launched.");
    assert_eq!(desk.removed.get(), None);
    assert_eq!(desk.keys.get(), 0);
}

#[test]
fn a_record_is_consistent_only_with_one_check_of_each_panel_and_passed_only_if_both_did() {
    let check = |kind: &str, passed: bool| Check {
        kind: kind.to_owned(),
        passed,
        stage: None,
        error: None,
    };
    let both = [check("save", true), check("open", true)];
    let one_failed = [check("save", true), check("open", false)];
    let twice = [check("save", true), check("save", true)];

    assert!(Check::consistent(&both, true));
    assert!(!Check::consistent(&both, false));
    assert!(Check::consistent(&one_failed, false));
    assert!(!Check::consistent(&one_failed, true));
    assert!(!Check::consistent(&twice, true));
    assert!(!Check::consistent(&both[..1], true));
}
