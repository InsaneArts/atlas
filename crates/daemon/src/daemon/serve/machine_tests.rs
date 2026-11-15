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
struct Channel(Mutex<mpsc::UnboundedReceiver<Framing>>);

impl Listener for Channel {
    fn accept(&self) -> Accepting<'_> {
        Box::pin(async {
            let connection = self.0.lock().await.recv().await;
            let connection = connection.ok_or(io::ErrorKind::BrokenPipe)?;
            Ok(connection)
        })
    }
}

/// A connection that says at once what it is for, as the guest's listener
/// hands it over.
fn framed(purpose: Purpose, raw: Raw) -> Framing {
    Box::pin(std::future::ready(Ok((purpose, raw))))
}

/// The station dialling lanes across the link, each handed to the guest.
struct Lanes(Arc<Link>, mpsc::UnboundedSender<Framing>);

impl Dial for Lanes {
    fn dial(&self) -> Dialing<'_> {
        let (station, guest, _cut) = self.0.connect();
        let _ = self.1.send(framed(Purpose::Lane, Box::new(guest)));
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
    connections: mpsc::UnboundedSender<Framing>,
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
        Self::with_parts(records, input, true).await
    }

    /// A guest serving with `records`, its input service `input` if any,
    /// and stages if `staging`.
    async fn with_parts(
        records: Records,
        input: Option<crate::machine::input::Input>,
        staging: bool,
    ) -> Self {
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
            enrolment: Token::new(),
            pairing: Arc::new(crate::machine::pairing::Pairing::new(
                vault.clone(),
                records.path().join("pairing-secret"),
            )),
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
            staging.then(|| crate::machine::stage::Stages::new(records.path().join("open"))),
            Some(crate::machine::mirror::Mirrors::default()),
        );
        machine.seen_as(crate::daemon::facts::Posture::CLOSED);
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
            .send(framed(Purpose::Control, Box::new(guest_end)))
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

/// A read answered from what the guest keeps, asking macOS nothing: a
/// hello reads permissions and displays, which a busy Mac can take many
/// seconds to give.
static QUICK_READ: std::sync::LazyLock<Value> =
    std::sync::LazyLock::new(|| json!({"op": "job", "action": "show"}));

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

    let replies = ask(&desktop, LaneKind::Read, &QUICK_READ).await;

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
async fn a_guest_open_to_more_than_atlas_holds_agents_lanes_and_still_answers_reads() {
    let served = Served::start().await;
    served.machine.seen_as(crate::daemon::facts::Posture {
        remote_login: Some(true),
        admin: Some(false),
    });
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();
    let request = json!({"op": "run", "argv": ["/bin/echo", "hi"], "change": change});

    let held = ask(&desktop, LaneKind::Change { change }, &request).await;
    let read = ask(&desktop, LaneKind::Read, &QUICK_READ).await;

    assert!(held.is_empty(), "{held:?}");
    assert_eq!(read.last().unwrap()["ok"], true, "{read:?}");
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
    let replies = ask(&new, LaneKind::Read, &QUICK_READ).await;
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
            .send(framed(Purpose::Lane, Box::new(guest)))
            .unwrap();
        held.push((station, cut));
    }
    let (mut over, guest, _cut) = served.link.connect();
    served
        .connections
        .send(framed(Purpose::Lane, Box::new(guest)))
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

/// The guest's own listener on a Unix socket, as a stand-in for its vsock
/// port: each connection names what it is for in its first byte.
fn socket_listener(path: &std::path::Path) -> super::listen::Guest {
    let listening = std::os::unix::net::UnixListener::bind(path).unwrap();
    listening.set_nonblocking(true).unwrap();
    super::listen::Guest::of(std::os::fd::OwnedFd::from(listening)).unwrap()
}

/// Reach the guest's socket at `path` for `purpose`.
async fn reach(path: &std::path::Path, purpose: Purpose) -> tokio::net::UnixStream {
    let mut stream = tokio::net::UnixStream::connect(path).await.unwrap();
    stream.write_all(&[purpose.byte()]).await.unwrap();
    stream
}

/// The station dialling lanes to the guest's socket.
struct SocketLanes(std::path::PathBuf);

impl Dial for SocketLanes {
    fn dial(&self) -> Dialing<'_> {
        Box::pin(async move {
            let mut stream = tokio::net::UnixStream::connect(&self.0).await?;
            stream.write_all(&[Purpose::Lane.byte()]).await?;
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
        enrolment: Token::new(),
        pairing: Arc::new(crate::machine::pairing::Pairing::new(
            _vault.clone(),
            _records.path().join("pairing-secret"),
        )),
        generations: Arc::new(Counted::default()),
        stop: stopping(&_records, &_vault),
        changes,
    };
    let listener = socket_listener(std::path::Path::new(socket));
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
        None,
        None,
    );
    machine.seen_as(crate::daemon::facts::Posture::CLOSED);
    let _ = machine.serve(&listener).await;
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
    let controlling = station_control(
        reach(&socket, Purpose::Control).await,
        &station,
        guest,
        &desktop,
    )
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
        job: None,
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
        job: None,
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

/// How a test's station sends a stage's bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sending {
    /// The manifest and every file, as they are.
    Whole,
    /// The last byte of the last file changed.
    Changed,
    /// The manifest and half the contents, then nothing more.
    Halfway,
    /// Every file, then one byte more.
    Extra,
}

/// A small tree to stage: a script, a nested file, a link and a
/// quarantine flag, in a folder of its own.
fn tree_to_stage() -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let folder = tempfile::tempdir().unwrap();
    let tree = folder.path().join("Tool.app");
    std::fs::create_dir_all(tree.join("Contents/MacOS")).unwrap();
    std::fs::write(tree.join("Contents/Info.plist"), b"<plist/>").unwrap();
    let script = tree.join("Contents/MacOS/tool");
    std::fs::write(&script, b"#!/bin/sh\necho staged\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink("MacOS/tool", tree.join("Contents/run")).unwrap();
    let marked = atlas_core::spawn::status(
        std::process::Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0081;5f;Atlas;"])
            .arg(&tree),
    )
    .unwrap();
    assert!(marked.success());
    (folder, tree)
}

