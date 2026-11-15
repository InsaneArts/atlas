use super::*;
use crate::test_support::{own_session, synthetic};
use atlas_core::{files::Token, session::Console};
use atlas_session::{
    clock::Manual,
    generation::Counted,
    standin::{
        Controlling,
        link::{Conditions, Link},
        station_control,
    },
    station::{Desktop, Dial, Dialing, Peers},
};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};

/// A guest's change records.
type Kept = Changes<Arc<dyn Vault>>;

/// A guest's records' folder, the vault vouching for them, and its change
/// records.
type Records = (tempfile::TempDir, Arc<dyn Vault>, Arc<Kept>);

/// A guest's change records as pairing leaves them, in a folder of their
/// own, and the vault vouching for them.
fn records() -> Records {
    let folder = tempfile::tempdir().unwrap();
    let vault: Arc<dyn Vault> = Arc::new(crate::machine::vault::Memory::default());
    crate::machine::commitment::create(&vault, crate::machine::changes::STORE).unwrap();
    let changes = Arc::new(Changes::new(folder.path(), vault.clone()));
    (folder, vault, changes)
}

/// A guest's Stop as pairing leaves it, beside its change records.
fn stopping(records: &tempfile::TempDir, vault: &Arc<dyn Vault>) -> Arc<Stop<Arc<dyn Vault>>> {
    crate::machine::commitment::create(vault, crate::machine::stop::STORE).unwrap();
    Arc::new(Stop::new(records.path().join("stop"), vault.clone()))
}

/// What a guest's commands run under, its supervisors kept in a folder of
/// their own beside its change records, as pairing leaves them.
fn supervising(records: &tempfile::TempDir, vault: &Arc<dyn Vault>) -> Supervising {
    crate::machine::commitment::create(vault, crate::machine::processes::STORE).unwrap();
    let folder = records.path().join("supervisors");
    let proofs = folder.join("proofs");
    std::fs::create_dir_all(&proofs).unwrap();
    Supervising {
        supervisor: crate::machine::supervise::invocation::Supervisor::of_these_tests(),
        proofs,
        kept: Arc::new(crate::machine::processes::Processes::new(
            folder.join("processes.json"),
            vault.clone(),
        )),
    }
}

/// Connections the station makes, as the guest's listener hands them over.
struct Channel(Mutex<mpsc::UnboundedReceiver<(Purpose, Raw)>>);

impl Listener for Channel {
    fn accept(&self) -> Accepting<'_> {
        Box::pin(async {
            self.0
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| io::ErrorKind::BrokenPipe.into())
        })
    }
}

/// The station dialling lanes across the link, each handed to the guest.
struct Lanes(Arc<Link>, mpsc::UnboundedSender<(Purpose, Raw)>);

impl Dial for Lanes {
    fn dial(&self) -> Dialing<'_> {
        let (station, guest, _cut) = self.0.connect();
        let _ = self.1.send((Purpose::Lane, Box::new(guest)));
        Box::pin(async move { Ok(Box::new(station) as Raw) })
    }
}

/// A paired guest serving across a healthy link, and the station's end.
struct Served {
    machine: Arc<Machine>,
    clock: Arc<Manual>,
    link: Arc<Link>,
    station: Arc<Keypair>,
    guest_key: PeerKey,
    desktop: DesktopId,
    connections: mpsc::UnboundedSender<(Purpose, Raw)>,
    changes: Arc<Kept>,
    vault: Arc<dyn Vault>,
    records: tempfile::TempDir,
    _serving: tokio::task::JoinHandle<Result<()>>,
}

impl Served {
    async fn start() -> Self {
        Self::on(records()).await
    }

    /// A guest serving with `records`, as a daemon that started finds them.
    async fn on(records: Records) -> Self {
        Self::with_input(records, None).await
    }

    /// A guest serving with `records`, its input service `input` if any.
    async fn with_input(records: Records, input: Option<crate::machine::input::Input>) -> Self {
        let (records, vault, changes) = records;
        let mut daemon = Daemon::new("Aqua".into(), synthetic, || true, own_session);
        daemon.screen.console = Console::Own;
        let (station, guest) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
        let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();
        let guest_key = guest.public();
        let clock = Arc::new(Manual::default());
        let paired = Paired {
            keys: guest,
            desktop: desktop.clone(),
            station: station.public(),
            generations: Arc::new(Counted::default()),
            changes: changes.clone(),
            stop: stopping(&records, &vault),
        };
        let machine = Machine::new(
            daemon,
            paired,
            clock.clone(),
            supervising(&records, &vault),
            input,
        );
        let (connections, accepted) = mpsc::unbounded_channel();
        let serving = tokio::spawn({
            let machine = machine.clone();
            async move { machine.serve(&Channel(Mutex::new(accepted))).await }
        });
        Self {
            machine,
            clock,
            link: Arc::new(Link::new(Conditions::default())),
            station: Arc::new(station),
            guest_key,
            desktop,
            connections,
            changes,
            vault,
            records,
            _serving: serving,
        }
    }

    /// The station's control connection to the guest, in a new generation,
    /// and the station's way to its lanes in it.
    async fn control(&self) -> (Controlling, Desktop) {
        let (station_end, guest_end, _cut) = self.link.connect();
        self.connections
            .send((Purpose::Control, Box::new(guest_end)))
            .unwrap();
        let controlling =
            station_control(station_end, &self.station, self.guest_key, &self.desktop)
                .await
                .unwrap();
        let peers = Peers {
            desktop: self.desktop.clone(),
            own: self.station.clone(),
            key: self.guest_key,
        };
        let desktop = Desktop::new(
            peers,
            controlling.generation,
            controlling.outbox.clone(),
            Box::new(Lanes(self.link.clone(), self.connections.clone())),
        );
        (controlling, desktop)
    }

    fn retired(&self) -> Vec<(Generation, Retired)> {
        self.machine.said.retired.lock().unwrap().clone()
    }
}

