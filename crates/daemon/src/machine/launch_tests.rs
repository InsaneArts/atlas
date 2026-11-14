use super::*;

const PRINTED: &str = "gui/502/com.insanearts.atlas.machine = {
\tactive count = 1
\tpath = /Library/LaunchAgents/com.insanearts.atlas.machine.plist
\ttype = LaunchAgent
\tstate = running
\tprogram = /Applications/Atlas.app/Contents/MacOS/atlas
\tpid = 4242
\tendpoints = {
\t\tpid = 1
\t\tpath = /tmp/elsewhere.plist
\t}
}
";

#[test]
fn a_jobs_own_pid_and_path_are_read_never_a_nested_ones() {
    let job = Job::parse(PRINTED);

    assert_eq!(job.pid, Some(4242));
    assert_eq!(job.path.as_deref(), Some(PLIST));
}

#[test]
fn a_job_that_is_not_running_names_no_process() {
    let job = Job::parse("x = {\n\tpath = /Library/LaunchAgents/a.plist\n\tstate = waiting\n}\n");

    assert_eq!(job.pid, None);
}

/// A plist running `program` as the machine daemon.
fn plist(program: &str) -> Plist {
    Plist {
        label: LABEL.to_owned(),
        program_arguments: vec![program.into(), "daemon".into(), "--machine".into()],
    }
}

/// Root-owned files that stand in for the installed plist and program.
const INSTALLED: &str = "/usr/share/dict/web2";
const PROGRAM: &str = "/usr/bin/true";

fn job(pid: u32, path: &str) -> Job {
    Job {
        pid: Some(pid),
        path: Some(path.to_owned()),
    }
}

#[test]
fn the_genuine_job_running_this_installed_program_is_proved() {
    let at = Path::new(INSTALLED);

    judge(
        &job(7, INSTALLED),
        7,
        (at, &plist(PROGRAM)),
        Path::new(PROGRAM),
    )
    .unwrap();
}

#[test]
fn a_job_an_agent_could_make_is_refused() {
    let at = Path::new(INSTALLED);
    let program = Path::new(PROGRAM);
    let ours = plist(PROGRAM);
    let mut renamed = plist(PROGRAM);
    renamed.label = "com.example.agent".into();
    let mut other_command = plist(PROGRAM);
    other_command.program_arguments[2] = "--serve".into();

    for (job, installed) in [
        // Loaded from a plist of the agent's own.
        (job(7, "/Users/guest/Library/LaunchAgents/x.plist"), &ours),
        // Another process as the job.
        (job(8, INSTALLED), &ours),
        (job(7, INSTALLED), &renamed),
        (job(7, INSTALLED), &other_command),
        (job(7, INSTALLED), &plist("/bin/ls")),
    ] {
        let refused = judge(&job, 7, (at, installed), program).unwrap_err();
        assert_eq!(refused.code, Code::Identity);
    }
}

#[test]
fn a_plist_or_program_the_account_could_change_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let mine = folder.path().join("job.plist");
    std::fs::write(&mine, "").unwrap();
    let mine_text = mine.to_str().unwrap();

    let plist_refused = judge(
        &job(7, mine_text),
        7,
        (&mine, &plist(PROGRAM)),
        Path::new(PROGRAM),
    );
    let program_refused = judge(
        &job(7, INSTALLED),
        7,
        (Path::new(INSTALLED), &plist(mine_text)),
        &mine,
    );

    assert!(plist_refused.is_err());
    assert!(program_refused.is_err());
}

#[test]
fn a_field_a_jobs_own_arguments_smuggled_in_names_nothing() {
    let smuggled = format!(
        "gui/502/{LABEL} = {{\n\tpath = /Users/guest/Library/LaunchAgents/x.plist\n\targuments = {{\n\t\t/Applications/Atlas.app/Contents/MacOS/atlas\n\t\tdaemon\n\t\t--machine\n\tpath = {PLIST}\n\tpid = 1\n\t}}\n\tpid = 4242\n}}\n"
    );

    let job = Job::parse(&smuggled);

    assert_eq!(job.path, None);
    assert_eq!(job.pid, None);
}

#[test]
fn a_file_its_access_list_lets_anyone_change_is_refused_whatever_its_mode() {
    let installed = Owned {
        uid: 0,
        gid: 0,
        mode: 0o100_644,
        symlink: false,
        listed: false,
    };
    assert!(installed.by_root());

    let listed = Owned {
        listed: true,
        ..installed
    };
    assert!(!listed.by_root());
}

#[test]
fn a_file_another_group_or_anyone_may_write_is_refused() {
    let installed = Owned {
        uid: 0,
        gid: 80,
        mode: 0o040_775,
        symlink: false,
        listed: false,
    };
    assert!(installed.by_root(), "administrators may write it");

    for changed in [
        Owned {
            gid: 20,
            ..installed
        },
        Owned {
            mode: 0o040_757,
            ..installed
        },
        Owned {
            uid: 502,
            ..installed
        },
        Owned {
            symlink: true,
            ..installed
        },
    ] {
        assert!(!changed.by_root(), "{changed:?}");
    }
}