/// Stage `source` as `change` on a lane, its bytes paced by `credit` and
/// sent as `sending` says: every line the guest said, in order.
async fn stage(
    desktop: &Desktop,
    credit: &Credit,
    change: &Token,
    source: &std::path::Path,
    sending: Sending,
) -> Vec<Value> {
    use std::io::Read;
    let snapshot = atlas_core::stage::Snapshot::of(source).unwrap();
    let manifest = serde_json::to_vec(&snapshot).unwrap();
    let request = json!({
        "op": "stage",
        "change": change,
        "name": snapshot.name,
        "digest": snapshot.digest,
        "entries": snapshot.entries.len(),
        "size": snapshot.size(),
        "attributes": snapshot.attribute_bytes(),
        "manifest": manifest.len(),
    });
    let opened = desktop
        .open(LaneKind::Stage {
            change: change.clone(),
        })
        .await
        .unwrap();
    let lane = opened.lane;
    let (reading, mut writing) = tokio::io::split(opened.stream);
    let mut lines = BufReader::new(reading).lines();
    let mut line = serde_json::to_vec(&request).unwrap();
    line.push(b'\n');
    writing.write_all(&line).await.unwrap();
    writing.flush().await.unwrap();
    let mut said = Vec::new();
    let Ok(Ok(Some(first))) =
        tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await
    else {
        return said;
    };
    let first: Value = serde_json::from_str(&first).unwrap();
    said.push(first.clone());
    if first != json!({"ready": true}) {
        return said;
    }
    let mut bytes = (manifest.len() as u64).to_be_bytes().to_vec();
    bytes.extend(&manifest);
    let contents_at = bytes.len();
    for (entry, _, _) in snapshot.files() {
        let mut read = Vec::new();
        atlas_core::stage::open_file(source, &entry.path)
            .unwrap()
            .read_to_end(&mut read)
            .unwrap();
        bytes.extend(read);
    }
    match sending {
        Sending::Whole => {}
        Sending::Changed => *bytes.last_mut().unwrap() ^= 1,
        Sending::Halfway => bytes.truncate(contents_at + (bytes.len() - contents_at) / 2),
        Sending::Extra => bytes.push(b'!'),
    }
    // Bounded, so a send the guest stopped taking never holds the test.
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        credit.send(lane, &mut writing, &bytes).await?;
        writing
            .flush()
            .await
            .map_err(atlas_core::error::Error::from)
    })
    .await;
    while let Ok(Ok(Some(line))) =
        tokio::time::timeout(Duration::from_secs(20), lines.next_line()).await
    {
        said.push(serde_json::from_str(&line).unwrap());
    }
    credit.closed(lane);
    said
}

/// The guest's copies folder.
fn copies(served: &Served) -> std::path::PathBuf {
    served.records.path().join("open")
}

#[tokio::test]
async fn a_tree_staged_is_placed_whole_and_what_it_came_to_is_kept() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();

    let said = stage(
        &desktop,
        &controlling.credit,
        &change,
        &tree,
        Sending::Whole,
    )
    .await;

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], true, "{said:?}");
    let placed = std::path::PathBuf::from(reply["path"].as_str().unwrap());
    assert_eq!(
        placed,
        copies(&served).join(change.as_str()).join("Tool.app")
    );
    assert_eq!(
        atlas_core::stage::Snapshot::of(&placed).unwrap().entries,
        atlas_core::stage::Snapshot::of(&tree).unwrap().entries
    );
    assert!(
        matches!(served.changes.of(&change).unwrap(), Some(State::Done { reply: ref kept }) if kept == reply)
    );
}

#[tokio::test]
async fn a_stage_asked_again_after_a_restart_is_answered_from_its_record_with_no_bytes_sent() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();
    let first = stage(
        &desktop,
        &controlling.credit,
        &change,
        &tree,
        Sending::Whole,
    )
    .await;
    let Served { records, vault, .. } = served;

    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::on((records, vault, changes)).await;
    let (controlling, desktop) = restarted.control().await;
    let again = stage(
        &desktop,
        &controlling.credit,
        &change,
        &tree,
        Sending::Whole,
    )
    .await;

    assert_eq!(again.len(), 1, "it asked for the bytes again: {again:?}");
    assert_eq!(again.last(), first.last());
}

#[tokio::test]
async fn bytes_that_are_not_the_manifest_s_leave_nothing_placed_and_are_kept_as_refused() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();

    let said = stage(
        &desktop,
        &controlling.credit,
        &change,
        &tree,
        Sending::Changed,
    )
    .await;

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], false, "{said:?}");
    assert_ne!(reply["uncertain"], true, "{reply}");
    let left: Vec<_> = std::fs::read_dir(copies(&served)).unwrap().collect();
    assert!(left.is_empty(), "something was left: {left:?}");
    assert!(
        matches!(served.changes.of(&change).unwrap(), Some(State::Done { reply: ref kept }) if kept == reply)
    );
}

#[tokio::test]
async fn a_stage_naming_another_change_than_its_lane_is_refused() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let (lane, asked) = (Token::new(), Token::new());

    let said = ask(
        &desktop,
        LaneKind::Stage { change: lane },
        &json!({
            "op": "stage", "change": asked, "name": "Tool.app", "digest": "0",
            "entries": 1, "size": 0, "attributes": 0, "manifest": 2,
        }),
    )
    .await;

    assert_eq!(said.last().unwrap()["code"], "bad-request", "{said:?}");
    assert_eq!(served.changes.of(&asked).unwrap(), None);
}

#[tokio::test]
async fn a_stop_while_a_stage_s_bytes_arrive_leaves_nothing_placed() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();
    let staging = copies(&served).join(format!(".staging-{}", change.as_str()));
    let credit = controlling.credit.clone();
    let staged = {
        let (change, tree) = (change.clone(), tree.clone());
        tokio::spawn(
            async move { stage(&desktop, &credit, &change, &tree, Sending::Halfway).await },
        )
    };
    for _ in 0..100 {
        if staging.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(staging.exists(), "the stage never began writing");

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    let said = staged.await.unwrap();

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], false, "{said:?}");
    assert!(!staging.exists());
    assert!(!copies(&served).join(change.as_str()).exists());
}

#[tokio::test]
async fn a_stage_larger_than_the_bulk_credit_is_placed_whole() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let big: Vec<u8> = (0..3 << 20).map(|at: u32| (at % 251) as u8).collect();
    std::fs::write(tree.join("Contents/big.bin"), &big).unwrap();
    let change = Token::new();

    let said = stage(
        &desktop,
        &controlling.credit,
        &change,
        &tree,
        Sending::Whole,
    )
    .await;

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], true, "{said:?}");
    let placed = std::path::PathBuf::from(reply["path"].as_str().unwrap());
    assert_eq!(std::fs::read(placed.join("Contents/big.bin")).unwrap(), big);
    assert_eq!(
        controlling.credit.left(),
        atlas_session::credit::BULK as usize
    );
}

#[tokio::test]
async fn one_change_staged_twice_at_once_runs_once_and_both_hear_what_it_came_to() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();

    let (first, second) = tokio::join!(
        stage(
            &desktop,
            &controlling.credit,
            &change,
            &tree,
            Sending::Whole
        ),
        stage(
            &desktop,
            &controlling.credit,
            &change,
            &tree,
            Sending::Whole
        ),
    );

    assert_eq!(first.last(), second.last(), "{first:?} {second:?}");
    assert_eq!(first.last().unwrap()["ok"], true);
    let readies = [&first, &second]
        .iter()
        .filter(|said| said.first() == Some(&json!({"ready": true})))
        .count();
    assert_eq!(readies, 1, "both were told to send");
}

