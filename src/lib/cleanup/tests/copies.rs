//! Collected copies: a file is removed only while a copy of its newest
//! version is still as it was collected.
use super::*;

fn copy_of(source: &str, in_home: &str) -> crate::collect::Kept {
    crate::collect::Kept {
        source: source.into(),
        in_home: Some(in_home.into()),
        path: PathBuf::from("/nowhere/copy"),
        size: 3,
        sha256: "00".into(),
        read: None,
    }
}

#[tokio::test]
async fn a_copy_that_is_missing_blocks_removing_its_source_and_nothing_else() {
    let place = Place::new();
    let key = session("not-kept");
    let task = default_task(&key);
    let touching = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    // The fake's plans copy .config/probe/settings.toml into the helper.
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            missing: vec![copy_of(
                "/Users/atlas/.config/probe/settings.toml",
                ".config/probe/settings.toml",
            )],
            ..Default::default()
        },
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(kinds(&reply), ["check-outputs", "undo-plan"]);
    assert_eq!(reply["steps"][0]["state"], "failed");
    assert_eq!(reply["steps"][0]["reason"], "output-not-kept");
    assert_eq!(
        reply["steps"][0]["notKept"][0]["source"],
        "/Users/atlas/.config/probe/settings.toml"
    );
    assert_eq!(
        (&reply["steps"][1]["state"], &reply["steps"][1]["reason"]),
        (&json!("blocked"), &json!("output-not-kept"))
    );
    assert!(
        !fake.logged().contains(&format!("undo {touching}")),
        "the source was kept"
    );
    assert!(reply["error"].as_str().unwrap().contains("atlas collect"));

    // Collected again, the file is kept by its newer copy.
    let again = collected(&place, &task);
    fake.intact.borrow_mut().insert(
        again.to_string(),
        crate::collect::Intact {
            kept: vec![copy_of(
                "/Users/atlas/.config/probe/settings.toml",
                ".config/probe/settings.toml",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("undo {touching}")));
}

#[tokio::test]
async fn a_copy_of_a_file_no_step_removes_lets_every_step_run() {
    let place = Place::new();
    let key = session("elsewhere");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            changed: vec![copy_of(
                "/Users/atlas/atlas-work/report.pdf",
                "atlas-work/report.pdf",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(states(&reply), ["failed", "done"], "{reply}");
    assert!(fake.logged().contains(&format!("undo {plan}")));
    assert_eq!(reply["state"], "incomplete");
}

#[tokio::test]
async fn a_receipt_that_is_gone_keeps_every_source_and_job_stops_go_on() {
    let place = Place::new();
    let key = session("receipt-gone");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            unknown: true,
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(kinds(&reply), ["check-outputs", "stop-job", "undo-plan"]);
    assert_eq!(states(&reply), ["failed", "done", "blocked"], "{reply}");
    assert_eq!(reply["steps"][2]["reason"], "output-not-kept");
    assert!(
        reply["steps"][0]["error"]
            .as_str()
            .unwrap()
            .contains("receipt")
    );
    assert!(fake.logged().contains(&format!("stop job {job}")));
    assert!(
        !fake.logged().contains(&format!("undo {plan}")),
        "nothing that may be the only copy's source is rolled back"
    );
}

/// Note in `task`'s record a collection made at `at` milliseconds.
fn collected_at(place: &Place, task: &Task, at: u64) -> crate::collect::ReceiptId {
    let receipt = crate::collect::ReceiptId::parse(&format!("{at}-a1b2c3")).unwrap();
    Tasks::at(&place.output)
        .update(task, 1, |record| {
            record.noted_output(crate::task::record::OutputEntry {
                receipt: receipt.clone(),
                at,
                files: 1,
                bytes: 3,
                later: serde_json::Map::new(),
            });
            Ok(())
        })
        .unwrap();
    receipt
}

/// One version of the probe's settings file, read from `started` to
/// `finished` and kept with `sha256`.
fn version(sha256: &str, read: Option<(u64, u64)>) -> crate::collect::Kept {
    crate::collect::Kept {
        sha256: sha256.into(),
        read: read.map(|(started, finished)| crate::collect::ReadSpan { started, finished }),
        ..copy_of(
            "/Users/atlas/.config/probe/settings.toml",
            ".config/probe/settings.toml",
        )
    }
}

/// Clean up a task that applied a plan writing the probe's settings and
/// made two collections of them: `first` at 1000 and `second` at 2000,
/// each copy `kept` or lost. Whether the plan was undone, and the reply.
async fn two_versions(
    first: crate::collect::Kept,
    first_kept: bool,
    second: crate::collect::Kept,
    second_kept: bool,
) -> (bool, Value) {
    let place = Place::new();
    let key = session("versions");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let fake = Fake::at(&place.root);
    for (at, file, kept) in [(1_000, first, first_kept), (2_000, second, second_kept)] {
        let receipt = collected_at(&place, &task, at);
        let intact = if kept {
            crate::collect::Intact {
                kept: vec![file],
                ..Default::default()
            }
        } else {
            crate::collect::Intact {
                missing: vec![file],
                ..Default::default()
            }
        };
        fake.intact.borrow_mut().insert(receipt.to_string(), intact);
    }
    let reply = place.apply(&fake, &own(&key)).await;
    (fake.logged().contains(&format!("undo {plan}")), reply)
}

#[tokio::test]
async fn the_version_read_after_every_other_is_the_one_that_counts() {
    // Read one after the other: the later bytes count.
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        true,
        version("bb", Some((2_000, 2_100))),
        false,
    )
    .await;
    assert!(
        !undone,
        "an older version never stands in for the newest: {reply}"
    );
    assert_eq!(states(&reply), ["failed", "blocked"]);

    // The older copy lost, the newest kept: the file is kept.
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        false,
        version("bb", Some((2_000, 2_100))),
        true,
    )
    .await;
    assert!(undone, "{reply}");
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn the_same_bytes_kept_in_any_collection_keep_the_file() {
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 1_100))),
        true,
        version("aa", Some((2_000, 2_100))),
        false,
    )
    .await;
    assert!(undone, "{reply}");
    assert_eq!(reply["ok"], true);
}

