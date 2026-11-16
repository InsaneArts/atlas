use super::*;

const UID: u32 = 501;

fn folder() -> Seen {
    Seen {
        there: true,
        folder: true,
        socket: false,
        uid: UID,
        mode: 0o040_700,
    }
}

fn socket() -> Seen {
    Seen {
        there: true,
        folder: false,
        socket: true,
        uid: UID,
        mode: 0o140_600,
    }
}

#[test]
fn the_keeper_launchd_runs_signed_as_atlas_in_its_private_folder_is_trusted() {
    assert_eq!(
        judge_keeper(&folder(), &socket(), Some((90, UID)), Some(90), UID),
        Ok(())
    );
}

#[test]
fn a_listener_that_is_not_the_keeper_launchd_runs_is_refused() {
    for launchd in [Some(91), None] {
        assert_eq!(
            judge_keeper(&folder(), &socket(), Some((90, UID)), launchd, UID),
            Err("it is not the keeper launchd runs"),
            "{launchd:?}"
        );
    }
}

#[test]
fn a_listener_not_signed_as_atlas_or_of_another_account_is_refused() {
    assert_eq!(
        judge_keeper(&folder(), &socket(), None, Some(90), UID),
        Err("it is not signed as Atlas")
    );
    assert_eq!(
        judge_keeper(&folder(), &socket(), Some((90, UID + 1)), Some(90), UID),
        Err("it runs as another account")
    );
}

#[test]
fn a_keeper_outside_a_private_folder_of_this_accounts_or_not_a_socket_is_refused() {
    let shared = Seen {
        mode: 0o040_755,
        ..folder()
    };
    let others = Seen {
        uid: UID + 1,
        ..folder()
    };
    for folder in [shared, others, Seen::default()] {
        assert!(
            judge_keeper(&folder, &socket(), Some((90, UID)), Some(90), UID).is_err(),
            "{folder:?}"
        );
    }
    let file = Seen {
        socket: false,
        ..socket()
    };
    assert!(judge_keeper(&folder(), &file, Some((90, UID)), Some(90), UID).is_err());
}

#[test]
fn a_keeper_hello_of_another_protocol_is_refused_with_advice() {
    let ours = json!({"ok": true, "keeper": {"protocol": crate::vm::keeper::PROTOCOL}});
    let other = json!({"ok": true, "keeper": {"protocol": crate::vm::keeper::PROTOCOL + 1}});

    assert!(spoken(&ours, "work").is_none());
    let refused = spoken(&other, "work").unwrap();
    assert_eq!(refused.code, Code::Update);
    assert!(
        refused.error.starts_with("work's session keeper"),
        "{}",
        refused.error
    );
    assert!(refused.details["next"].is_string());
    assert!(spoken(&json!({"ok": false}), "work").is_some());
}

/// A keeper's trust that refuses whoever listens.
struct Refusing;

impl Trust for Refusing {
    fn trusts(&self, _: &DesktopId, _: &Path, _: &UnixStream) -> Result<()> {
        Err(Error::new(Code::Identity, "not the keeper"))
    }
}

fn selected() -> Arc<Selected> {
    let directory = tempfile::tempdir().unwrap();
    Arc::new(crate::test_support::vm_client(directory.path(), None).selected)
}

#[test]
fn a_keeper_not_trusted_is_told_nothing() {
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let path = folder.path().join("keeper.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let heard = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut said = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stream, &mut said);
        said
    });
    let keeper = Keeper::at(
        selected(),
        DesktopId::parse(crate::test_support::VM_ID).unwrap(),
        path,
        Duration::from_secs(5),
        Arc::new(Refusing),
    );

    let refused = keeper
        .exchange(&json!({"op": "stop"}), Duration::from_secs(5))
        .unwrap_err();
    drop(keeper);

    assert_eq!(refused.code, Code::Identity);
    assert!(heard.join().unwrap().is_empty(), "the request was sent");
}

#[test]
fn a_vm_whose_keeper_is_not_running_is_unreachable_and_says_why() {
    let vm = DesktopId::parse("fedcba9876543210fedcba98").unwrap();

    let link = probe(&selected(), &vm, Duration::from_secs(5));

    let Link::Unreachable(unreachable) = link else {
        panic!("a keeper that is not there was reached");
    };
    assert_eq!(
        unreachable.why.as_deref(),
        Some("work's session keeper is not running")
    );
}

/// How many looks [`counting`] made.
static COUNTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A look at a keeper that takes a while, counted.
fn counting(selected: &Selected, _: &DesktopId, timeout: Duration) -> Link {
    COUNTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    Link::Unreachable(Unreachable {
        selected: Arc::new(selected.clone()),
        timeout,
        why: Some("looked".to_owned()),
    })
}

#[test]
fn polled_reads_at_once_make_one_look_and_reuse_it_while_it_is_fresh() {
    static LOOKED: Shared<Link, Looked> = Shared::new(FRESH);
    let selected = selected();
    let vm = DesktopId::parse("aaaabbbbccccddddeeeeffff").unwrap();

    let reads: Vec<_> = (0..8)
        .map(|_| {
            let (selected, vm) = (selected.clone(), vm.clone());
            std::thread::spawn(move || {
                shared_look(&LOOKED, &selected, &vm, Duration::from_secs(5), counting)
            })
        })
        .collect();
    for read in reads {
        assert!(matches!(read.join().unwrap(), Link::Unreachable(_)));
    }
    let again = shared_look(&LOOKED, &selected, &vm, Duration::from_secs(7), counting);

    assert_eq!(COUNTED.load(std::sync::atomic::Ordering::SeqCst), 1);
    let Link::Unreachable(again) = again else {
        panic!("a look made a keeper");
    };
    assert_eq!(again.why.as_deref(), Some("looked"));
    assert_eq!(
        again.timeout,
        Duration::from_secs(7),
        "each caller's own deadline"
    );
    LOOKED.join_workers();
}