/// Send `request` on a lane of `kind`, and read its lines until the lane
/// ends: none when it was refused before any reply.
async fn ask(desktop: &Desktop, kind: LaneKind, request: &Value) -> Vec<Value> {
    let opened = desktop.open(kind).await.unwrap();
    let mut stream = opened.stream;
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    if stream.write_all(&line).await.is_err() || stream.flush().await.is_err() {
        return Vec::new();
    }
    let mut lines = BufReader::new(stream).lines();
    let mut replies = Vec::new();
    while let Ok(Ok(Some(line))) =
        tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await
    {
        replies.push(serde_json::from_str(&line).unwrap());
    }
    replies
}

#[tokio::test]
async fn a_read_on_a_lane_of_the_live_generation_is_answered() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let replies = ask(&desktop, LaneKind::Read, &json!({"op": "hello"})).await;

    assert_eq!(replies.last().unwrap()["ok"], true, "{replies:?}");
}

#[tokio::test]
async fn a_request_that_is_not_what_its_lane_was_opened_for_is_refused_before_it_runs() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let marker = tempfile::tempdir().unwrap();
    let touched = marker.path().join("ran");
    let run = |change: &Token| json!({"op": "run", "argv": ["/usr/bin/touch", touched], "change": change});
    let (change, other) = (Token::new(), Token::new());

    for (kind, request) in [
        (LaneKind::Read, run(&change)),
        (LaneKind::Change { change: other }, run(&change)),
        (LaneKind::Read, json!({"op": "screenshot"})),
        (
            LaneKind::Read,
            json!({"op": "read", "path": "notes", "offset": 0, "length": 1,
                "maxSize": 1, "session": {"securitySessionId": 42, "displayId": 7}}),
        ),
        (LaneKind::Picture, json!({"op": "hello"})),
        (
            LaneKind::Picture,
            json!({"op": "frame", "maxDimension": 320,
                "session": {"securitySessionId": 42, "displayId": 7}}),
        ),
        (
            LaneKind::Change {
                change: change.clone(),
            },
            json!({"op": "hello"}),
        ),
    ] {
        let replies = ask(&desktop, kind, &request).await;
        assert_eq!(replies.len(), 1, "{request}");
        assert_eq!(replies[0]["code"], "bad-request", "{request}");
    }
    assert!(!touched.exists(), "nothing ran");
}

#[tokio::test]
async fn a_change_on_the_lane_of_that_change_runs() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();
    let request = json!({"op": "run", "argv": ["/bin/echo", "hi"], "change": change});

    let replies = ask(&desktop, LaneKind::Change { change }, &request).await;

    assert_eq!(replies.last().unwrap()["ok"], true, "{replies:?}");
}

#[tokio::test]
async fn a_newer_control_connection_retires_the_generation_before_it_and_refuses_its_lanes() {
    let served = Served::start().await;
    let (_first, old) = served.control().await;
    let (_second, new) = served.control().await;

    assert_eq!(served.retired(), [(Generation(1), Retired::Replaced)]);
    assert!(
        ask(&old, LaneKind::Read, &json!({"op": "hello"}))
            .await
            .is_empty()
    );
    let replies = ask(&new, LaneKind::Read, &json!({"op": "hello"})).await;
    assert_eq!(replies.last().unwrap()["ok"], true);
}

#[tokio::test]
async fn a_cancel_over_control_ends_a_running_command_with_its_lane_still_open() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let change = Token::new();
    let opened = desktop
        .open(LaneKind::Change {
            change: change.clone(),
        })
        .await
        .unwrap();
    let mut stream = opened.stream;
    let request = json!({"op": "run", "argv": ["/bin/sleep", "30"], "timeoutMs": 60000,
        "change": change});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stream.flush().await.unwrap();
    let mut lines = BufReader::new(stream).lines();
    let started: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);

    controlling
        .outbox
        .say(atlas_session::control::Said::Cancel { lane: opened.lane })
        .unwrap();

    tokio::time::timeout(Duration::from_secs(10), async {
        while nix::sys::signal::killpg(group, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the command was ended");
}

#[tokio::test]
async fn the_watchdog_retires_a_generation_whose_lease_lapsed_with_nothing_else_happening() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    // A silent partition: the connection stays open, nothing crosses, and
    // nothing is asked.
    served.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    served.clock.advance(atlas_session::authority::LEASE * 2);

    tokio::time::timeout(Duration::from_secs(5), async {
        while served.retired().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the generation retired");
    assert_eq!(served.retired(), [(Generation(1), Retired::Lapsed)]);
    served.link.set(Conditions::default());
    assert!(
        ask(&desktop, LaneKind::Read, &json!({"op": "hello"}))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn a_change_whose_lane_was_cancelled_after_admission_never_runs() {
    let served = Served::start().await;
    let (_controlling, _desktop) = served.control().await;
    let (authority, lanes) = {
        let live = served.machine.live.lock().await;
        let live = live.as_ref().unwrap();
        (live.authority.clone(), live.lanes.clone())
    };
    let asker = Asker {
        controller: true,
        host: None,
        gate: Some(Gate {
            authority,
            lanes: lanes.clone(),
            lane: LaneId(9),
            halted: tokio::sync::watch::Sender::new(super::super::Halt::default()).subscribe(),
            stops: 0,
        }),
    };
    assert!(asker.refused_now().is_none());

    lanes.cancel(LaneId(9));

    assert_eq!(asker.refused_now().unwrap()["code"], "cancelled");
}

#[tokio::test]
async fn a_machine_desktop_refuses_every_change_but_a_command_for_now() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();

    let replies = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "sound", "muted": true, "change": change}),
    )
    .await;

    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["code"], "platform", "{replies:?}");
}