#[tokio::test]
async fn a_station_that_hangs_up_once_its_bytes_are_in_has_its_stage_ended_before_it_is_placed() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();
    let stages = served.machine.daemon.stages.clone().unwrap();
    let (held, release) = stages.hold_before_commit();
    let credit = controlling.credit.clone();
    let staging = {
        let (change, tree) = (change.clone(), tree.clone());
        tokio::spawn(async move { stage(&desktop, &credit, &change, &tree, Sending::Whole).await })
    };
    tokio::time::timeout(Duration::from_secs(10), held)
        .await
        .unwrap()
        .unwrap();

    // The station goes, its lane with it, while the stage waits to commit.
    staging.abort();
    let _ = staging.await;
    let _ = release.send(());

    let mut kept = None;
    for _ in 0..100 {
        if let Some(State::Done { reply }) = served.changes.of(&change).unwrap() {
            kept = Some(reply);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let kept = kept.expect("what it came to was never kept");
    assert_eq!(kept["ok"], false, "{kept}");
    assert!(!copies(&served).join(change.as_str()).exists());
}

#[tokio::test]
async fn a_stop_received_while_a_stage_waits_to_commit_refuses_its_commit() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();
    let stages = served.machine.daemon.stages.clone().unwrap();
    let (held, release) = stages.hold_before_commit();
    let credit = controlling.credit.clone();
    let staging = {
        let (change, tree) = (change.clone(), tree.clone());
        tokio::spawn(async move { stage(&desktop, &credit, &change, &tree, Sending::Whole).await })
    };
    tokio::time::timeout(Duration::from_secs(10), held)
        .await
        .unwrap()
        .unwrap();

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = release.send(());
    let said = staging.await.unwrap();

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], false, "{said:?}");
    assert!(!copies(&served).join(change.as_str()).exists());
}

#[tokio::test]
async fn bytes_past_a_stage_s_last_file_end_it_with_nothing_placed() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let change = Token::new();
    let stages = served.machine.daemon.stages.clone().unwrap();
    // Held until the extra byte has surely come.
    let (held, release) = stages.hold_before_commit();
    let credit = controlling.credit.clone();
    let staging = {
        let (change, tree) = (change.clone(), tree.clone());
        tokio::spawn(async move { stage(&desktop, &credit, &change, &tree, Sending::Extra).await })
    };
    tokio::time::timeout(Duration::from_secs(10), held)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = release.send(());

    let said = staging.await.unwrap();

    let reply = said.last().unwrap();
    assert_eq!(reply["ok"], false, "{said:?}");
    assert!(!copies(&served).join(change.as_str()).exists());
}

#[tokio::test]
async fn a_staged_copy_is_listed_then_removed_as_a_change_answered_again_from_its_record() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let (_source, tree) = tree_to_stage();
    let copy = Token::new();
    let staged = stage(&desktop, &controlling.credit, &copy, &tree, Sending::Whole).await;
    assert_eq!(staged.last().unwrap()["ok"], true, "{staged:?}");

    let listed = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "copies", "copy": copy}),
    )
    .await;
    let listed = listed.last().unwrap()["copies"][0].clone();
    assert_eq!(listed["state"], "placed", "{listed}");
    assert_eq!(listed["path"], staged.last().unwrap()["path"]);
    let change = Token::new();
    let removal =
        json!({"op": "remove-copy", "copy": copy, "node": listed["node"], "change": change});
    let lane = || LaneKind::Change {
        change: change.clone(),
    };
    let removed = ask(&desktop, lane(), &removal).await;
    // Its reply lost on the way, it is asked for again.
    let again = ask(&desktop, lane(), &removal).await;

    assert_eq!(removed.last().unwrap()["outcome"], "removed", "{removed:?}");
    assert_eq!(again.last(), removed.last());
    assert!(!copies(&served).join(copy.as_str()).exists());
    let left = ask(&desktop, LaneKind::Read, &json!({"op": "copies"})).await;
    assert_eq!(left.last().unwrap()["copies"], json!([]), "{left:?}");
}

#[tokio::test]
async fn a_guest_declares_only_what_it_was_made_to_serve() {
    use atlas_session::compatibility::{
        CHANGES, COPIES, INPUT_LANES, LEFTOVERS, PROTOCOL, STAGE, Usable, judge,
    };
    let served = Served::start().await;

    let (controlling, _desktop) = served.control().await;

    let declared = &controlling.declared;
    assert_eq!(declared.protocol, PROTOCOL);
    assert_eq!(declared.version, env!("CARGO_PKG_VERSION"));
    for served in [CHANGES, LEFTOVERS, STAGE, COPIES] {
        assert!(
            declared.capabilities.iter().any(|has| has == served),
            "{declared:?}"
        );
    }
    assert!(!declared.capabilities.iter().any(|has| has == INPUT_LANES));
    assert_eq!(judge(declared, &[STAGE, COPIES]), Usable::Usable);
    assert_eq!(
        judge(declared, &[INPUT_LANES]),
        Usable::MissingCapabilities {
            missing: vec![INPUT_LANES.to_owned()]
        }
    );
}

#[tokio::test]
async fn a_guest_with_an_input_service_declares_its_input_lanes() {
    let (_served, controlling, _desktop, _folder, _turn) = with_input_service().await;

    assert!(
        controlling
            .declared
            .capabilities
            .iter()
            .any(|has| has == atlas_session::compatibility::INPUT_LANES)
    );
}

#[tokio::test]
async fn a_guest_started_again_without_stages_is_judged_by_what_it_declares_now() {
    use atlas_session::compatibility::{COPIES, STAGE, Usable, judge};
    let served = Served::start().await;
    let (first, _desktop) = served.control().await;
    assert_eq!(judge(&first.declared, &[STAGE]), Usable::Usable);
    let Served { records, vault, .. } = served;

    let changes = Arc::new(Changes::new(records.path(), vault.clone()));
    let restarted = Served::with_parts((records, vault, changes), None, false).await;
    let (second, _desktop) = restarted.control().await;

    let has = |name: &str| second.declared.capabilities.iter().any(|has| has == name);
    assert!(!has(STAGE) && !has(COPIES), "{:?}", second.declared);
    assert_eq!(
        judge(&second.declared, &[STAGE]),
        Usable::MissingCapabilities {
            missing: vec![STAGE.to_owned()]
        }
    );
}

/// A loopback port free now.
fn free_port(family: atlas_session::mirror::Family) -> u16 {
    let endpoint = atlas_session::mirror::Endpoint {
        family,
        port: 0,
        incarnation: 0,
    };
    std::net::TcpListener::bind(endpoint.address())
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A server on this Mac that answers each connection with what it read,
/// upper-cased, once the client has finished sending.
async fn shouting(address: std::net::SocketAddr) -> std::net::SocketAddr {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let bound = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut heard = Vec::new();
                let _ = stream.read_to_end(&mut heard).await;
                let _ = stream.write_all(&heard.to_ascii_uppercase()).await;
            });
        }
    });
    bound
}

