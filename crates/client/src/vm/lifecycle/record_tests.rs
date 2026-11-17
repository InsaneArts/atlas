use super::*;
use crate::vm::lifecycle::tests::{lifecycle, provenance};
use crate::vm::runner::{Install, Operation};

fn record(lifecycle: &Lifecycle) -> Record {
    let reservation = lifecycle.reserve(Some("work"), &[]).unwrap();
    Record::adding(
        &reservation,
        PathBuf::from("/Users/someone/.config/atlas.toml"),
        provenance(),
    )
}

#[test]
fn a_record_is_kept_read_back_and_listed_whole() {
    let (_home, lifecycle) = lifecycle();
    let mut record = record(&lifecycle);
    record.done(Step::Agent);
    record.done(Step::Agent);
    record.failed(&Error::new(
        Code::Transport,
        "The VM's runner did not answer.",
    ));

    lifecycle.keep_record(&record).unwrap();

    let read = lifecycle.record(&record.id).unwrap().unwrap();
    assert_eq!(read, record);
    assert_eq!(read.steps, [Step::Agent]);
    assert_eq!(read.state, State::Adding);
    assert_eq!(lifecycle.records().unwrap(), [record.clone()]);
    lifecycle.forget_record(&record.id).unwrap();
    assert_eq!(lifecycle.record(&record.id).unwrap(), None);
}

#[test]
fn a_change_is_kept_once_with_its_digest_and_enrolment_never_its_secret() {
    let (_home, lifecycle) = lifecycle();
    let mut record = record(&lifecycle);
    let enrolment = Token::new();
    let install = Operation::Install {
        change: Token::new(),
        install: Install::new(
            PathBuf::from("/Applications/Atlas.app"),
            enrolment.clone(),
            "the pairing secret".to_owned(),
        ),
    };

    record.sending(&install).unwrap();
    record.sending(&install).unwrap();
    lifecycle.keep_record(&record).unwrap();

    assert_eq!(record.operations.len(), 1);
    let kept = &record.operations[0];
    assert_eq!(kept.op, "install");
    assert_eq!(kept.enrolment.as_ref(), Some(&enrolment));
    assert_eq!(kept.request, install.digest());
    let text = std::fs::read_to_string(
        lifecycle
            .folder()
            .unwrap()
            .join(format!("{}.json", record.id.as_str())),
    )
    .unwrap();
    assert!(!text.contains("the pairing secret"), "{text}");
}

#[test]
fn another_request_under_a_kept_change_is_refused() {
    let (_home, lifecycle) = lifecycle();
    let mut record = record(&lifecycle);
    let change = Token::new();
    record
        .sending(&Operation::Window {
            change: change.clone(),
            input: true,
        })
        .unwrap();

    let refused = record
        .sending(&Operation::Window {
            change,
            input: false,
        })
        .unwrap_err();

    assert!(
        refused.error.contains("Another request"),
        "{}",
        refused.error
    );
    assert!(record.sending(&Operation::Hello).is_err());
}

#[test]
fn a_removal_is_reconciled_with_its_record_in_the_vms_folder_by_its_change() {
    let (home, lifecycle) = lifecycle();
    let mut record = record(&lifecycle);
    let vms = crate::vm::Vms::under(home.path(), nix::unistd::getuid().as_raw());
    let change = Token::new();
    let marker = vms.removal(&record.id);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let removal = |change: &Token, step: &str| {
        serde_json::json!({
            "change": change, "step": step, "folder": null, "forgot": null, "proved": null,
            "passwords": {"a": "gone"}, "kept": ["/somewhere"],
        })
    };

    assert_eq!(lifecycle.removal(&record).unwrap(), Removal::None);
    record.removal = Some(change.clone());
    assert_eq!(lifecycle.removal(&record).unwrap(), Removal::None);
    std::fs::write(&marker, removal(&change, "deleting").to_string()).unwrap();
    assert_eq!(lifecycle.removal(&record).unwrap(), Removal::UnderWay);
    std::fs::write(&marker, removal(&change, "done").to_string()).unwrap();
    assert_eq!(
        lifecycle.removal(&record).unwrap(),
        Removal::Done {
            kept: vec!["/somewhere".to_owned()],
            passwords: [("a".to_owned(), "gone".to_owned())].into(),
            enrolment: None,
        }
    );
    std::fs::write(&marker, removal(&Token::new(), "done").to_string()).unwrap();
    assert_eq!(lifecycle.removal(&record).unwrap(), Removal::Other);
}

#[test]
fn a_record_outlives_the_vms_station_folder() {
    let (home, lifecycle) = lifecycle();
    let record = record(&lifecycle);
    lifecycle.keep_record(&record).unwrap();
    let station = crate::station::Station::under(home.path());
    let folder = station.folder(&record.id).unwrap();

    std::fs::remove_dir_all(&folder).unwrap();

    assert_eq!(lifecycle.record(&record.id).unwrap(), Some(record));
}

#[test]
fn a_removal_that_deleted_the_passwords_but_left_the_enrolment_says_so() {
    let (home, lifecycle) = lifecycle();
    let mut record = record(&lifecycle);
    let vms = crate::vm::Vms::under(home.path(), nix::unistd::getuid().as_raw());
    let change = Token::new();
    record.removal = Some(change.clone());
    let marker = vms.removal(&record.id);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let done = serde_json::json!({
        "change": change, "step": "done", "folder": null, "forgot": null, "proved": null,
        "passwords": {"a": "deleted", "b": "deleted"}, "kept": [], "enrolment": "left",
    });
    std::fs::write(&marker, done.to_string()).unwrap();

    let removal = lifecycle.removal(&record).unwrap();

    assert_eq!(
        removal,
        Removal::Done {
            kept: Vec::new(),
            passwords: [
                ("a".to_owned(), "deleted".to_owned()),
                ("b".to_owned(), "deleted".to_owned()),
            ]
            .into(),
            enrolment: Some("left".to_owned()),
        }
    );
}