#[tokio::test]
async fn a_lapsed_generation_stays_silent_once_its_partition_heals() {
    let served = Served::start().await;
    let (controlling, _desktop) = served.control().await;
    served.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    served.clock.advance(atlas_session::authority::LEASE * 2);
    tokio::time::timeout(Duration::from_secs(5), async {
        while served.retired().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    served.link.set(Conditions::default());

    // The guest closed its side: the station's reading ends, never heard
    // from again in that generation.
    tokio::time::timeout(Duration::from_secs(5), controlling.reading)
        .await
        .expect("control was closed")
        .ok();
    assert_eq!(
        controlling
            .heard
            .again
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn lanes_over_the_bound_are_closed_before_any_handshake() {
    let served = Served::start().await;
    // Lanes that never begin their handshake fill the room.
    let mut held = Vec::new();
    for _ in 0..LANES {
        let (station, guest, cut) = served.link.connect();
        served
            .connections
            .send((Purpose::Lane, Box::new(guest)))
            .unwrap();
        held.push((station, cut));
    }
    let (mut over, guest, _cut) = served.link.connect();
    served
        .connections
        .send((Purpose::Lane, Box::new(guest)))
        .unwrap();

    let mut byte = [0; 1];
    let read = tokio::time::timeout(Duration::from_secs(5), over.read(&mut byte))
        .await
        .expect("closed at once");
    assert_eq!(read.unwrap(), 0);
}

/// Set in the guest process the test below starts: its socket and its
/// station's key. The guest makes its own key and says only its public
/// half, beside the socket, once it listens there.
const GUEST: &str = "ATLAS_TEST_MACHINE_GUEST";

/// Connections on a Unix socket, each naming what it is for in its first
/// byte, as a stand-in for the transport a machine's kind gives.
struct Socket(tokio::net::UnixListener);

impl Listener for Socket {
    fn accept(&self) -> Accepting<'_> {
        Box::pin(async {
            let (mut stream, _) = self.0.accept().await?;
            let purpose = match stream.read_u8().await? {
                0 => Purpose::Control,
                1 => Purpose::Lane,
                _ => Purpose::Pairing,
            };
            Ok((purpose, Box::new(stream) as Raw))
        })
    }
}

/// Reach the guest's socket at `path` for `purpose`.
async fn reach(path: &std::path::Path, purpose: u8) -> tokio::net::UnixStream {
    let mut stream = tokio::net::UnixStream::connect(path).await.unwrap();
    stream.write_all(&[purpose]).await.unwrap();
    stream
}

/// The station dialling lanes to the guest's socket.
struct SocketLanes(std::path::PathBuf);

impl Dial for SocketLanes {
    fn dial(&self) -> Dialing<'_> {
        Box::pin(async move {
            let mut stream = tokio::net::UnixStream::connect(&self.0).await?;
            stream.write_all(&[1]).await?;
            Ok(Box::new(stream) as Raw)
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Serve as the guest, in this process, until the parent ends it.
async fn be_the_guest(spec: &str) {
    let (_records, _vault, changes) = records();
    let [socket, station] = spec.split(' ').collect::<Vec<_>>()[..] else {
        panic!("not a guest's spec: {spec}");
    };
    Console::Own.keep().unwrap();
    let keys = Keypair::generate().unwrap();
    let said = hex(keys.public().as_bytes());
    let mut daemon = Daemon::new("Aqua".into(), synthetic, || true, own_session);
    daemon.screen.console = Console::Own;
    let paired = Paired {
        keys,
        desktop: atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id(),
        station: PeerKey::from_hex(station).unwrap(),
        generations: Arc::new(Counted::default()),
        stop: stopping(&_records, &_vault),
        changes,
    };
    let listener = tokio::net::UnixListener::bind(socket).unwrap();
    // Said once it listens, whole: the parent connects only after.
    let public = std::path::Path::new(socket).with_extension("pub");
    let writing = public.with_extension("pub.new");
    std::fs::write(&writing, said).unwrap();
    std::fs::rename(writing, public).unwrap();
    // Its input service, a child of its own, as a guest's daemon keeps one.
    let input_folder = _records.path().join("input");
    std::fs::create_dir_all(&input_folder).unwrap();
    let input = crate::machine::input::Input::start(
        crate::machine::input::Starter::of_these_tests(&input_folder),
        input_folder,
    );
    let machine = Machine::new(
        daemon,
        paired,
        Arc::new(atlas_session::clock::Continuous),
        supervising(&_records, &_vault),
        Some(input),
    );
    let _ = machine.serve(&Socket(listener)).await;
}

/// A guest in a process of its own, the test binary again, and the
/// station's control of it and way to its lanes.
struct Guest {
    child: tokio::process::Child,
    desktop: Desktop,
    _controlling: Controlling,
    _folder: tempfile::TempDir,
}

/// Start the guest as the test named `test`, which must serve as the guest
/// when [`GUEST`] is set, and take control of it.
async fn guest(test: &str) -> Guest {
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("guest.sock");
    let station = Arc::new(Keypair::generate().unwrap());
    let spec = format!("{} {}", socket.display(), hex(station.public().as_bytes()));
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--test-threads=1"])
        .env(GUEST, spec)
        .kill_on_drop(true);
    let child = atlas_core::spawn::spawn_async(&mut command).unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while !socket.with_extension("pub").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the guest listens");
    let guest =
        PeerKey::from_hex(&std::fs::read_to_string(socket.with_extension("pub")).unwrap()).unwrap();
    let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();
    let controlling = station_control(reach(&socket, 0).await, &station, guest, &desktop)
        .await
        .unwrap();
    let peers = Peers {
        desktop,
        own: station,
        key: guest,
    };
    let lanes = Desktop::new(
        peers,
        controlling.generation,
        controlling.outbox.clone(),
        Box::new(SocketLanes(socket.clone())),
    );
    Guest {
        child,
        desktop: lanes,
        _controlling: controlling,
        _folder: folder,
    }
}

#[tokio::test]
async fn a_guest_in_a_process_of_its_own_serves_its_station_and_is_not_the_hidden_login() {
    if let Ok(spec) = std::env::var(GUEST) {
        return be_the_guest(&spec).await;
    }
    let mut guest = guest(
        "daemon::serve::machine::tests::a_guest_in_a_process_of_its_own_serves_its_station_and_is_not_the_hidden_login",
    )
    .await;
    let child_pid = guest.child.id().unwrap();

    // A guest still starting under load may take a few tries to answer.
    let mut replies = Vec::new();
    for _ in 0..3 {
        replies = ask(&guest.desktop, LaneKind::Read, &json!({"op": "hello"})).await;
        if !replies.is_empty() {
            break;
        }
    }
    let hello = replies.last().unwrap().clone();
    guest.child.kill().await.unwrap();

    assert_eq!(hello["ok"], true, "{hello}");
    assert_eq!(
        hello["daemon"]["pid"].as_u64(),
        Some(u64::from(child_pid)),
        "{hello}"
    );
    // As a guest account of its own would answer: this test runs as the
    // person, whom no proof takes for a helper. Its screen is as the child
    // saw it.
    let mut answered = hello.clone();
    answered["user"]["uid"] = json!(502);
    let hidden = atlas_core::session::require_hidden(&answered, 502).unwrap_err();
    assert_eq!(hidden.code, Code::OnConsole, "{}", hidden.error);
}

/// A command that adds a line to `file` each time it runs.
fn appending(file: &std::path::Path, change: &Token) -> Value {
    json!({"op": "run", "argv": ["/bin/sh", "-c", format!("echo ran >> '{}'", file.display())],
        "change": change})
}

#[tokio::test]
async fn a_change_asked_for_twice_runs_once_and_is_answered_from_what_it_came_to() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("runs");
    let change = Token::new();
    let lane = || LaneKind::Change {
        change: change.clone(),
    };

    let first = ask(&desktop, lane(), &appending(&file, &change)).await;
    let again = ask(&desktop, lane(), &appending(&file, &change)).await;

    assert_eq!(first.last().unwrap()["ok"], true, "{first:?}");
    assert_eq!(again, [first.last().unwrap().clone()]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "ran\n");
    assert!(matches!(
        served.changes.of(&change).unwrap(),
        Some(crate::machine::changes::State::Done { .. })
    ));
}

#[tokio::test]
async fn what_a_change_came_to_is_answered_by_a_daemon_that_started_since() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let change = Token::new();
    // Its reply lost on the way: the station never read it.
    let ran = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &appending(&folder.path().join("runs"), &change),
    )
    .await;
    let Served { records, vault, .. } = served;

    // A daemon that started since, with nothing in its memory.
    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::on((records, vault, changes)).await;
    let (_controlling, desktop) = restarted.control().await;
    let asked = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "outcome", "change": change}),
    )
    .await;

    assert_eq!(
        atlas_core::outcomes::Outcome::remembered(&asked[0]),
        Some(ran.last().unwrap().clone()),
        "{asked:?}"
    );
}