/// The test's station for mirrors, as step 5's will be: it says a list
/// only to a guest that declares mirrors, says it again until it is
/// answered, and opens a lane only for a connection on an endpoint of its
/// current list in that very incarnation, once each, at most twelve at
/// once, connecting each to the server `targets` names for its port: the
/// translation a test on one Mac needs, as the guest cannot listen on the
/// server's own port.
struct MirrorStation {
    controlling: Arc<Controlling>,
    /// Its way to the generation's lanes, shared with its relays: lane IDs
    /// are given by one counter per generation.
    desktop: Arc<Desktop>,
    listed: tokio::sync::watch::Sender<Vec<atlas_session::mirror::Endpoint>>,
    relaying: tokio::task::JoinHandle<()>,
}

impl Drop for MirrorStation {
    /// The station goes: every relay it owns ends with it.
    fn drop(&mut self) {
        self.relaying.abort();
    }
}

impl MirrorStation {
    /// Say list `list` and wait for its answer, saying it again meanwhile.
    async fn mirror(
        &self,
        list: u64,
        endpoints: Vec<atlas_session::mirror::Endpoint>,
    ) -> Vec<atlas_session::mirror::Refused> {
        let declared = &self.controlling.declared;
        assert_eq!(
            atlas_session::compatibility::judge(declared, &[atlas_session::compatibility::MIRRORS]),
            atlas_session::compatibility::Usable::Usable,
            "a guest without mirrors is never sent a list"
        );
        self.listed.send_replace(endpoints.clone());
        for _ in 0..10 {
            self.controlling
                .outbox
                .say(atlas_session::control::Said::Mirror {
                    list,
                    endpoints: endpoints.clone(),
                })
                .unwrap();
            let answered =
                tokio::time::timeout(Duration::from_secs(1), self.controlling.heard.taken(list))
                    .await;
            if let Ok(refused) = answered {
                return refused;
            }
        }
        panic!("the guest never answered list {list}");
    }
}

fn relaying(
    controlling: Controlling,
    desktop: Desktop,
    targets: std::collections::HashMap<u16, std::net::SocketAddr>,
) -> MirrorStation {
    let controlling = Arc::new(controlling);
    let listed = tokio::sync::watch::Sender::new(Vec::new());
    let (station, mut current) = (controlling.clone(), listed.subscribe());
    let desktop = Arc::new(desktop);
    let shared = desktop.clone();
    let lanes = Arc::new(tokio::sync::Semaphore::new(atlas_session::mirror::LANES));
    let relaying = tokio::spawn(async move {
        let mut seen = std::collections::HashSet::new();
        // Every relay it runs, owned here: one for an endpoint no longer
        // listed ends, and all end when this does.
        let mut relays = tokio::task::JoinSet::new();
        let mut by_endpoint = Vec::new();
        let mut looking = tokio::time::interval(Duration::from_millis(200));
        loop {
            let asked = tokio::select! {
                // Its control connection gone, its generation is over: every
                // relay ends as this does.
                _ = looking.tick() => {
                    if station.reading.is_finished() {
                        return;
                    }
                    continue;
                }
                asked = station.heard.connect() => asked,
                changed = current.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let still = current.borrow_and_update().clone();
                    by_endpoint.retain(|(endpoint, relay): &(atlas_session::mirror::Endpoint, tokio::task::AbortHandle)| {
                        let kept = still.contains(endpoint);
                        if !kept {
                            relay.abort();
                        }
                        kept
                    });
                    continue;
                }
                Some(_) = relays.join_next(), if !relays.is_empty() => continue,
            };
            let listed = current.borrow().contains(&asked.endpoint);
            let target = targets.get(&asked.endpoint.port).copied();
            let (Some(target), true, true) = (target, listed, seen.insert(asked.connection)) else {
                continue;
            };
            let Ok(place) = lanes.clone().try_acquire_owned() else {
                continue;
            };
            let (desktop, station) = (desktop.clone(), station.clone());
            let relay = relays.spawn(async move {
                let _place = place;
                let _ = atlas_session::mirror::relay(
                    &desktop,
                    &station.credit,
                    &station.outbox,
                    asked.connection,
                    asked.endpoint,
                    target,
                )
                .await;
            });
            by_endpoint.push((asked.endpoint, relay));
        }
    });
    MirrorStation {
        controlling,
        desktop: shared,
        listed,
        relaying,
    }
}

/// What a client gets back sending `said` to `address`, its own end of
/// stream passed on.
async fn shout_through(address: std::net::SocketAddr, said: &[u8]) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream.write_all(said).await?;
    stream.shutdown().await?;
    let mut heard = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut heard))
        .await
        .map_err(|_| std::io::Error::other("no answer"))??;
    Ok(heard)
}

fn endpoint(family: atlas_session::mirror::Family, port: u16) -> atlas_session::mirror::Endpoint {
    atlas_session::mirror::Endpoint {
        family,
        port,
        incarnation: 1,
    }
}

#[tokio::test]
async fn a_client_in_the_guest_reaches_the_station_s_server_through_its_mirror() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let server = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());

    let refused = station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let heard = shout_through((std::net::Ipv4Addr::LOCALHOST, port).into(), b"hello").await;

    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(heard.unwrap(), b"HELLO");
}

#[tokio::test]
async fn an_ipv6_mirror_listens_on_ipv6_alone() {
    use atlas_session::mirror::Family::Ipv6;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let server = shouting((std::net::Ipv6Addr::LOCALHOST, 0).into()).await;
    let port = free_port(Ipv6);
    let station = relaying(controlling, desktop, [(port, server)].into());

    station.mirror(1, vec![endpoint(Ipv6, port)]).await;
    let on_v6 = shout_through((std::net::Ipv6Addr::LOCALHOST, port).into(), b"six").await;
    let on_v4 = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await;

    assert_eq!(on_v6.unwrap(), b"SIX");
    assert!(on_v4.is_err(), "IPv4 was listened on too");
}

#[tokio::test]
async fn a_port_held_in_the_guest_is_reported_and_the_others_still_mirrored() {
    use atlas_session::mirror::{Family::Ipv4, Reason};
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let server = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let held_port = held.local_addr().unwrap().port();
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());

    let refused = station
        .mirror(1, vec![endpoint(Ipv4, held_port), endpoint(Ipv4, port)])
        .await;
    let heard = shout_through((std::net::Ipv4Addr::LOCALHOST, port).into(), b"other").await;

    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].endpoint.port, held_port);
    assert_eq!(refused[0].reason, Reason::InUse);
    assert_eq!(heard.unwrap(), b"OTHER");
}

#[tokio::test]
async fn an_endpoint_no_longer_listed_is_no_longer_listened_on() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let server = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());
    station.mirror(1, vec![endpoint(Ipv4, port)]).await;

    station.mirror(2, Vec::new()).await;

    let refused = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await;
    assert!(refused.is_err(), "still listened on");
}

#[tokio::test]
async fn a_list_with_port_zero_or_a_repeat_is_refused_whole() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, std::collections::HashMap::new());

    let refused = station
        .mirror(1, vec![endpoint(Ipv4, port), endpoint(Ipv4, port)])
        .await;

    assert_eq!(refused.len(), 2, "{refused:?}");
    let listened = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await;
    assert!(listened.is_err());
}

