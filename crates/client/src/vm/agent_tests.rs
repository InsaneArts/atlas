use super::*;
use crate::vm::tests::{Fake, desktop, home};

fn loaded(vms: &Vms, program: &Path, arguments: &[&str]) -> Loaded {
    Loaded {
        pid: Some(77),
        path: Some(vms.agent_plist(&desktop())),
        program: Some(program.to_string_lossy().into_owned()),
        arguments: Some(
            arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
        ),
        properties: Some(vec!["keepalive".to_owned()]),
        exit_timeout: Some(EXIT_TIMEOUT),
    }
}

#[test]
fn the_agents_plist_is_one_launchd_reads_as_written() {
    let (_home, vms) = home();
    let fake = Fake::new();
    let plist = vms.agent_plist(&desktop());
    std::fs::create_dir_all(vms.agents()).unwrap();
    std::fs::write(
        &plist,
        plist_of(
            &Vms::label(&desktop()),
            &fake.program().unwrap(),
            &desktop(),
        ),
    )
    .unwrap();

    let json = atlas_core::spawn::output(
        std::process::Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&plist),
    )
    .unwrap();
    let read: Value = serde_json::from_slice(&json.stdout).unwrap();

    assert_eq!(read["Label"], Vms::label(&desktop()));
    assert_eq!(read["ProgramArguments"][1], "serve");
    assert_eq!(read["ProgramArguments"][2], desktop().as_str());
    assert_eq!(read["KeepAlive"], true);
    assert_eq!(read["ExitTimeOut"], EXIT_TIMEOUT);
}

#[test]
fn an_agent_not_loaded_is_bootstrapped_and_judged_by_what_launchd_loaded() {
    let (_home, vms) = home();
    let fake = Fake::new();

    let first = install_with(&vms, &fake, &desktop()).unwrap();
    let program = fake.program().unwrap();
    let shown = program.to_string_lossy().into_owned();
    *fake.job.lock().unwrap() = Some(loaded(
        &vms,
        &program,
        &[&shown, "serve", desktop().as_str()],
    ));
    let second = install_with(&vms, &fake, &desktop()).unwrap();

    assert_eq!(first["agent"], "pending", "{first}");
    assert!(fake.launched.lock().unwrap()[0].starts_with("bootstrap gui/"));
    assert_eq!(second["agent"], "loaded", "{second}");
}

#[test]
fn a_job_loaded_otherwise_is_never_reloaded() {
    let (_home, vms) = home();
    let fake = Fake::new();
    let program = fake.program().unwrap();
    let shown = program.to_string_lossy().into_owned();
    let mut other = loaded(&vms, &program, &[&shown, "serve", desktop().as_str()]);
    other.properties = Some(Vec::new());
    *fake.job.lock().unwrap() = Some(other);

    let installed = install_with(&vms, &fake, &desktop()).unwrap();

    assert_eq!(installed["agent"], "pending", "{installed}");
    assert!(fake.launched.lock().unwrap().is_empty());
    let mut timed = loaded(&vms, &program, &[&shown, "serve", desktop().as_str()]);
    timed.exit_timeout = Some(0);
    assert_eq!(
        mismatch(&timed, &vms.agent_plist(&desktop()), &program, &desktop()),
        Some("another exit timeout")
    );
    let argued = loaded(
        &vms,
        &program,
        &[&shown, "serve", "a7a5000000000000000000f2"],
    );
    assert_eq!(
        mismatch(&argued, &vms.agent_plist(&desktop()), &program, &desktop()),
        Some("other arguments")
    );
}

#[test]
fn no_agent_is_installed_while_its_vm_is_being_removed() {
    let (_home, vms) = home();
    let fake = Fake::new();
    std::fs::create_dir_all(vms.removals()).unwrap();
    std::fs::write(vms.removal(&desktop()), "{}").unwrap();

    let installed = install_with(&vms, &fake, &desktop()).unwrap();

    assert_eq!(installed["ok"], false, "{installed}");
    assert!(!vms.agent_plist(&desktop()).exists());
}

#[test]
fn the_held_form_works_under_the_callers_lock_and_never_takes_it_again() {
    let (_home, vms) = home();
    let fake = Fake::new();
    let admitted = crate::vm::lifecycle::Lifecycle::under(&vms.home)
        .admit()
        .unwrap();
    let hold = crate::vm::hold_in(&vms, &desktop(), &admitted)
        .unwrap()
        .unwrap();

    let wrapped = install_with(&vms, &fake, &desktop()).unwrap();
    let held = installed(&vms, &fake, &hold).unwrap();

    assert_eq!(wrapped["why"], "Another station operation holds this VM.");
    assert_ne!(held["why"], "Another station operation holds this VM.");
    assert!(fake.launched.lock().unwrap()[0].starts_with("bootstrap"));
}

#[test]
fn the_wrapper_is_refused_by_an_uninstall_before_it_takes_the_vms_lock() {
    let (_home, vms) = home();
    let fake = Fake::new();
    let lifecycle = crate::vm::lifecycle::Lifecycle::under(&vms.home);
    let uninstalling = lifecycle
        .uninstalling(&atlas_core::files::Token::new())
        .unwrap();

    let refused = install_with(&vms, &fake, &desktop()).unwrap_err();

    assert!(
        refused.error.contains("being uninstalled"),
        "{}",
        refused.error
    );
    assert!(fake.launched.lock().unwrap().is_empty());
    let hold = crate::vm::hold_in(&vms, &desktop(), &uninstalling).unwrap();
    assert!(hold.is_some(), "the VM's lock was never taken");
}