#[tokio::test]
async fn a_change_asked_again_when_its_records_cannot_be_trusted_is_uncertain() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let change = Token::new();
    let request = appending(&folder.path().join("runs"), &change);
    let lane = || LaneKind::Change {
        change: change.clone(),
    };
    ask(&desktop, lane(), &request).await;
    // The records gone, as a guest command could make them.
    std::fs::remove_file(served.records.path().join("manifest.json")).unwrap();
    let Served { records, vault, .. } = served;
    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::on((records, vault, changes)).await;
    let (_controlling, desktop) = restarted.control().await;

    let replies = ask(&desktop, lane(), &request).await;

    assert_eq!(replies[0]["uncertain"], true, "{replies:?}");
}

#[tokio::test]
async fn a_change_whose_outcome_is_unknown_is_never_run_again() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("runs");
    let change = Token::new();
    let request = appending(&file, &change);
    // Asked before by a daemon that died before it said what it came to.
    served.changes.ask(&change, "run", &request).unwrap();

    let replies = ask(&desktop, LaneKind::Change { change }, &request).await;

    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["code"], "uncertain", "{replies:?}");
    assert_eq!(replies[0]["uncertain"], true);
    assert!(!file.exists(), "it did not run");
}

#[tokio::test]
async fn a_change_refused_at_admission_is_kept_as_refused_and_can_be_asked_for() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();
    // Pinned to a session that is not this one.
    let request = json!({"op": "run", "argv": ["/usr/bin/true"], "change": change,
        "session": {"securitySessionId": 1, "displayId": 1}});

    let refused = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &request,
    )
    .await;
    let asked = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "outcome", "change": change}),
    )
    .await;

    assert_eq!(refused[0]["ok"], false, "{refused:?}");
    assert_eq!(asked[0]["outcome"], "done", "{asked:?}");
    assert_eq!(asked[0]["reply"], refused[0]);
}

#[tokio::test]
async fn a_change_that_is_not_a_request_leaves_no_intent_behind() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();

    let replies = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "run", "argv": "not a list", "change": change}),
    )
    .await;

    assert_eq!(replies[0]["code"], "bad-request", "{replies:?}");
    assert_eq!(served.changes.of(&change).unwrap(), None);
}

#[tokio::test]
async fn a_change_whose_reply_was_lost_is_still_answered_after_the_memory_moved_on() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let change = Token::new();
    // Its reply is lost: the station hangs up once the request is sent.
    let opened = desktop
        .open(LaneKind::Change {
            change: change.clone(),
        })
        .await
        .unwrap();
    let mut stream = opened.stream;
    let request = appending(&folder.path().join("runs"), &change);
    stream
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stream.flush().await.unwrap();
    drop(stream);
    let kept = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(crate::machine::changes::State::Done { reply }) =
                served.changes.of(&change).unwrap()
            {
                return reply;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("what it came to was kept");
    // More changes since than the daemon's memory holds.
    for _ in 0..260 {
        let other = Token::new();
        let refused = json!({"op": "run", "argv": ["/usr/bin/true"], "change": other,
            "session": {"securitySessionId": 1, "displayId": 1}});
        ask(&desktop, LaneKind::Change { change: other }, &refused).await;
    }

    let asked = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "outcome", "change": change}),
    )
    .await;

    assert_eq!(
        atlas_core::outcomes::Outcome::remembered(&asked[0]),
        Some(kept),
        "{asked:?}"
    );
}

#[tokio::test]
async fn a_done_change_is_named_at_every_hello_until_the_station_settles_it() {
    let served = Served::start().await;
    let (_first, desktop) = served.control().await;
    let folder = tempfile::tempdir().unwrap();
    let change = Token::new();
    ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &appending(&folder.path().join("runs"), &change),
    )
    .await;

    // A settlement lost with its connection: the next hello names it again.
    let (second, _desktop) = served.control().await;
    assert_eq!(second.unsettled, std::slice::from_ref(&change));
    let settling = atlas_session::control::settling(&second.unsettled, |_| false);
    second
        .outbox
        .say(atlas_session::control::Said::Settled { changes: settling })
        .unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        while served.changes.of(&change).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("forgotten once settled");
    let (third, _desktop) = served.control().await;
    assert!(third.unsettled.is_empty());
}