#[tokio::test]
async fn a_mirror_lane_naming_no_waiting_connection_is_refused() {
    use atlas_session::mirror::Family::Ipv4;
    use tokio::io::AsyncReadExt;
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let opened = desktop
        .open(LaneKind::Mirror {
            connection: 99,
            endpoint: endpoint(Ipv4, 4000),
        })
        .await
        .unwrap();
    let mut stream = opened.stream;
    let mut heard = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut heard)).await;

    assert!(read.is_ok(), "the lane stayed open");
    assert!(heard.is_empty());
}

#[tokio::test]
async fn a_new_generation_binds_the_ports_its_retired_one_listened_on() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (first, desktop) = served.control().await;
    let server = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let port = free_port(Ipv4);
    let first = relaying(first, desktop, [(port, server)].into());
    first.mirror(1, vec![endpoint(Ipv4, port)]).await;

    let (second, desktop) = served.control().await;
    let second = relaying(second, desktop, [(port, server)].into());
    let refused = second.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let heard = shout_through((std::net::Ipv4Addr::LOCALHOST, port).into(), b"again").await;

    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(heard.unwrap(), b"AGAIN");
}

#[tokio::test]
async fn a_mirrored_transfer_larger_than_the_bulk_credit_goes_whole_both_ways() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let server = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());
    station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let big: Vec<u8> = (0..3 << 20).map(|at: u32| b'a' + (at % 26) as u8).collect();

    let heard = shout_through((std::net::Ipv4Addr::LOCALHOST, port).into(), &big).await;

    assert_eq!(heard.unwrap(), big.to_ascii_uppercase());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        station.controlling.credit.left(),
        atlas_session::credit::BULK as usize
    );
}

#[tokio::test]
async fn a_mirrored_connection_carries_on_through_a_stop() {
    use atlas_session::mirror::Family::Ipv4;
    use tokio::io::AsyncReadExt;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    // A server that sends back each read at once.
    let echoing = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let server = echoing.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = echoing.accept().await {
            tokio::spawn(async move {
                let (mut reading, mut writing) = stream.into_split();
                let _ = tokio::io::copy(&mut reading, &mut writing).await;
            });
        }
    });
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());
    station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let mut client = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let mut heard = [0_u8; 6];
    client.write_all(b"before").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), client.read_exact(&mut heard))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&heard, b"before");

    station
        .controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();
    // Only once the guest has taken the Stop in.
    for _ in 0..100 {
        if served.machine.said.stops.load(SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        served.machine.said.stops.load(SeqCst) > 0,
        "the Stop never came"
    );
    let mut after = [0_u8; 5];
    client.write_all(b"after").await.unwrap();
    let read = tokio::time::timeout(Duration::from_secs(10), client.read_exact(&mut after)).await;

    assert!(matches!(read, Ok(Ok(_))), "{read:?}");
    assert_eq!(&after, b"after");
}

#[tokio::test]
async fn an_endpoint_listed_again_closes_the_connections_of_its_old_incarnation() {
    use atlas_session::mirror::{Endpoint, Family::Ipv4};
    use tokio::io::AsyncReadExt;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let port = free_port(Ipv4);
    // No server: the station never opens a lane, so the connection waits.
    let station = relaying(controlling, desktop, std::collections::HashMap::new());
    station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let mut waiting = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let again = Endpoint {
        incarnation: 2,
        ..endpoint(Ipv4, port)
    };
    station.mirror(2, vec![again]).await;
    let mut heard = Vec::new();
    let closed =
        tokio::time::timeout(Duration::from_secs(5), waiting.read_to_end(&mut heard)).await;

    // Closed well before its own wait would have ended it.
    assert!(
        closed.is_ok(),
        "the old incarnation's connection stayed open"
    );
}

#[tokio::test]
async fn a_guest_with_mirrors_declares_them() {
    let served = Served::start().await;

    let (controlling, _desktop) = served.control().await;

    assert!(
        controlling
            .declared
            .capabilities
            .iter()
            .any(|has| has == atlas_session::compatibility::MIRRORS)
    );
}

#[tokio::test]
async fn one_mirrored_port_calls_another_through_the_guest() {
    use atlas_session::mirror::Family::Ipv4;
    use tokio::io::AsyncReadExt;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let inner = shouting((std::net::Ipv4Addr::LOCALHOST, 0).into()).await;
    let (outer_port, inner_port) = (free_port(Ipv4), free_port(Ipv4));
    // The outer server answers by asking the inner one, through its mirror.
    let outer = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let outer_address = outer.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = outer.accept().await {
            tokio::spawn(async move {
                let mut heard = Vec::new();
                let _ = stream.read_to_end(&mut heard).await;
                let through = (std::net::Ipv4Addr::LOCALHOST, inner_port).into();
                let answer = shout_through(through, &heard).await.unwrap_or_default();
                let _ = stream.write_all(&answer).await;
            });
        }
    });
    let targets = [(outer_port, outer_address), (inner_port, inner)].into();
    let station = relaying(controlling, desktop, targets);
    station
        .mirror(
            1,
            vec![endpoint(Ipv4, outer_port), endpoint(Ipv4, inner_port)],
        )
        .await;

    let heard = shout_through(
        (std::net::Ipv4Addr::LOCALHOST, outer_port).into(),
        b"nested",
    )
    .await;

    assert_eq!(heard.unwrap(), b"NESTED");
}