#[tokio::test]
async fn versions_read_while_each_other_was_read_are_all_kept() {
    // The first collection began earlier, but read the file while the
    // second did: either may hold its latest state.
    for (lost_first, lost_second) in [(true, false), (false, true)] {
        let (undone, reply) = two_versions(
            version("aa", Some((1_000, 2_500))),
            !lost_first,
            version("bb", Some((2_000, 2_100))),
            !lost_second,
        )
        .await;
        assert!(!undone, "{reply}");
        assert_eq!(states(&reply), ["failed", "blocked"], "{reply}");
    }
    let (undone, reply) = two_versions(
        version("aa", Some((1_000, 2_500))),
        true,
        version("bb", Some((2_000, 2_100))),
        true,
    )
    .await;
    assert!(undone, "both versions are kept: {reply}");
}

#[tokio::test]
async fn a_clock_that_moved_backwards_orders_nothing() {
    // Read "later" by a clock that went back: neither version is known to
    // be the newest, so losing either keeps the file.
    for second in [
        version("bb", Some((2_100, 2_000))),
        version("bb", Some((500, 600))),
        version("bb", None),
    ] {
        let (undone, reply) =
            two_versions(version("aa", Some((1_000, 1_100))), false, second, true).await;
        assert!(!undone, "{reply}");
        assert_eq!(states(&reply), ["failed", "blocked"], "{reply}");
    }
}

#[tokio::test]
async fn a_source_named_in_another_case_is_kept_all_the_same() {
    let place = Place::new();
    let key = session("case");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.intact.borrow_mut().insert(
        receipt.to_string(),
        crate::collect::Intact {
            missing: vec![copy_of(
                "/Users/atlas/.Config/Probe/settings.toml",
                ".Config/Probe/settings.toml",
            )],
            ..Default::default()
        },
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["steps"][1]["state"], "blocked", "{reply}");
    assert!(!fake.logged().contains(&format!("undo {plan}")));
}