#[tokio::test]
async fn a_guest_whose_owner_went_ends_its_control_though_heartbeats_keep_coming() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (link, machine) = (served.link.clone(), Arc::downgrade(&served.machine));

    // Everything of the guest goes, the way this test reaches it too. The
    // link and the station's control stay, its heartbeats still coming.
    drop(desktop);
    drop(served);

    tokio::time::timeout(Duration::from_secs(5), async {
        while machine.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the guest went");
    tokio::time::timeout(Duration::from_secs(5), controlling.reading)
        .await
        .expect("its control ended")
        .ok();
    drop(link);
}

/// Wait until `served` keeps agents stopped as `kept`.
async fn kept_as(served: &Served, kept: crate::machine::stop::Kept) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while served.machine.paired.stop.kept().ok() != Some(kept.clone()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the Stop as it should be");
}

#[tokio::test]
async fn a_stop_ends_a_running_command_and_refuses_every_change_until_resumed() {
    use crate::machine::stop::{Kept, Why};
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let change = Token::new();
    let opened = desktop
        .open(LaneKind::Change {
            change: change.clone(),
        })
        .await
        .unwrap();
    let mut stream = opened.stream;
    let request = json!({"op": "run", "argv": ["/bin/sleep", "30"], "timeoutMs": 60000,
        "change": change});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stream.flush().await.unwrap();
    let mut lines = BufReader::new(stream).lines();
    let started: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();

    let ended: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(ended["code"], "stopped", "{ended}");
    assert!(nix::sys::signal::killpg(group, None).is_err());
    kept_as(&served, Kept::Stopped { why: Why::Person }).await;
    let other = Token::new();
    let refused = ask(
        &desktop,
        LaneKind::Change {
            change: other.clone(),
        },
        &json!({"op": "run", "argv": ["/usr/bin/true"], "change": other}),
    )
    .await;
    assert_eq!(refused[0]["code"], "stopped", "{refused:?}");

    controlling
        .outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();
    kept_as(&served, Kept::Clear).await;
    let again = Token::new();
    let ran = ask(
        &desktop,
        LaneKind::Change {
            change: again.clone(),
        },
        &json!({"op": "run", "argv": ["/usr/bin/true"], "change": again}),
    )
    .await;
    assert_eq!(ran.last().unwrap()["ok"], true, "{ran:?}");
}

#[tokio::test]
async fn a_generation_that_lapsed_leaves_agents_stopped_on_the_guests_disk() {
    use crate::machine::stop::{Kept, Why};
    let served = Served::start().await;
    let (_controlling, _desktop) = served.control().await;
    served.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });

    served.clock.advance(atlas_session::authority::LEASE * 2);

    kept_as(&served, Kept::Stopped { why: Why::Lapsed }).await;
}

#[tokio::test]
async fn a_guest_restarted_with_a_saved_stop_takes_no_change_until_resumed() {
    use crate::machine::stop::Why;
    let served = Served::start().await;
    served.machine.paired.stop.stop(Why::Person).unwrap();
    let Served { records, vault, .. } = served;
    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::on((records, vault, changes)).await;
    let (_controlling, desktop) = restarted.control().await;
    let change = Token::new();

    let refused = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "run", "argv": ["/usr/bin/true"], "change": change}),
    )
    .await;

    assert_eq!(refused[0]["code"], "stopped", "{refused:?}");
}

#[tokio::test]
async fn a_newer_generation_taking_the_place_of_one_is_no_stop() {
    let served = Served::start().await;
    let (_first, _old) = served.control().await;

    let (_second, _new) = served.control().await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Clear
    );
}

/// Start `/bin/sleep 30` on a change lane of `desktop`: the lines its
/// reply comes in, and its process group.
async fn sleeping(
    desktop: &Desktop,
) -> (
    tokio::io::Lines<BufReader<atlas_session::secure::Secure<Raw>>>,
    nix::unistd::Pid,
) {
    let change = Token::new();
    let opened = desktop
        .open(LaneKind::Change {
            change: change.clone(),
        })
        .await
        .unwrap();
    let mut stream = opened.stream;
    let request = json!({"op": "run", "argv": ["/bin/sleep", "30"], "timeoutMs": 60000,
        "change": change});
    stream
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stream.flush().await.unwrap();
    let mut lines = BufReader::new(stream).lines();
    let started: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let group = nix::unistd::Pid::from_raw(started["pid"].as_i64().unwrap() as i32);
    (lines, group)
}

/// Whether a new change on `desktop` is refused as stopped.
async fn refused_as_stopped(desktop: &Desktop) -> bool {
    let change = Token::new();
    let replies = ask(
        desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "run", "argv": ["/usr/bin/true"], "change": change}),
    )
    .await;
    replies[0]["code"] == "stopped"
}

#[tokio::test]
async fn a_command_admitted_before_a_stop_ends_though_a_resume_follows_at_once() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (mut lines, group) = sleeping(&desktop).await;

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    controlling
        .outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();

    let ended: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(ended["code"], "stopped", "{ended}");
    assert!(nix::sys::signal::killpg(group, None).is_err());
}

#[tokio::test]
async fn a_resume_from_a_generation_that_retired_lifts_nothing() {
    let served = Served::start().await;
    let (old, _old_desktop) = served.control().await;
    old.outbox.say(atlas_session::control::Said::Stop).unwrap();
    kept_as(
        &served,
        crate::machine::stop::Kept::Stopped {
            why: crate::machine::stop::Why::Person,
        },
    )
    .await;
    let (_new, desktop) = served.control().await;

    old.outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(matches!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Stopped { .. }
    ));
    assert!(refused_as_stopped(&desktop).await);
}

#[tokio::test]
async fn a_stop_that_could_not_be_saved_holds_and_no_resume_lifts_it() {
    use std::os::unix::fs::PermissionsExt;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let folder = served.records.path().to_owned();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    controlling
        .outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let halted = served.machine.daemon.halted.borrow().halted;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(halted, "still stopped");
    assert!(refused_as_stopped(&desktop).await);
}

