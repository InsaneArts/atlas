use super::*;
use crate::vm::tests::{Fake, desktop, home};
use std::path::PathBuf;

fn program() -> PathBuf {
    PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas")
}

fn settings() -> PathBuf {
    PathBuf::from("/Users/person/.config/atlas/config.toml")
}

fn plist(vms: &Vms) -> PathBuf {
    vms.agents()
        .join(format!("{}.plist", Vms::keeper_label(&desktop())))
}

/// The job as launchd loads the plist written for the keeper.
fn loaded(vms: &Vms) -> Loaded {
    Loaded {
        pid: Some(91),
        path: Some(plist(vms)),
        program: Some(program().to_string_lossy().into_owned()),
        arguments: Some(arguments(&program(), &desktop(), &settings())),
        properties: Some(vec!["keepalive".to_owned()]),
        exit_timeout: Some(EXIT_TIMEOUT),
    }
}

#[test]
fn the_keepers_plist_is_one_launchd_reads_as_written() {
    let (_home, vms) = home();
    let path = plist(&vms);
    std::fs::create_dir_all(vms.agents()).unwrap();
    let written = arguments(&program(), &desktop(), &settings());
    std::fs::write(&path, plist_of(&Vms::keeper_label(&desktop()), &written)).unwrap();

    let json = atlas_core::spawn::output(
        std::process::Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&path),
    )
    .unwrap();
    let read: Value = serde_json::from_slice(&json.stdout).unwrap();

    assert_eq!(
        read["Label"],
        format!("com.insanearts.atlas.station.{}", desktop().as_str())
    );
    assert_eq!(read["ProgramArguments"], json!(written));
    assert_eq!(read["ProgramArguments"][1], "station");
    assert_eq!(read["ProgramArguments"][2], "keep");
    assert_eq!(
        read["ProgramArguments"][4],
        format!("vm-{}", desktop().as_str())
    );
    assert_eq!(read["KeepAlive"], true);
    assert_eq!(read["ExitTimeOut"], EXIT_TIMEOUT);
    assert_eq!(read["LimitLoadToSessionType"], "Aqua");
    assert_eq!(read["ProcessType"], "Interactive");
}

#[test]
fn a_keeper_not_loaded_is_bootstrapped_and_judged_by_what_launchd_loaded() {
    let (_home, vms) = home();
    let fake = Fake::new();

    let first = install_with(&vms, &fake, &desktop(), &program(), &settings()).unwrap();
    *fake.keeper_job.lock().unwrap() = Some(loaded(&vms));
    let second = install_with(&vms, &fake, &desktop(), &program(), &settings()).unwrap();

    assert_eq!(first["keeper"], "pending", "{first}");
    assert_eq!(
        fake.launched.lock().unwrap().as_slice(),
        [format!(
            "bootstrap gui/{} {}",
            nix::unistd::getuid(),
            plist(&vms).display()
        )]
    );
    assert_eq!(second["keeper"], "loaded", "{second}");
    assert_eq!(second["pid"], 91);
}

#[test]
fn a_keeper_loaded_otherwise_is_named_and_never_reloaded() {
    let (_home, vms) = home();
    let fake = Fake::new();
    let mut cases = Vec::new();
    let mut other = loaded(&vms);
    other.path = Some(PathBuf::from("/tmp/other.plist"));
    cases.push((other, "another plist"));
    let mut other = loaded(&vms);
    other.program = Some("/tmp/atlas".to_owned());
    cases.push((other, "another program"));
    let mut other = loaded(&vms);
    other.arguments = Some(arguments(&program(), &desktop(), Path::new("/tmp/x.toml")));
    cases.push((other, "other arguments"));
    let mut other = loaded(&vms);
    other.properties = Some(Vec::new());
    cases.push((other, "not kept alive"));
    let mut other = loaded(&vms);
    other.exit_timeout = Some(5);
    cases.push((other, "another exit timeout"));

    for (job, why) in cases {
        *fake.keeper_job.lock().unwrap() = Some(job);

        let said = install_with(&vms, &fake, &desktop(), &program(), &settings()).unwrap();

        assert_eq!(said["keeper"], "pending", "{said}");
        assert!(said["why"].as_str().unwrap().contains(why), "{said}");
    }
    assert!(
        fake.launched.lock().unwrap().is_empty(),
        "launchd was asked"
    );
}

#[test]
fn no_keeper_is_installed_while_its_vm_is_being_removed() {
    let (_home, vms) = home();
    let fake = Fake::new();
    std::fs::create_dir_all(vms.removals()).unwrap();
    std::fs::write(vms.removal(&desktop()), b"{}").unwrap();

    let said = install_with(&vms, &fake, &desktop(), &program(), &settings()).unwrap();

    assert_eq!(said["ok"], false, "{said}");
    assert!(!plist(&vms).exists());
    assert!(fake.launched.lock().unwrap().is_empty());
}
