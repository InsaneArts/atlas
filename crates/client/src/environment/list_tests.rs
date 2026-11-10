use super::*;
use crate::environment::tests::Sandbox;
use atlas_core::files::Lock;
use serde_json::json;
use std::{fs, time::Duration};

#[test]
fn every_record_the_list_comes_across_is_listed_or_counted() {
    let sandbox = Sandbox::new();
    let plan = sandbox.plan(vec![sandbox.payload("file", b"copied")]);
    assert_eq!(sandbox.apply(&plan)["ok"], true);
    let store = state::Store::open(&sandbox.home, Lock::Exclusive).unwrap();
    let before_1970 = std::time::UNIX_EPOCH - Duration::from_secs(24 * 60 * 60);
    fs::File::options()
        .write(true)
        .open(store.folder(&plan.id).join("transaction.json"))
        .unwrap()
        .set_modified(before_1970)
        .unwrap();
    fs::write(store.folder(&atlas_core::files::token()), b"not a folder").unwrap();
    drop(store);

    let listed = sandbox.transactions();

    assert_eq!(listed["unreadable"], 1, "{listed}");
    let listed = listed["transactions"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (&listed[0]["planId"], &listed[0]["updatedAt"]),
        (&json!(plan.id), &json!(0))
    );
}