#[tokio::test]
async fn a_port_freed_in_the_guest_is_listened_on_at_the_next_retry() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = held.local_addr().unwrap().port();
    let station = relaying(controlling, desktop, std::collections::HashMap::new());
    let refused = station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    assert_eq!(refused.len(), 1);

    drop(held);

    let mut bound = false;
    for _ in 0..30 {
        let answers = station.controlling.heard.taken.lock().unwrap().clone();
        if answers
            .last()
            .is_some_and(|(_, refused)| refused.is_empty())
        {
            bound = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(bound, "never said it listened once the port was free");
}

#[tokio::test]
async fn a_link_full_of_stalled_mirrors_still_carries_a_stage_and_a_stop_and_release_at_once() {
    use atlas_session::mirror::Family::Ipv4;
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    // A server that sends without end; its clients never read.
    let flooding = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let server = flooding.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = flooding.accept().await {
            tokio::spawn(async move {
                let flood = vec![b'x'; 1 << 16];
                while stream.write_all(&flood).await.is_ok() {}
            });
        }
    });
    let port = free_port(Ipv4);
    let station = relaying(controlling, desktop, [(port, server)].into());
    station.mirror(1, vec![endpoint(Ipv4, port)]).await;
    let mut stalled = Vec::new();
    for _ in 0..atlas_session::mirror::LANES {
        stalled.push(
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                .await
                .unwrap(),
        );
    }
    // Every mirror lane live and stalled, each holding the one chunk it may:
    // what is left of the bulk credit says so.
    let credit = &station.controlling.credit;
    let held = atlas_session::mirror::LANES * atlas_session::mirror::CHUNK;
    let stalled_all = atlas_session::credit::BULK as usize - held;
    let mut saturated = false;
    for _ in 0..250 {
        if credit.left() == stalled_all {
            saturated = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        saturated,
        "the mirrors never all stalled: {} left",
        credit.left()
    );
    let (_source, tree) = tree_to_stage();
    let big: Vec<u8> = (0..4 << 20).map(|at: u32| (at % 251) as u8).collect();
    std::fs::write(tree.join("Contents/big.bin"), &big).unwrap();
    let staged = tokio::time::timeout(
        Duration::from_secs(60),
        stage(
            &station.desktop,
            credit,
            &Token::new(),
            &tree,
            Sending::Whole,
        ),
    )
    .await
    .expect("the stage never got through beside the stalled mirrors");
    assert_eq!(staged.last().unwrap()["ok"], true, "{staged:?}");
    assert_eq!(
        credit.left(),
        stalled_all,
        "the mirrors let go while the stage ran"
    );
    // Now a narrow link, with a stage filling what credit is left of it.
    let rate: u64 = 1 << 20;
    served.link.set(Conditions {
        bytes_per_second: Some(rate),
        ..Conditions::default()
    });
    let again = {
        let (desktop, credit, tree) = (station.desktop.clone(), credit.clone(), tree.clone());
        tokio::spawn(
            async move { stage(&desktop, &credit, &Token::new(), &tree, Sending::Whole).await },
        )
    };
    for _ in 0..250 {
        if credit.left() < stalled_all {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(credit.left() < stalled_all, "the stage never began sending");
    let said = |what: atlas_session::control::Said| station.controlling.outbox.say(what).unwrap();
    let begun = std::time::Instant::now();
    said(atlas_session::control::Said::Stop);
    said(atlas_session::control::Said::Release);
    // At worst behind every byte the credit lets be on its way, at the
    // link's rate, with room for the rest.
    let bound = Duration::from_secs_f64(f64::from(atlas_session::credit::BULK) / rate as f64 + 1.0);
    let mut heard = false;
    while begun.elapsed() < bound {
        let machine = &served.machine.said;
        if machine.stops.load(SeqCst) > 0 && machine.releases.load(SeqCst) > 0 {
            heard = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert!(
        heard,
        "Stop and release waited past {bound:?} behind bulk traffic"
    );
    again.abort();
    drop(stalled);
}

/// Start a background job running `argv` for `lifetime_ms` as `change`:
/// the guest's last reply.
async fn job_start(desktop: &Desktop, change: &Token, argv: &[&str], lifetime_ms: u64) -> Value {
    let request = json!({
        "op": "job", "action": "start", "argv": argv, "lifetimeMs": lifetime_ms,
        "uid": nix::unistd::getuid().as_raw(), "change": change,
    });
    let said = ask(
        desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &request,
    )
    .await;
    said.last().cloned().unwrap_or(Value::Null)
}

/// What the guest shows of job `id`.
async fn job_shown(desktop: &Desktop, id: &str) -> Value {
    let said = ask(
        desktop,
        LaneKind::Read,
        &json!({"op": "job", "action": "show", "id": id}),
    )
    .await;
    said.last().unwrap()["job"].clone()
}

/// Whether the group `pgid` leads ends within `within`.
async fn group_ends(pgid: i32, within: Duration) -> bool {
    let group = nix::unistd::Pid::from_raw(pgid);
    tokio::time::timeout(within, async {
        while nix::sys::signal::killpg(group, None).is_ok() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_ok()
}

/// Job `id`'s state once it says it has ended, within `within`.
async fn job_ended(desktop: &Desktop, id: &str, within: Duration) -> Value {
    let started = std::time::Instant::now();
    loop {
        let shown = job_shown(desktop, id).await;
        if shown["state"] == "ended" || started.elapsed() > within {
            return shown;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_job_runs_past_its_lane_and_its_log_holds_what_it_said() {
    use base64::Engine;
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let started = job_start(
        &desktop,
        &Token::new(),
        &["/bin/sh", "-c", "echo said; exec sleep 30"],
        60_000,
    )
    .await;

    let job = &started["job"];
    assert_eq!(job["state"], "running", "{started}");
    assert_eq!(job["pid"], job["pgid"]);
    let id = job["id"].as_str().unwrap();
    let mut logged = String::new();
    for _ in 0..50 {
        let said = ask(
            &desktop,
            LaneKind::Read,
            &json!({"op": "job", "action": "log", "id": id, "bytes": 4096}),
        )
        .await;
        logged = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(said.last().unwrap()["data"].as_str().unwrap_or_default())
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        if logged.contains("said") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(logged.contains("said"), "{logged:?}");
    assert_eq!(job_shown(&desktop, id).await["state"], "running");
    assert!(
        nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(job["pgid"].as_i64().unwrap() as i32),
            None
        )
        .is_ok()
    );
}

#[tokio::test]
async fn a_job_stopped_by_its_identity_ends_and_one_stopped_by_another_runs_on() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await;
    let job = started["job"].clone();
    let id = job["id"].as_str().unwrap();
    let expect =
        json!({"boot": job["boot"], "pid": job["pid"], "start": job["start"], "pgid": job["pgid"]});
    let mut wrong = expect.clone();
    wrong["pid"] = json!(1);
    let stop = |expect: &Value| {
        let change = Token::new();
        let request = json!({"op": "job", "action": "stop", "id": id, "expect": expect, "graceMs": 2000, "change": change});
        (change, request)
    };

    let (change, request) = stop(&wrong);
    let refused = ask(&desktop, LaneKind::Change { change }, &request).await;
    let (change, request) = stop(&expect);
    let stopped = ask(&desktop, LaneKind::Change { change }, &request).await;

    assert_eq!(refused.last().unwrap()["reason"], "identity", "{refused:?}");
    assert_eq!(
        stopped.last().unwrap()["signalled"],
        "SIGTERM",
        "{stopped:?}"
    );
    let ended = job_ended(&desktop, id, Duration::from_secs(10)).await;
    assert_eq!(ended["state"], "ended", "{ended}");
    assert_eq!(ended["how"], "stopped", "{ended}");
}

#[tokio::test]
async fn a_job_s_lifetime_ends_it() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 1_500).await;
    let pgid = started["job"]["pgid"].as_i64().unwrap() as i32;

    assert!(group_ends(pgid, Duration::from_secs(10)).await, "{started}");
    let ended = job_ended(
        &desktop,
        started["job"]["id"].as_str().unwrap(),
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(ended["how"], "timeout", "{ended}");
}

#[tokio::test]
async fn a_job_that_exits_by_itself_is_shown_with_its_code() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let started = job_start(
        &desktop,
        &Token::new(),
        &["/bin/sh", "-c", "sleep 1; exit 7"],
        60_000,
    )
    .await;

    let ended = job_ended(
        &desktop,
        started["job"]["id"].as_str().unwrap(),
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(ended["how"], "exited", "{ended}");
    assert_eq!(ended["exitCode"], 7, "{ended}");
}

/// A job pinned to the session Atlas verified starts only in that session:
/// one pinned elsewhere is refused before its program runs.
#[tokio::test]
async fn a_job_pinned_to_another_session_is_refused_before_it_runs() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let pinned = |session: Value| {
        let change = Token::new();
        let request = json!({
            "op": "job", "action": "start", "argv": ["/bin/sleep", "30"], "lifetimeMs": 60_000,
            "uid": nix::unistd::getuid().as_raw(), "change": change, "session": session,
        });
        (change, request)
    };

    let (change, elsewhere) = pinned(json!({"securitySessionId": 43, "displayId": 7}));
    let said = ask(&desktop, LaneKind::Change { change }, &elsewhere).await;
    assert_eq!(said.last().unwrap()["code"], "session-changed", "{said:?}");
    let shown = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "job", "action": "show"}),
    )
    .await;
    assert_eq!(
        shown.last().unwrap()["jobs"],
        json!([]),
        "nothing ran: {shown:?}"
    );

    let (change, here) = pinned(json!({"securitySessionId": 42, "displayId": 7}));
    let said = ask(&desktop, LaneKind::Change { change }, &here).await;
    assert_eq!(said.last().unwrap()["ok"], true, "{said:?}");
}

#[tokio::test]
async fn a_stop_ends_every_job() {
    let served = Served::start().await;
    let (controlling, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await;
    let pgid = started["job"]["pgid"].as_i64().unwrap() as i32;

    controlling
        .outbox
        .say(atlas_session::control::Said::Stop)
        .unwrap();

    assert!(group_ends(pgid, Duration::from_secs(10)).await);
    let id = started["job"]["id"].as_str().unwrap();
    let ended = job_ended(&desktop, id, Duration::from_secs(10)).await;
    assert_eq!(ended["how"], "person", "a person's Stop: {ended}");
}

#[tokio::test]
async fn a_new_generation_ends_the_jobs_of_the_one_before() {
    let served = Served::start().await;
    let (_first, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await;
    let pgid = started["job"]["pgid"].as_i64().unwrap() as i32;

    let (_second, second) = served.control().await;

    assert!(group_ends(pgid, Duration::from_secs(10)).await);
    let id = started["job"]["id"].as_str().unwrap();
    let ended = job_ended(&second, id, Duration::from_secs(10)).await;
    assert_eq!(
        ended["how"], "gone",
        "its generation went, no one stopped it: {ended}"
    );
}

#[tokio::test]
async fn a_job_running_through_a_silent_partition_ends_once_its_lease_lapses_and_its_stop_is_saved()
{
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await;
    let pgid = started["job"]["pgid"].as_i64().unwrap() as i32;
    served.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });

    // No request, heartbeat or check: only time passes.
    served.clock.advance(atlas_session::authority::LEASE * 2);

    assert!(
        group_ends(pgid, Duration::from_secs(10)).await,
        "the job ran on"
    );
    assert!(matches!(
        served.machine.paired.stop.kept().unwrap(),
        crate::machine::stop::Kept::Stopped { .. }
    ));
}

#[tokio::test]
async fn a_job_start_asked_again_is_answered_from_its_record_and_starts_nothing() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();
    let first = job_start(&desktop, &change, &["/bin/sleep", "30"], 60_000).await;

    let again = job_start(&desktop, &change, &["/bin/sleep", "30"], 60_000).await;

    assert_eq!(again, first);
    let every = ask(
        &desktop,
        LaneKind::Read,
        &json!({"op": "job", "action": "show"}),
    )
    .await;
    let jobs = every.last().unwrap()["jobs"].as_array().unwrap().len();
    assert_eq!(jobs, 1, "{every:?}");
}

#[tokio::test]
async fn stopping_every_job_at_once_is_refused_on_a_machine() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let change = Token::new();

    let said = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "job", "action": "stop-all", "reason": "person", "change": change}),
    )
    .await;

    assert_eq!(said.last().unwrap()["code"], "platform", "{said:?}");
}