#[tokio::test]
async fn a_generation_that_lapsed_ends_its_running_command_once_its_stop_is_saved() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let (_lines, group) = sleeping(&desktop).await;
    served.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });

    served.clock.advance(atlas_session::authority::LEASE * 2);

    tokio::time::timeout(Duration::from_secs(10), async {
        while nix::sys::signal::killpg(group, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the command was ended");
    assert!(matches!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Stopped { .. }
    ));
}

/// Run a command on `desktop` that leaves a process behind, and that
/// process's PID once the command has replied.
async fn leaving(desktop: &Desktop) -> i32 {
    use base64::Engine;
    let change = Token::new();
    let request = json!({"op": "run",
        "argv": ["/bin/sh", "-c", "/bin/sleep 30 >/dev/null 2>&1 & echo $!"],
        "change": change});
    let replies = ask(desktop, LaneKind::Change { change }, &request).await;
    assert_eq!(replies.last().unwrap()["exitCode"], 0, "{replies:?}");
    let said: Vec<u8> = replies
        .iter()
        .filter(|reply| reply["event"] == "output")
        .flat_map(|reply| {
            base64::engine::general_purpose::STANDARD
                .decode(reply["data"].as_str().unwrap())
                .unwrap()
        })
        .collect();
    String::from_utf8(said).unwrap().trim().parse().unwrap()
}

/// Whether process `pid` is gone, or only waits to be reaped, within `within`.
async fn gone_within(pid: i32, within: Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        if atlas_core::process::info(pid)
            .unwrap()
            .is_none_or(|info| info.zombie)
        {
            return true;
        }
        if started.elapsed() >= within {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn what_a_command_left_ends_once_agents_are_stopped_though_a_resume_follows() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let left = leaving(&desktop).await;
    assert!(!gone_within(left, Duration::ZERO).await);

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    controlling
        .outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();

    assert!(gone_within(left, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn what_a_command_left_ends_once_its_generation_is_replaced() {
    let served = Served::start().await;
    let (_old, desktop) = served.control().await;
    let left = leaving(&desktop).await;

    let _new = served.control().await;

    assert!(gone_within(left, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn a_guest_killed_with_no_restart_leaves_none_of_its_work_running() {
    if let Ok(spec) = std::env::var(GUEST) {
        return be_the_guest(&spec).await;
    }
    let mut guest = guest(
        "daemon::serve::machine::tests::a_guest_killed_with_no_restart_leaves_none_of_its_work_running",
    )
    .await;
    let left = leaving(&guest.desktop).await;
    assert!(!gone_within(left, Duration::ZERO).await);
    let input = input_services_of(guest.child.id().unwrap()).await;
    assert!(!input.is_empty(), "the guest runs its input service");

    guest.child.kill().await.unwrap();

    assert!(gone_within(left, Duration::from_secs(2)).await);
    for pid in input {
        assert!(
            gone_within(pid, Duration::from_secs(5)).await,
            "its input service outlived it"
        );
    }
}

/// The input services the guest `parent` started, once one is up.
async fn input_services_of(parent: u32) -> Vec<i32> {
    let parent = i32::try_from(parent).unwrap();
    for _ in 0..100 {
        let found: Vec<i32> = atlas_core::process::children(parent)
            .unwrap_or_default()
            .into_iter()
            .filter(|pid| {
                atlas_core::process::arguments(*pid).is_ok_and(|arguments| {
                    arguments
                        .iter()
                        .any(|argument| argument.contains("an_input_service_runs"))
                })
            })
            .collect();
        if !found.is_empty() {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Vec::new()
}

#[tokio::test]
async fn a_command_on_a_machine_lane_with_nothing_to_supervise_it_is_refused() {
    let served = Served::start().await;
    let (_controlling, _desktop) = served.control().await;
    let (authority, lanes) = {
        let live = served.machine.live.lock().await;
        let live = live.as_ref().unwrap();
        (live.authority.clone(), live.lanes.clone())
    };
    let asker = Asker {
        controller: true,
        host: None,
        gate: Some(Gate {
            authority,
            lanes,
            lane: LaneId(9),
            halted: tokio::sync::watch::Sender::new(super::super::Halt::default()).subscribe(),
            stops: 0,
        }),
    };
    let unsupervised = Daemon::new("Aqua".into(), synthetic, || true, own_session);
    assert!(super::super::unsupervised(&served.machine.daemon, &asker).is_none());

    let refusal = super::super::unsupervised(&unsupervised, &asker).unwrap();

    assert_eq!(refusal["code"], "platform", "{refusal}");
    assert_eq!(refusal["uncertain"], false, "{refusal}");
}

#[tokio::test]
async fn a_supervisor_killed_from_outside_stops_every_change_after_it() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let (_lines, supervisor) = sleeping(&desktop).await;
    assert!(!refused_as_stopped(&desktop).await);

    nix::sys::signal::kill(supervisor, nix::sys::signal::Signal::SIGKILL).unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert!(refused_as_stopped(&desktop).await);
    let _ = nix::sys::signal::killpg(supervisor, nix::sys::signal::Signal::SIGKILL);
}

#[tokio::test]
async fn a_resume_while_stopped_work_is_still_being_ended_lifts_nothing() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let left = leaving(&desktop).await;

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    controlling
        .outbox
        .say(atlas_session::control::Said::Resume)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(
        refused_as_stopped(&desktop).await,
        "resumed over work being ended"
    );
    assert!(gone_within(left, Duration::from_secs(2)).await);
    // Once its work is judged, a later Resume lifts it.
    let mut lifted = false;
    for _ in 0..25 {
        controlling
            .outbox
            .say(atlas_session::control::Said::Resume)
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !refused_as_stopped(&desktop).await {
            lifted = true;
            break;
        }
    }
    assert!(lifted, "a later Resume lifts it");
}

/// A guest's records with one supervisor an earlier daemon left, its PID
/// one no process can have, and its proof of `code` if it left one.
fn left_behind(code: Option<u8>) -> (Records, crate::machine::processes::Supervised) {
    let records = records();
    let supervising = supervising(&records.0, &records.1);
    let supervised = crate::machine::processes::Supervised {
        boot: atlas_core::process::boot().unwrap(),
        generation: atlas_session::generation::Generation(1),
        supervisor: atlas_core::process::Instance::new(100_500, [1, 0]),
    };
    supervising.kept.add(supervised.clone()).unwrap();
    if let Some(code) = code {
        crate::machine::supervise::proof::leave(&supervising.proofs, &supervised.supervisor, code)
            .unwrap();
    }
    (records, supervised)
}

/// Resume on `controlling` until changes are taken again, within 5 s.
async fn resumed(controlling: &Controlling, desktop: &Desktop) -> bool {
    for _ in 0..25 {
        controlling
            .outbox
            .say(atlas_session::control::Said::Resume)
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !refused_as_stopped(desktop).await {
            return true;
        }
    }
    false
}

#[tokio::test]
async fn a_guest_that_starts_on_work_left_behind_stops_agents_until_resumed() {
    let (records, left) = left_behind(Some(crate::machine::supervise::ENDED));

    let served = Served::on(records).await;
    let (controlling, desktop) = served.control().await;

    assert_eq!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Stopped {
            why: crate::machine::stop::Why::Restarted
        }
    );
    assert!(refused_as_stopped(&desktop).await);
    assert!(
        resumed(&controlling, &desktop).await,
        "a Resume lifts it once recovered"
    );
    let kept = served
        .machine
        .daemon
        .supervising
        .as_ref()
        .unwrap()
        .kept
        .kept()
        .unwrap();
    assert!(
        !kept.contains(&left),
        "what was left is forgotten: {kept:?}"
    );
}

#[tokio::test]
async fn a_guest_whose_left_work_is_not_known_to_have_ended_lifts_no_stop() {
    let (records, left) = left_behind(None);

    let served = Served::on(records).await;
    let (controlling, desktop) = served.control().await;

    assert!(
        !resumed(&controlling, &desktop).await,
        "lifted over work not known to have ended"
    );
    let kept = served
        .machine
        .daemon
        .supervising
        .as_ref()
        .unwrap()
        .kept
        .kept()
        .unwrap();
    assert_eq!(kept, [left]);
}

#[tokio::test]
async fn a_guest_that_could_not_save_its_stop_at_start_forgets_nothing_it_was_left() {
    let (records, left) = left_behind(Some(crate::machine::supervise::ENDED));
    // The Stop's file cannot be written: a folder stands in its place.
    std::fs::create_dir(records.0.path().join("stop")).unwrap();

    let served = Served::on(records).await;
    let (controlling, desktop) = served.control().await;

    assert!(
        !resumed(&controlling, &desktop).await,
        "lifted with no Stop saved"
    );
    let kept = served
        .machine
        .daemon
        .supervising
        .as_ref()
        .unwrap()
        .kept
        .kept()
        .unwrap();
    assert_eq!(kept, [left], "forgotten with no Stop saved");
}

/// What the guest says of the work it left.
async fn leftovers(desktop: &Desktop) -> Value {
    let replies = ask(desktop, LaneKind::Read, &json!({"op": "leftovers"})).await;
    replies.last().unwrap()["leftovers"].clone()
}

#[tokio::test]
async fn a_guest_that_ran_nothing_has_no_leftover_work() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    assert_eq!(leftovers(&desktop).await, "none");
}

#[tokio::test]
async fn a_guest_has_leftover_work_while_a_command_or_what_it_left_runs() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let (_lines, _group) = sleeping(&desktop).await;

    assert_eq!(leftovers(&desktop).await, "running");
}

#[tokio::test]
async fn work_left_running_is_leftover_until_it_ends_by_itself() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let left = leaving(&desktop).await;
    assert_eq!(leftovers(&desktop).await, "running");

    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(left),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();

    let mut answered = Value::Null;
    for _ in 0..25 {
        answered = leftovers(&desktop).await;
        if answered == "none" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(answered, "none");
}

#[tokio::test]
async fn work_left_by_an_earlier_daemon_with_no_proof_is_unknown() {
    let (records, _left) = left_behind(None);

    let served = Served::on(records).await;
    let (_controlling, desktop) = served.control().await;

    assert_eq!(leftovers(&desktop).await, "unknown");
}

#[tokio::test]
async fn work_kept_from_another_boot_is_forgotten_before_none_is_said() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let kept = served
        .machine
        .daemon
        .supervising
        .as_ref()
        .unwrap()
        .kept
        .clone();
    let earlier = crate::machine::processes::Supervised {
        boot: "an earlier boot".into(),
        generation: atlas_session::generation::Generation(1),
        supervisor: atlas_core::process::Instance::new(100_500, [1, 0]),
    };
    kept.add(earlier).unwrap();

    assert_eq!(leftovers(&desktop).await, "none");
    assert_eq!(kept.kept().unwrap(), []);
}

#[tokio::test]
async fn running_work_never_hides_a_record_that_cannot_be_trusted() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let (_lines, _group) = sleeping(&desktop).await;
    // Once its supervisor is kept: the word to start comes after.
    let kept = served
        .machine
        .daemon
        .supervising
        .as_ref()
        .unwrap()
        .kept
        .clone();
    for _ in 0..50 {
        if kept.kept().is_ok_and(|work| !work.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    std::fs::write(
        served
            .records
            .path()
            .join("supervisors")
            .join("processes.json"),
        b"{",
    )
    .unwrap();

    assert_eq!(leftovers(&desktop).await, "unknown");
}

#[tokio::test]
async fn a_stop_holds_input_and_a_resume_lifts_it_once_it_is_settled() {
    let _turn = crate::machine::input::tests::ONE_AT_A_TIME.lock().await;
    let folder = tempfile::tempdir().unwrap();
    let input = crate::machine::input::Input::start(
        crate::machine::input::Starter::of_these_tests(folder.path()),
        folder.path().to_owned(),
    );
    let served = Served::with_input(records(), Some(input)).await;
    let (controlling, desktop) = served.control().await;
    let input = served.machine.daemon.input.clone().unwrap();
    let standing = input.standing();
    for _ in 0..200 {
        if *standing.borrow() == crate::machine::input::Standing::Ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(*standing.borrow(), crate::machine::input::Standing::Ready);

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(input.word(), (1, true), "a Stop holds input");

    assert!(
        resumed(&controlling, &desktop).await,
        "a Resume lifts it once input is settled"
    );
    assert_eq!(input.word(), (2, false), "a Resume lifts the hold");
    let mut settled = false;
    for _ in 0..50 {
        if !input.unsettled() {
            settled = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(settled, "the lifting was acknowledged");
}

#[tokio::test]
async fn a_resume_while_input_is_not_ready_lifts_nothing() {
    // An input service that never starts.
    let folder = tempfile::tempdir().unwrap();
    let input = crate::machine::input::Input::start(
        crate::machine::input::Starter::new("/nonexistent/atlas".into(), Vec::new()),
        folder.path().to_owned(),
    );
    let served = Served::with_input(records(), Some(input)).await;
    let (controlling, desktop) = served.control().await;
    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !resumed(&controlling, &desktop).await,
        "lifted with input not ready"
    );
    let input = served.machine.daemon.input.as_ref().unwrap();
    assert_eq!(input.word(), (1, true), "the hold was lifted");
}

/// A guest with a real input service, and the station's way to its lanes,
/// once the service is ready.
async fn with_input_service() -> (
    Served,
    Controlling,
    Desktop,
    tempfile::TempDir,
    tokio::sync::MutexGuard<'static, ()>,
) {
    let turn = crate::machine::input::tests::ONE_AT_A_TIME.lock().await;
    let folder = tempfile::tempdir().unwrap();
    let input = crate::machine::input::Input::start(
        crate::machine::input::Starter::of_these_tests(folder.path()),
        folder.path().to_owned(),
    );
    let served = Served::with_input(records(), Some(input)).await;
    let (controlling, desktop) = served.control().await;
    let standing = served.machine.daemon.input.as_ref().unwrap().standing();
    for _ in 0..200 {
        if *standing.borrow() == crate::machine::input::Standing::Ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(*standing.borrow(), crate::machine::input::Standing::Ready);
    (served, controlling, desktop, folder, turn)
}

#[tokio::test]
async fn a_read_on_an_input_lane_is_answered_by_the_input_service() {
    let (_served, _controlling, desktop, _folder, _turn) = with_input_service().await;

    let replies = ask(
        &desktop,
        LaneKind::Input { change: None },
        &json!({"op": "hello"}),
    )
    .await;

    let hello = replies.last().unwrap();
    assert!(
        hello.get("ready").is_some(),
        "not the input service's hello: {hello}"
    );
}

#[tokio::test]
async fn a_change_on_an_input_lane_is_kept_and_answered_again_from_what_it_came_to() {
    let (served, _controlling, desktop, _folder, _turn) = with_input_service().await;
    let change = Token::new();
    let release = json!({"op": "release", "owner": Token::new(), "change": change});
    let lane = LaneKind::Input {
        change: Some(change.clone()),
    };
    let first = ask(&desktop, lane.clone(), &release).await;
    let first = first.last().unwrap().clone();
    let kept = served.changes.of(&change).unwrap();
    assert!(
        matches!(kept, Some(crate::machine::changes::State::Done { ref reply }) if *reply == first),
        "{kept:?}"
    );
    // A reply the service never gives: only the record can answer with it.
    let recorded = json!({"ok": true, "only": "the record"});
    served.changes.done(&change, &recorded).unwrap();

    let again = tokio::join!(
        ask(&desktop, lane.clone(), &release),
        ask(&desktop, lane.clone(), &release),
        ask(&desktop, lane.clone(), &release),
    );

    for again in [again.0, again.1, again.2] {
        assert_eq!(again.last(), Some(&recorded), "run again: {again:?}");
    }
}

#[tokio::test]
async fn a_change_asked_again_while_agents_are_stopped_is_answered_from_its_record() {
    let (served, controlling, desktop, _folder, _turn) = with_input_service().await;
    let (done, intended, new) = (Token::new(), Token::new(), Token::new());
    let request = |change: &Token| json!({"op": "release", "owner": "agent", "change": change});
    let recorded = json!({"ok": true, "only": "the record"});
    served
        .changes
        .ask(&done, "release", &request(&done))
        .unwrap();
    served.changes.done(&done, &recorded).unwrap();
    served
        .changes
        .ask(&intended, "release", &request(&intended))
        .unwrap();
    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let lane = |change: &Token| LaneKind::Input {
        change: Some(change.clone()),
    };

    let done_again = ask(&desktop, lane(&done), &request(&done)).await;
    let intended_again = ask(&desktop, lane(&intended), &request(&intended)).await;
    let refused = ask(&desktop, lane(&new), &request(&new)).await;

    assert_eq!(done_again.last(), Some(&recorded), "{done_again:?}");
    let intended_again = intended_again.last().unwrap();
    assert_eq!(intended_again["uncertain"], true, "{intended_again}");
    let refused = refused.last().unwrap();
    assert_eq!(refused["ok"], false, "{refused}");
    assert_ne!(refused["uncertain"], true, "{refused}");
    let kept = served.changes.of(&new).unwrap();
    assert!(
        matches!(kept, Some(crate::machine::changes::State::Done { ref reply }) if reply == refused),
        "{kept:?}"
    );
}

#[tokio::test]
async fn what_an_input_lane_was_not_opened_for_is_refused() {
    let (_served, _controlling, desktop, _folder, _turn) = with_input_service().await;
    let change = Token::new();

    let unnamed = ask(
        &desktop,
        LaneKind::Input {
            change: Some(change.clone()),
        },
        &json!({"op": "release", "owner": Token::new()}),
    )
    .await;
    let held = ask(
        &desktop,
        LaneKind::Input {
            change: Some(change),
        },
        &json!({"op": "hold", "on": false}),
    )
    .await;
    let read_with_a_change = ask(
        &desktop,
        LaneKind::Input {
            change: Some(Token::new()),
        },
        &json!({"op": "hello"}),
    )
    .await;

    for replies in [unnamed, held, read_with_a_change] {
        let reply = replies.last().unwrap();
        assert_eq!(reply["code"], "bad-request", "{reply}");
    }
}

#[tokio::test]
async fn an_input_lane_of_a_desktop_with_no_input_service_is_refused() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let replies = ask(
        &desktop,
        LaneKind::Input { change: None },
        &json!({"op": "hello"}),
    )
    .await;

    assert_eq!(replies.last().unwrap()["code"], "platform");
}
