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
    _serving: tokio::task::JoinHandle<Result<()>>,
}

impl Served {
    async fn start() -> Self {
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
        };
        let machine = Machine::new(Arc::new(daemon), paired, clock.clone());
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
/// half, beside the socket.
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
    let [socket, station] = spec.split(' ').collect::<Vec<_>>()[..] else {
        panic!("not a guest's spec: {spec}");
    };
    Console::Own.keep().unwrap();
    let keys = Keypair::generate().unwrap();
    let public = std::path::Path::new(socket).with_extension("pub");
    std::fs::write(public, hex(keys.public().as_bytes())).unwrap();
    let mut daemon = Daemon::new("Aqua".into(), synthetic, || true, own_session);
    daemon.screen.console = Console::Own;
    let paired = Paired {
        keys,
        desktop: atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id(),
        station: PeerKey::from_hex(station).unwrap(),
        generations: Arc::new(Counted::default()),
    };
    let listener = tokio::net::UnixListener::bind(socket).unwrap();
    let machine = Machine::new(
        Arc::new(daemon),
        paired,
        Arc::new(atlas_session::clock::Continuous),
    );
    let _ = machine.serve(&Socket(listener)).await;
}

#[tokio::test]
async fn a_guest_in_a_process_of_its_own_serves_its_station_and_is_not_the_hidden_login() {
    if let Ok(spec) = std::env::var(GUEST) {
        return be_the_guest(&spec).await;
    }
    let folder = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let socket = folder.path().join("guest.sock");
    let station = Arc::new(Keypair::generate().unwrap());
    let spec = format!("{} {}", socket.display(), hex(station.public().as_bytes()));
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "daemon::serve::machine::tests::a_guest_in_a_process_of_its_own_serves_its_station_and_is_not_the_hidden_login",
            "--test-threads=1",
        ])
        .env(GUEST, spec)
        .kill_on_drop(true);
    let mut child = atlas_core::spawn::spawn_async(&mut command).unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while !socket.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the guest listens");
    let guest =
        PeerKey::from_hex(&std::fs::read_to_string(socket.with_extension("pub")).unwrap()).unwrap();
    let child_pid = child.id().unwrap();
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
    let replies = ask(&lanes, LaneKind::Read, &json!({"op": "hello"})).await;
    let hello = replies.last().unwrap().clone();
    child.kill().await.unwrap();

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