#[tokio::test]
async fn a_guest_that_keeps_jobs_declares_them() {
    let served = Served::start().await;

    let (controlling, _desktop) = served.control().await;

    assert!(
        controlling
            .declared
            .capabilities
            .iter()
            .any(|has| has == atlas_session::compatibility::JOBS)
    );
}

#[tokio::test]
async fn a_job_whose_supervisor_is_killed_from_outside_is_unknown_never_ended() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;
    let started = job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await;
    let supervisor = started["job"]["pid"].as_i64().unwrap() as i32;
    let id = started["job"]["id"].as_str().unwrap().to_owned();

    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(supervisor),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();

    let mut shown = Value::Null;
    for _ in 0..100 {
        shown = job_shown(&desktop, &id).await;
        if shown["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(shown["state"], "unknown", "{shown}");
    // What it left is not proved gone: the program, outside its supervisor.
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(supervisor),
        nix::sys::signal::Signal::SIGKILL,
    );
}

#[tokio::test]
async fn more_than_eight_jobs_run_at_once() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let mut started = Vec::new();
    for _ in 0..10 {
        started.push(job_start(&desktop, &Token::new(), &["/bin/sleep", "30"], 60_000).await);
    }

    for (at, job) in started.iter().enumerate() {
        assert_eq!(job["job"]["state"], "running", "job {at}: {job}");
    }
}

/// A station's pairing record, in memory.
struct StationKept(std::sync::Mutex<Option<atlas_session::pairing::Enrolment>>);

impl atlas_session::pairing::StationRecords for StationKept {
    fn read(&self) -> Result<Option<atlas_session::pairing::Enrolment>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn replace(
        &self,
        _expected: Option<&atlas_session::pairing::Enrolment>,
        record: &atlas_session::pairing::Enrolment,
    ) -> Result<()> {
        *self.0.lock().unwrap() = Some(record.clone());
        Ok(())
    }
}

/// The station finishing enrolment `change`, pending with `served`'s guest,
/// on a pairing connection.
async fn finishing(
    served: &Served,
    change: &Token,
) -> (
    Result<atlas_core::session::paired::Paired>,
    Arc<dyn atlas_session::pairing::StationRecords>,
) {
    let pending = atlas_session::pairing::Enrolment::Pending {
        change: change.clone(),
        guest: served.guest_key,
    };
    let records: Arc<dyn atlas_session::pairing::StationRecords> =
        Arc::new(StationKept(std::sync::Mutex::new(Some(pending))));
    let (station_end, guest_end, _cut) = served.link.connect();
    served
        .connections
        .send(framed(Purpose::Pairing, Box::new(guest_end)))
        .unwrap();
    let finished = atlas_session::pairing::station_completes(
        station_end,
        &served.station,
        &served.desktop,
        change,
        &records,
    )
    .await;
    (finished, records)
}

#[tokio::test]
async fn a_station_whose_pairing_ack_was_lost_finishes_the_committed_pairing_and_no_other() {
    let served = Served::start().await;
    let enrolment = served.machine.paired.enrolment.clone();
    served
        .machine
        .paired
        .pairing
        .commit(served.station.public(), &enrolment)
        .unwrap();

    let (finished, records) = finishing(&served, &enrolment).await;
    assert!(finished.is_ok(), "{finished:?}");
    assert!(matches!(
        records.read().unwrap(),
        Some(atlas_session::pairing::Enrolment::Paired(_))
    ));

    let (other, _) = finishing(&served, &Token::new()).await;
    assert!(other.is_err(), "another enrolment finished: {other:?}");
}

#[tokio::test]
async fn a_guest_not_yet_seen_closed_answers_the_stations_reads_and_refuses_the_rest() {
    let served = Served::start().await;
    served
        .machine
        .seen_as(crate::daemon::facts::Posture::default());
    let (_controlling, desktop) = served.control().await;

    let read = ask(&desktop, LaneKind::Read, &QUICK_READ).await;
    assert_eq!(read.last().unwrap()["ok"], true, "{read:?}");
    let change = Token::new();
    let refused = ask(
        &desktop,
        LaneKind::Change {
            change: change.clone(),
        },
        &json!({"op": "run", "argv": ["/usr/bin/true"], "change": change}),
    )
    .await;
    assert!(refused.is_empty(), "a change ran: {refused:?}");
}

#[tokio::test]
async fn a_machine_says_hello_as_itself_never_as_the_hidden_login() {
    let served = Served::start().await;
    let (_controlling, desktop) = served.control().await;

    let said = ask(&desktop, LaneKind::Read, &json!({"op": "hello"})).await;

    let hello = said.last().unwrap();
    assert_eq!(hello["ok"], true, "{hello}");
    assert_eq!(hello["daemon"]["pid"], std::process::id(), "{hello}");
    let capabilities = hello["daemon"]["capabilities"].as_array().unwrap();
    assert!(
        capabilities.contains(&json!(atlas_session::compatibility::CHANGES)),
        "{hello}"
    );
    for helper_only in ["read", "rgb-stream", "helper-sound"] {
        assert!(!capabilities.contains(&json!(helper_only)), "{hello}");
    }
    assert!(hello.get("permissions").is_none(), "{hello}");
}

#[tokio::test]
async fn a_connection_that_never_says_what_it_is_for_holds_up_no_other_and_one_that_fails_ends_nothing()
 {
    let served = Served::start().await;
    served
        .connections
        .send(Box::pin(std::future::pending()))
        .unwrap();
    served
        .connections
        .send(Box::pin(std::future::ready(Err(
            io::ErrorKind::InvalidData.into(),
        ))))
        .unwrap();

    let (_controlling, desktop) = served.control().await;

    let read = ask(&desktop, LaneKind::Read, &QUICK_READ).await;
    assert_eq!(read.last().unwrap()["ok"], true, "{read:?}");
}

/// A station key kept in a file, for the live check below to reconnect with.
struct KeyFile(std::path::PathBuf);

impl atlas_session::keys::KeyStore for KeyFile {
    fn load(&self) -> Result<Option<atlas_session::keys::SecretKey>> {
        match std::fs::read(&self.0) {
            Ok(bytes) => Ok(<[u8; 32]>::try_from(bytes.as_slice())
                .ok()
                .map(atlas_session::keys::SecretKey::from_bytes)),
            Err(_) => Ok(None),
        }
    }

    fn save(&self, secret: &atlas_session::keys::SecretKey) -> Result<()> {
        atlas_core::files::write_private(&self.0, secret.as_bytes())
    }
}

/// Live, against a real guest through Atlas VM: pair over its station
/// socket with the secret its install handed over, unless paired already,
/// then go live and hear the guest's own hello over an authenticated lane.
/// Run by hand, `--ignored --nocapture`, with `ATLAS_LIVE_STATION` (the
/// station socket), `ATLAS_LIVE_SECRET` (hex), `ATLAS_LIVE_DESKTOP`,
/// `ATLAS_LIVE_ENROLMENT` and `ATLAS_LIVE_STATE` (a folder for the station's
/// key and the guest's, kept between runs); `ATLAS_LIVE_RUN` to run an
/// agent's change there too.
#[tokio::test]
#[ignore = "live: needs a real guest through Atlas VM"]
async fn live_a_station_pairs_with_a_real_guest_and_hears_its_own_hello() {
    let variable = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let socket = std::path::PathBuf::from(variable("ATLAS_LIVE_STATION"));
    let desktop = DesktopId::parse(&variable("ATLAS_LIVE_DESKTOP")).unwrap();
    let enrolment = Token::parse(&variable("ATLAS_LIVE_ENROLMENT")).unwrap();
    let state = std::path::PathBuf::from(variable("ATLAS_LIVE_STATE"));
    std::fs::create_dir_all(&state).unwrap();
    let keys = Arc::new(atlas_session::keys::own_key(&KeyFile(state.join("station.key"))).unwrap());
    let guest_file = state.join("guest.key");
    let guest = match std::fs::read_to_string(&guest_file) {
        Ok(hex) => PeerKey::from_hex(hex.trim()).unwrap(),
        Err(_) => {
            let secret = PeerKey::from_hex(&variable("ATLAS_LIVE_SECRET")).unwrap();
            let secret = atlas_session::keys::PairingSecret::from_bytes(*secret.as_bytes());
            let records: Arc<dyn atlas_session::pairing::StationRecords> =
                Arc::new(StationKept(std::sync::Mutex::new(None)));
            let paired = atlas_session::pairing::station_pairs(
                reach(&socket, Purpose::Pairing).await,
                &keys,
                &secret,
                &desktop,
                &enrolment,
                &records,
            )
            .await
            .unwrap();
            std::fs::write(&guest_file, hex(paired.guest.as_bytes())).unwrap();
            println!("paired: guest uid {}, boot {}", paired.uid, paired.boot);
            paired.guest
        }
    };
    let controlling = station_control(
        reach(&socket, Purpose::Control).await,
        &keys,
        guest,
        &desktop,
    )
    .await
    .unwrap();
    let peers = Peers {
        desktop: desktop.clone(),
        own: keys.clone(),
        key: guest,
    };
    let lanes = Desktop::new(
        peers,
        controlling.generation,
        controlling.outbox.clone(),
        Box::new(SocketLanes(socket)),
    );

    let said = ask(&lanes, LaneKind::Read, &json!({"op": "hello"})).await;

    let hello = said.last().unwrap();
    println!("hello: {hello}");
    assert_eq!(hello["ok"], true, "{hello}");
    // With `ATLAS_LIVE_RUN` set, once admitted and resumed (a Resume waits
    // for the guest's input service to be ready), an agent's change runs in
    // the guest as its account.
    if std::env::var_os("ATLAS_LIVE_RUN").is_some() {
        assert_eq!(hello["agents"]["admitted"], true, "{hello}");
        assert!(resumed(&controlling, &lanes).await, "agents stayed stopped");
        let change = Token::new();
        let ran = ask(
            &lanes,
            LaneKind::Change {
                change: change.clone(),
            },
            &json!({"op": "run", "argv": ["/usr/bin/id", "-un"], "change": change}),
        )
        .await;
        println!("ran: {ran:?}");
        assert_eq!(ran.last().unwrap()["ok"], true, "{ran:?}");
    }
}
