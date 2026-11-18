//! Looks at a VM end to end, through a real keeper and a real client: its
//! screenshots, the files collected from it and the picture before an
//! agent's input, each taken between two proofs, its guest a stand-in that
//! answers as a machine guest does, its file reads by the guest's own code.
use crate::{
    agent,
    client::{Client, Size},
    doing::Doing,
    vm::keeper::{
        change::tests::{admitted, live, proven},
        clients::tests::Serving,
        tests::{Saying, atlas_vm_state, input_hello, screen, until},
    },
};
use atlas_core::{
    actions::Button,
    error::Code,
    protocol::{Crop, Part, Region},
};
use atlas_session::lane::{Header, LaneKind};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

/// What the guest's daemon says Screen Recording waits for, in its words.
const CAPTURE_HELD: &str = "Allow Atlas in Privacy & Security, Screen & System Audio Recording, \
                            in the VM's window: agents' pictures wait for it.";

/// The guest's one app on screen: Notes, its window number 42.
const NOTES: i64 = 812;
const WINDOW: i64 = 42;

/// The bounds of Notes's window, in the guest's display points.
fn bounds() -> Value {
    json!({"x": 10, "y": 20, "width": 80, "height": 60})
}

/// The guest's windows, as its daemon lists them.
fn windows() -> Value {
    json!({"ok": true, "starts": {NOTES.to_string(): [123, 456]}, "windows": [{"number": WINDOW,
        "pid": NOTES, "owner": "Notes", "title": "Draft", "layer": 0, "alpha": 1.0,
        "bounds": bounds()}]})
}

/// The guest's apps, as its daemon lists them.
fn apps() -> Value {
    json!({"ok": true, "apps": [{"pid": NOTES, "name": "Notes"}],
        "starts": {NOTES.to_string(): [123, 456]}})
}

/// The head of a PNG `width` × `height` pixels: all a size is read from.
fn png(width: u64, height: u64) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(u32::try_from(width).unwrap().to_be_bytes());
    png.extend(u32::try_from(height).unwrap().to_be_bytes());
    png.resize(33, 0);
    png
}

/// A screenshot of the guest's 1440 × 900 point display, 2880 × 1800
/// pixels, as its daemon answers `request`: the whole display no larger
/// than asked, or the crop asked for at a pixel a point, with what it
/// shows.
fn shot(request: &Value) -> Value {
    let side = request["maxDimension"].as_u64().unwrap_or(2880).min(2880);
    let region = if request["region"].is_object() {
        Some(request["region"].clone())
    } else if request["pid"] == NOTES {
        Some(match &request["within"] {
            Value::Null => bounds(),
            within => {
                let edge = |key: &str| within[key].as_i64().unwrap();
                let (x, y) = (edge("x").max(10), edge("y").max(20));
                let right = (edge("x") + edge("width")).min(90);
                let bottom = (edge("y") + edge("height")).min(80);
                json!({"x": x, "y": y, "width": right - x, "height": bottom - y})
            }
        })
    } else {
        None
    };
    let (width, height) = region.as_ref().map_or((side, side * 900 / 1440), |region| {
        (
            region["width"].as_u64().unwrap(),
            region["height"].as_u64().unwrap(),
        )
    });
    let mut reply = json!({"ok": true, "png": STANDARD.encode(png(width, height)),
        "width": width, "height": height, "scale": 2.0, "offDisplayWindows": []});
    if let Some(region) = region {
        reply["region"] = region;
    }
    if request["pid"] == NOTES {
        reply["app"] = json!({"pid": NOTES, "name": "Notes", "bundleId": "com.apple.Notes"});
        reply["overlapping"] = json!([]);
    }
    reply
}

/// A frame of the guest's screen, `fill` in every pixel, as its daemon
/// frames one: its header's length, its header, then its pixels.
fn framed(fill: u8) -> Vec<u8> {
    let (width, height) = (64, 40);
    let header = serde_json::to_vec(&json!({"ok": true, "sequence": 0, "width": width,
        "height": height, "scale": 2.0, "session": screen(), "daemonPID": 123}))
    .unwrap();
    let mut bytes = u32::try_from(header.len()).unwrap().to_be_bytes().to_vec();
    bytes.extend(header);
    bytes.extend(vec![fill; width * height * 3]);
    bytes
}

/// What a test runs as the guest takes a screenshot, between the
/// command's two proofs.
type Change = Box<dyn Fn() + Send + Sync>;
type Between = Arc<Mutex<Option<Change>>>;

/// What changes a VM between a screenshot's proofs, as a test makes it.
type Changing = fn(&Looking) -> Change;

/// A VM a test looks at: its keeper serving a guest that proves itself,
/// lists Notes's window, answers screenshots and takes input; an agent's
/// client of it; every screenshot it was asked for; and what happened in
/// order on its input lanes and picture lanes.
struct Looking {
    serving: Serving,
    saying: Arc<Saying>,
    client: Client,
    directory: tempfile::TempDir,
    asked: Arc<Mutex<Vec<Value>>>,
    between: Between,
    happened: Arc<Mutex<Vec<&'static str>>>,
    /// How the guest's input service answers an input: none performs it.
    refusing: Arc<Mutex<Option<Value>>>,
}

async fn looking(doing: &'static str) -> Looking {
    let (serving, saying, client, directory) = proven().await;
    let client = client.doing(Doing::new(doing));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let between: Between = Arc::default();
    let happened = Arc::new(Mutex::new(Vec::new()));
    let refusing = Arc::new(Mutex::new(None));
    let (said, hook, order, refused) = (
        asked.clone(),
        between.clone(),
        happened.clone(),
        refusing.clone(),
    );
    *saying.otherwise.lock().unwrap() =
        Some(Arc::new(move |header: &Header, request: Value| {
            match (&header.kind, request["op"].as_str()) {
                (LaneKind::Read, Some("windows")) => windows(),
                (LaneKind::Read, Some("apps")) => apps(),
                (LaneKind::Picture, Some("screenshot")) => {
                    said.lock().unwrap().push(request.clone());
                    if let Some(between) = &*hook.lock().unwrap() {
                        between();
                    }
                    shot(&request)
                }
                (LaneKind::Input { change: Some(_) }, Some("acquire")) => json!({"ok": true}),
                (LaneKind::Input { change: Some(_) }, Some("input")) => {
                    order.lock().unwrap().push("input");
                    refused.lock().unwrap().clone().unwrap_or_else(|| {
                        json!({"ok": true, "app": null, "bundleId": null,
                        "performed": request["actions"].as_array().map_or(0, Vec::len)})
                    })
                }
                (LaneKind::Input { change: Some(_) }, Some("release")) => {
                    json!({"ok": true, "released": true})
                }
                _ => json!({"ok": true, "echo": request}),
            }
        }));
    Looking {
        serving,
        saying,
        client,
        directory,
        asked,
        between,
        happened,
        refusing,
    }
}

impl Looking {
    /// Run `between` as the guest takes each screenshot from now.
    fn between(&self, between: impl Fn() + Send + Sync + 'static) {
        *self.between.lock().unwrap() = Some(Box::new(between));
    }

    /// How many times the guest was proved.
    fn proofs(&self) -> usize {
        self.saying.challenges.lock().unwrap().len()
    }

    /// The picture lanes the guest admitted.
    fn picture_lanes(&self) -> usize {
        admitted(&self.serving)
            .iter()
            .filter(|header| header.kind == LaneKind::Picture)
            .count()
    }

    /// Every screenshot kept in the VM's records.
    fn captures(&self) -> Vec<PathBuf> {
        std::fs::read_dir(self.client.records())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.to_string_lossy().ends_with("-capture.png"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Hold agents by the station's Stop, or the guest's own.
    fn stopped(&self, by_station: bool) {
        if by_station {
            self.client.controller.note_stop(true).unwrap();
        } else {
            *self.serving.fixture.guest.stop.lock().unwrap() = "stopped";
        }
    }

    /// Answer each frame the guest is asked for with `fill` in every pixel,
    /// noting that it was asked; none leaves it stalled.
    fn framing(&self, fill: Option<u8>) {
        let order = self.happened.clone();
        self.serving
            .fixture
            .guest
            .standin
            .pictures
            .set(Arc::new(move |request| {
                if request["op"] != "frame" {
                    let mut line = serde_json::to_vec(&shot(request)).unwrap();
                    line.push(b'\n');
                    return Some(line);
                }
                order.lock().unwrap().push("frame");
                fill.map(framed)
            }));
    }

    /// The pictures before actions kept in the VM's records, once every
    /// one being saved is.
    fn pictures(&self) -> Vec<PathBuf> {
        crate::before::settle(Duration::from_secs(10));
        std::fs::read_dir(self.client.records().join(crate::before::FOLDER))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_none_or(|extension| extension != "json"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_screenshot_goes_on_a_picture_lane_between_two_proofs_and_is_kept_in_its_records() {
    let looking = looking("screenshot").await;
    let generation = live(&looking.serving);

    let saved = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap();

    assert_eq!(saved["ok"], true, "{saved}");
    let path = Path::new(saved["path"].as_str().unwrap());
    assert!(path.starts_with(looking.client.records()), "{saved}");
    assert!(path.starts_with(looking.directory.path()), "{saved}");
    assert_eq!(std::fs::read(path).unwrap(), png(1600, 1000));
    let asked = looking.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0]["session"]["securitySessionId"], 42);
    assert_eq!(asked[0]["maxDimension"], 1600);
    assert!(
        admitted(&looking.serving).iter().any(|header| {
            header.kind == LaneKind::Picture && header.generation.0 == generation
        })
    );
    assert_eq!(
        looking.saying.challenges.lock().unwrap().len(),
        2,
        "proved before and after"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_screenshot_is_at_most_1600_pixels_its_shape_and_its_points_kept() {
    let looking = looking("screenshot").await;

    let larger = looking
        .client
        .capture(Size::Longest(4000), None, true)
        .await
        .unwrap();
    let standard = looking
        .client
        .capture(Size::Standard, None, true)
        .await
        .unwrap();

    for saved in [&larger, &standard] {
        let (width, height) = (
            saved["width"].as_f64().unwrap(),
            saved["height"].as_f64().unwrap(),
        );
        let per = saved["pointsPerPixel"].as_f64().unwrap();
        assert!(width <= 1600.0, "{saved}");
        assert!((width * per - 1440.0).abs() < 1e-9, "{saved}");
        assert!((height * per - 900.0).abs() <= per, "{saved}");
        assert_eq!(saved["origin"], json!({"x": 0.0, "y": 0.0}));
    }
    assert_eq!(larger["width"], 1600);
    assert_eq!(larger["height"], 1000);
    let asked = looking.asked.lock().unwrap().clone();
    assert_eq!(asked[0]["maxDimension"], 1600, "capped before it was asked");
    assert_eq!(asked[1]["maxDimension"], 1456);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_picture_larger_than_its_cap_or_of_another_shape_is_refused_and_nothing_kept() {
    let looking = looking("screenshot").await;
    let wrong = Arc::new(Mutex::new((1700, 1062)));
    let answering = wrong.clone();
    *looking.saying.otherwise.lock().unwrap() = Some(Arc::new(move |header: &Header, request| {
        if header.kind != LaneKind::Picture {
            return json!({"ok": true, "echo": request});
        }
        let (width, height) = *answering.lock().unwrap();
        json!({"ok": true, "png": STANDARD.encode(png(width, height)), "width": width,
            "height": height, "scale": 2.0, "offDisplayWindows": []})
    }));

    let larger = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();
    *wrong.lock().unwrap() = (1600, 1200);
    let misshapen = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();

    assert_eq!(larger.code, Code::Protocol, "{larger:?}");
    assert_eq!(misshapen.code, Code::Protocol, "{misshapen:?}");
    assert!(looking.captures().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_screenshot_of_the_display_an_app_a_window_or_an_apps_part_is_taken_under_each_stop() {
    for by_station in [true, false] {
        let looking = looking("screenshot").await;
        looking.stopped(by_station);
        let client = &looking.client;
        let notes = agent::Target::Name("Notes".into());
        let within = Region {
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        };

        let display = agent::screenshot(client, None, None, None, Size::Standard, true)
            .await
            .unwrap();
        let app = agent::screenshot(client, Some(&notes), None, None, Size::Standard, true)
            .await
            .unwrap();
        let window = agent::screenshot(client, None, None, Some(WINDOW), Size::Standard, true)
            .await
            .unwrap();
        let part = agent::screenshot(
            client,
            Some(&notes),
            Some(within),
            None,
            Size::Standard,
            true,
        )
        .await
        .unwrap();
        let region = agent::screenshot(client, None, Some(within), None, Size::Standard, true)
            .await
            .unwrap();

        for saved in [&display, &app, &window, &part, &region] {
            assert_eq!(saved["ok"], true, "{by_station}: {saved}");
        }
        assert_eq!(app["region"], bounds(), "{app}");
        assert_eq!(app["app"]["name"], "Notes");
        assert_eq!(window["app"]["pid"], NOTES);
        assert_eq!(
            part["region"],
            json!({"x": 10, "y": 20, "width": 40, "height": 30})
        );
        assert_eq!(region["origin"], json!({"x": 0.0, "y": 0.0}));
        assert_eq!(region["pointsPerPixel"], 1.0);
        let asked = looking.asked.lock().unwrap().clone();
        assert_eq!(asked[1]["pid"], NOTES);
        assert_eq!(asked[2]["window"], WINDOW, "{:?}", asked[2]);
        assert_eq!(asked[3]["within"], json!(within));
        assert!(asked.iter().all(|asked| asked["maxDimension"] == 1456));
        assert_eq!(looking.captures().len(), 5);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_crop_of_an_app_or_a_window_is_its_guests_own_and_not_the_helpers_parts() {
    let looking = looking("screenshot").await;
    assert!(!atlas_session::compatibility::MACHINE.contains(&atlas_core::protocol::APP_PARTS));

    let window = looking
        .client
        .capture(
            Size::Standard,
            Some(&Crop::Part {
                pid: NOTES,
                part: Part::Window(WINDOW),
            }),
            false,
        )
        .await
        .unwrap();

    assert_eq!(window["region"], bounds(), "{window}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_screenshot_whose_proofs_differ_is_refused_as_the_session_changed_and_nothing_is_kept() {
    let changes: [(&str, Changing); 3] = [
        ("the fence", |looking| {
            let guest = looking.serving.fixture.guest.clone();
            Box::new(move || *guest.runner.lock().unwrap() = atlas_vm_state("closed", 1))
        }),
        ("Atlas VM's run", |looking| {
            let guest = looking.serving.fixture.guest.clone();
            Box::new(move || {
                let mut state = atlas_vm_state("closed", 0);
                state["incarnation"]["id"] = json!("another-run");
                *guest.runner.lock().unwrap() = state;
            })
        }),
        ("the epoch", |looking| {
            let saying = looking.saying.clone();
            Box::new(move || {
                let mut input = input_hello();
                input["pid"] = json!(457);
                *saying.input.lock().unwrap() = Some(input);
            })
        }),
    ];
    for (what, change) in changes {
        let looking = looking("screenshot").await;
        looking.between(change(&looking));

        let refused = looking
            .client
            .capture(Size::Full, None, false)
            .await
            .unwrap_err();

        assert_eq!(refused.code, Code::SessionChanged, "{what}: {refused:?}");
        assert!(!refused.error.contains("helper"), "{what}: {refused:?}");
        assert_eq!(looking.asked.lock().unwrap().len(), 1, "{what}: taken");
        assert!(looking.captures().is_empty(), "{what}: nothing kept");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_opened_and_closed_again_while_a_screenshot_is_taken_refuses_it() {
    let looking = looking("screenshot").await;
    let guest = looking.serving.fixture.guest.clone();
    // Closed at the first proof; opened, the fence moved on, while the guest
    // captured; closed again before the second.
    looking.between(move || {
        *guest.runner.lock().unwrap() = atlas_vm_state("open", 1);
        *guest.runner.lock().unwrap() = atlas_vm_state("closed", 1);
    });
    let refused = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();
    let guest = looking.serving.fixture.guest.clone();
    looking.between(move || *guest.runner.lock().unwrap() = atlas_vm_state("open", 2));
    let open = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::SessionChanged, "{refused:?}");
    assert_eq!(
        open.code,
        Code::OnConsole,
        "still open at the second: {open:?}"
    );
    assert!(looking.captures().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_after_a_picture_must_show_the_same_epoch_generation_run_and_fence() {
    let looking = looking("screenshot").await;
    let client = &looking.client;
    let verified = client.verify_vm_looking().await.unwrap();
    let pinned = verified.machine().unwrap().clone();

    let same = client.reproved(&pinned).await;
    let mut generation = pinned.clone();
    generation.generation += 1;
    let mut fence = pinned.clone();
    fence.fence.as_mut().unwrap().count += 1;
    let mut run = pinned.clone();
    run.fence.as_mut().unwrap().incarnation = "another-run".into();
    let mut epoch = pinned.clone();
    epoch.epoch.capture_pid += 1;
    let mut unfenced = pinned.clone();
    unfenced.fence = None;

    assert!(same.is_ok(), "{same:?}");
    for (what, before) in [
        ("generation", generation),
        ("fence", fence),
        ("run", run),
        ("epoch", epoch),
        ("no fence", unfenced),
    ] {
        let refused = client.reproved(&before).await.unwrap_err();
        assert_eq!(refused.code, Code::SessionChanged, "{what}: {refused:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_screenshot_of_a_vm_whose_runner_keeps_no_fence_is_refused_before_any_picture_lane() {
    let looking = looking("screenshot").await;
    let mut older = atlas_vm_state("closed", 0);
    older.as_object_mut().unwrap().remove("fence");
    *looking.serving.fixture.guest.runner.lock().unwrap() = older;

    let refused = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert_eq!(looking.picture_lanes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_screenshot_is_refused_without_screen_recording_in_the_guests_words_while_collect_is_served()
 {
    let looking = looking("screenshot").await;
    *looking.saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
        hello["capture"] = json!({"held": CAPTURE_HELD});
    });
    let home = Home::new();
    home.serving(&looking);
    let bytes = b"collected while pictures wait".to_vec();
    home.file("atlas-work/note.txt", &bytes);

    let refused = looking
        .client
        .capture(Size::Full, None, false)
        .await
        .unwrap_err();
    let lanes = looking.picture_lanes();
    let collected = crate::collect::collect(&looking.client, &["atlas-work/note.txt".into()], 1024)
        .await
        .unwrap();

    assert_eq!(refused.code, Code::Permission, "{refused:?}");
    assert_eq!(refused.error, CAPTURE_HELD);
    assert_eq!(lanes, 0, "refused before any picture lane");
    assert_eq!(collected["ok"], true, "{collected}");
    assert_eq!(collected["collected"][0]["sha256"], sha256(&bytes));
}

/// The guest's home, whose files its own read code serves on picture lanes.
struct Home {
    _folder: tempfile::TempDir,
    root: PathBuf,
}

impl Home {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap().join("guest");
        std::fs::create_dir(&root).unwrap();
        Self {
            _folder: folder,
            root,
        }
    }

    /// Write `bytes` at `name` in the home.
    fn file(&self, name: &str, bytes: &[u8]) {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
    }

    /// Serve the reads `looking`'s guest is asked for from this home, by
    /// the guest's own read code, paced by its bulk credit.
    fn serving(&self, looking: &Looking) {
        self.serving_then(looking, || {});
    }

    /// [`Home::serving`], running `reading` as each read is served.
    fn serving_then(&self, looking: &Looking, reading: impl Fn() + Send + Sync + 'static) {
        let root = self.root.clone();
        looking
            .serving
            .fixture
            .guest
            .standin
            .pictures
            .set(Arc::new(move |request| {
                let Ok(atlas_core::protocol::Request::Read(read)) =
                    atlas_core::protocol::parse(&serde_json::to_vec(request).unwrap())
                        .map(|asked| asked.request)
                else {
                    let mut line = serde_json::to_vec(&shot(request)).unwrap();
                    line.push(b'\n');
                    return Some(line);
                };
                reading();
                let mut lines = Vec::new();
                let reply = atlas_daemon::test_support::read_at(
                    &read,
                    root.clone(),
                    nix::unistd::getuid().as_raw(),
                    &mut |event| {
                        lines.push(event);
                        true
                    },
                    // Its clock is ahead, so a file just written counts as
                    // still.
                    SystemTime::now() + Duration::from_secs(3),
                );
                lines.push(reply);
                let mut said = Vec::new();
                for line in lines {
                    said.extend(serde_json::to_vec(&line).unwrap());
                    said.push(b'\n');
                }
                Some(said)
            }));
    }
}

fn sha256(bytes: &[u8]) -> String {
    atlas_core::name::hex(&Sha256::digest(bytes))
}

/// Bytes that look random, `length` of them.
fn random(length: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_mebibytes_collected_from_a_vm_come_back_with_their_digest_through_its_keeper() {
    let looking = looking("collect").await;
    let home = Home::new();
    home.serving(&looking);
    let bytes = random(3 << 20);
    home.file("atlas-work/random.bin", &bytes);

    let collected =
        crate::collect::collect(&looking.client, &["atlas-work/random.bin".into()], 4 << 20)
            .await
            .unwrap();

    assert_eq!(collected["ok"], true, "{collected}");
    let kept = &collected["collected"][0];
    assert_eq!(kept["sha256"], sha256(&bytes));
    assert_eq!(kept["size"], bytes.len());
    let copy = Path::new(kept["path"].as_str().unwrap());
    assert!(copy.starts_with(looking.client.records()), "{kept}");
    assert!(std::fs::read(copy).unwrap() == bytes);
    assert!(looking.picture_lanes() >= 2, "every part on a picture lane");
    let credit = looking.serving.fixture.guest.standin.bulk_credit().unwrap();
    until("every lane repaid", || {
        credit.left() == atlas_session::credit::BULK as usize
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_files_are_collected_under_each_stop_and_their_refusals_name_the_vm() {
    for by_station in [true, false] {
        let looking = looking("collect").await;
        looking.stopped(by_station);
        let home = Home::new();
        home.serving(&looking);
        home.file("atlas-work/a.txt", b"a");

        let collected = crate::collect::collect(
            &looking.client,
            &["atlas-work/a.txt".into(), "atlas-work/missing.txt".into()],
            1024,
        )
        .await
        .unwrap();

        assert_eq!(
            collected["collected"][0]["sha256"],
            sha256(b"a"),
            "{by_station}"
        );
        assert_eq!(collected["failed"][0]["reason"], "missing", "{collected}");
        let shown = collected.to_string();
        assert!(!shown.contains("helper"), "{shown}");
        assert!(shown.contains("No such file in the VM"), "{shown}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_picture_before_an_input_on_a_vm_is_of_the_screen_before_it_and_is_kept() {
    let looking = looking("click").await;
    looking.framing(Some(0x40));

    let clicked = agent::act(
        &looking.client,
        None,
        &agent::click(10.0, 10.0, Button::Left, 1, &[]),
    )
    .await
    .unwrap();

    assert_eq!(clicked["ok"], true, "{clicked}");
    assert_eq!(*looking.happened.lock().unwrap(), ["frame", "input"]);
    assert_eq!(looking.proofs(), 2, "proved again once it was in hand");
    assert_eq!(looking.pictures().len(), 1, "kept");
    let (name, _) = looking
        .client
        .doing
        .as_ref()
        .unwrap()
        .before()
        .facts()
        .unwrap();
    assert!(
        looking
            .client
            .records()
            .join(crate::before::FOLDER)
            .join(name)
            .exists()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_on_a_vm_goes_once_its_picture_is_given_up_and_none_is_kept() {
    let looking = looking("click").await;
    looking.framing(None);

    let clicked = agent::act(
        &looking.client,
        None,
        &agent::click(10.0, 10.0, Button::Left, 1, &[]),
    )
    .await
    .unwrap();

    assert_eq!(clicked["ok"], true, "{clicked}");
    assert_eq!(*looking.happened.lock().unwrap(), ["frame", "input"]);
    assert_eq!(looking.proofs(), 1, "none in hand to prove again");
    assert!(looking.pictures().is_empty());
    until("the frame's lane cancelled", || {
        looking
            .serving
            .fixture
            .guest
            .standin
            .pumped()
            .iter()
            .any(|pump| !pump.whole)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_before_an_input_proven_never_sent_is_never_kept() {
    for refused in [
        json!({"ok": false, "code": "denied", "error": "This account is Look Only.",
            "performed": 0}),
        json!({"ok": false, "code": "stopped", "error": "Agents were stopped; nothing was done"}),
        json!({"ok": false, "code": "transport", "uncertain": false, "sent": false,
            "error": "The change's lane to the guest did not open in time; nothing was sent."}),
    ] {
        let looking = looking("click").await;
        looking.framing(Some(0x40));
        *looking.refusing.lock().unwrap() = Some(refused.clone());

        let outcome = agent::act(
            &looking.client,
            None,
            &agent::click(10.0, 10.0, Button::Left, 1, &[]),
        )
        .await;

        assert!(
            outcome.as_ref().map_or(true, |reply| reply["ok"] == false),
            "{refused}: {outcome:?}"
        );
        assert_eq!(*looking.happened.lock().unwrap(), ["frame", "input"]);
        assert_eq!(looking.proofs(), 1, "{refused}: never proved again");
        assert!(looking.pictures().is_empty(), "{refused}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_before_an_input_that_went_in_part_is_kept() {
    let looking = looking("click").await;
    looking.framing(Some(0x40));
    *looking.refusing.lock().unwrap() = Some(json!({"ok": false, "code": "input",
        "error": "The input stopped part way.", "performed": 1, "partial": true}));

    let outcome = agent::act(
        &looking.client,
        None,
        &agent::click(10.0, 10.0, Button::Left, 1, &[]),
    )
    .await;

    assert!(
        outcome.as_ref().map_or(true, |reply| reply["ok"] == false),
        "{outcome:?}"
    );
    assert_eq!(looking.proofs(), 2, "proved again");
    assert_eq!(looking.pictures().len(), 1, "kept");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picture_before_an_input_whose_window_opened_meanwhile_is_never_kept() {
    let looking = looking("click").await;
    looking.framing(Some(0x40));
    let guest = looking.serving.fixture.guest.clone();
    *looking.saying.otherwise.lock().unwrap() =
        Some(Arc::new(move |header: &Header, request| {
            match (&header.kind, request["op"].as_str()) {
                (LaneKind::Input { change: Some(_) }, Some("input")) => {
                    *guest.runner.lock().unwrap() = atlas_vm_state("closed", 1);
                    json!({"ok": true, "app": null, "bundleId": null, "performed": 1})
                }
                (LaneKind::Input { change: Some(_) }, Some("release")) => {
                    json!({"ok": true, "released": true})
                }
                _ => json!({"ok": true}),
            }
        }));

    let clicked = agent::act(
        &looking.client,
        None,
        &agent::click(10.0, 10.0, Button::Left, 1, &[]),
    )
    .await
    .unwrap();

    assert_eq!(clicked["ok"], true, "{clicked}");
    assert_eq!(looking.proofs(), 2, "in hand, and proved again");
    assert!(looking.pictures().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plain_click_after_a_vms_screenshot_still_goes() {
    let looking = looking("click").await;

    let saved = looking
        .client
        .capture(Size::Standard, None, false)
        .await
        .unwrap();
    let clicked = agent::act(
        &looking.client,
        None,
        &agent::click(10.0, 10.0, Button::Left, 1, &[]),
    )
    .await
    .unwrap();

    assert_eq!(saved["ok"], true, "{saved}");
    assert_eq!(clicked["ok"], true, "{clicked}");
    assert!(looking.happened.lock().unwrap().contains(&"input"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_screenshot_remembers_its_windows_so_a_plain_click_on_one_is_held_to_it() {
    let mut looking = looking("click").await;
    looking.client.host_session = Some(
        crate::host_session::HostSession::new(crate::host_session::Host::Named, "one").unwrap(),
    );
    let generation = live(&looking.serving);
    let heard: Arc<Mutex<Vec<(u64, Value)>>> = Arc::default();
    let hearing = heard.clone();
    *looking.saying.otherwise.lock().unwrap() =
        Some(Arc::new(move |header: &Header, request: Value| {
            hearing
                .lock()
                .unwrap()
                .push((header.generation.0, request.clone()));
            match (&header.kind, request["op"].as_str()) {
                (LaneKind::Read, Some("windows")) => windows(),
                (LaneKind::Picture, Some("screenshot")) => shot(&request),
                (LaneKind::Input { change: Some(_) }, Some("acquire")) => json!({"ok": true}),
                (LaneKind::Input { change: Some(_) }, Some("input")) => {
                    json!({"ok": true, "app": "Notes", "bundleId": null, "performed": 1,
                        "expect": request["expect"]})
                }
                (LaneKind::Input { change: Some(_) }, Some("release")) => {
                    json!({"ok": true, "released": true})
                }
                _ => json!({"ok": false, "code": "bad-request", "error": "Not asked for."}),
            }
        }));

    let saved = looking
        .client
        .capture(Size::Standard, None, false)
        .await
        .unwrap();
    let clicked = agent::act(
        &looking.client,
        None,
        &agent::click(30.0, 40.0, Button::Left, 1, &[]),
    )
    .await
    .unwrap();

    assert_eq!(saved["ok"], true, "{saved}");
    assert_eq!(clicked["ok"], true, "{clicked}");
    let heard = heard.lock().unwrap().clone();
    // The picture before the click is asked for beside taking input.
    let ops: Vec<&str> = heard
        .iter()
        .map(|(_, request)| request["op"].as_str().unwrap())
        .filter(|op| *op != "frame")
        .collect();
    assert_eq!(
        ops,
        [
            "windows",
            "screenshot",
            "windows",
            "windows",
            "acquire",
            "input",
            "release"
        ],
        "the windows read on either side of the picture, then to find the one clicked"
    );
    assert!(heard.iter().all(|(of, _)| *of == generation), "{heard:?}");
    let input = heard
        .iter()
        .find(|(_, request)| request["op"] == "input")
        .unwrap();
    assert_eq!(
        input.1["expect"],
        json!({"pid": NOTES, "start": [123, 456], "window": WINDOW})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_read_while_a_vms_window_opened_or_opened_and_closed_again_are_never_kept() {
    for (window, reason) in [("open", "on-console"), ("closed", "session-changed")] {
        let looking = looking("collect").await;
        let home = Home::new();
        let guest = looking.serving.fixture.guest.clone();
        home.serving_then(&looking, move || {
            *guest.runner.lock().unwrap() = atlas_vm_state(window, 1);
        });
        home.file("atlas-work/a.txt", b"a");
        home.file("atlas-work/b.txt", b"b");

        let collected = crate::collect::collect(
            &looking.client,
            &["atlas-work/a.txt".into(), "atlas-work/b.txt".into()],
            1024,
        )
        .await
        .unwrap();

        assert_eq!(collected["collected"], json!([]), "{window}: {collected}");
        assert_eq!(collected["receipt"], Value::Null);
        let failed = collected["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 2, "{collected}");
        for failure in failed {
            assert_eq!(failure["reason"], reason, "{window}: {failure}");
        }
        let shown = collected.to_string();
        assert!(!shown.contains("helper"), "{shown}");
        if window == "closed" {
            assert!(
                shown.contains("work changed, or its window was opened"),
                "{shown}"
            );
        }
        assert!(
            kept_copies(looking.client.records()).is_empty(),
            "{window}: nothing kept"
        );
    }
}

/// Every collected copy kept under `records`.
fn kept_copies(records: &Path) -> Vec<PathBuf> {
    fn walk(folder: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&records.join("collected"), &mut found);
    found
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_are_never_read_from_a_vm_whose_runner_keeps_no_fence() {
    let looking = looking("collect").await;
    let home = Home::new();
    home.serving(&looking);
    home.file("atlas-work/a.txt", b"a");
    let mut older = atlas_vm_state("closed", 0);
    older.as_object_mut().unwrap().remove("fence");
    *looking.serving.fixture.guest.runner.lock().unwrap() = older;

    let refused = crate::collect::collect(&looking.client, &["atlas-work/a.txt".into()], 1024)
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Update, "{refused:?}");
    assert!(!refused.error.contains("helper"), "{refused:?}");
    assert_eq!(looking.picture_lanes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_screenshots_target_is_looked_up_only_in_the_generation_it_was_proved_in() {
    let looking = looking("screenshot").await;
    let reads = Arc::new(Mutex::new(0));
    let counted = reads.clone();
    *looking.saying.otherwise.lock().unwrap() =
        Some(Arc::new(move |header: &Header, request| {
            match (&header.kind, request["op"].as_str()) {
                (LaneKind::Read, Some("windows")) => {
                    *counted.lock().unwrap() += 1;
                    windows()
                }
                (LaneKind::Read, Some("apps")) => {
                    *counted.lock().unwrap() += 1;
                    apps()
                }
                _ => json!({"ok": true, "echo": request}),
            }
        }));
    let verified = looking.client.verify_vm_looking().await.unwrap();
    crate::vm::keeper::change::tests::restarted(&looking.serving).await;

    let windows = looking
        .client
        .windows_in(Some(&verified), false, Duration::from_secs(5))
        .await
        .unwrap_err();
    let apps = looking.client.apps_in(&verified).await.unwrap_err();

    assert_eq!(windows.code, Code::SessionChanged, "{windows:?}");
    assert_eq!(apps.code, Code::SessionChanged, "{apps:?}");
    assert_eq!(*reads.lock().unwrap(), 0, "neither read ran");
}

/// Every word of `outcome`, a refusal or a reply, its next step as an
/// agent is shown it included.
fn said(outcome: &atlas_core::error::Result<Value>) -> String {
    match outcome {
        Ok(reply) => reply.to_string(),
        Err(error) => serde_json::to_string(error).unwrap(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_screenshot_and_collect_refusals_and_their_next_steps_name_the_vm_never_the_helper() {
    let looking = looking("screenshot").await;
    let client = &looking.client;
    *looking.saying.hello.lock().unwrap() = Some(|hello| {
        hello["permissions"]["screenCapture"] = json!(false);
        hello["capture"] = json!({"held": CAPTURE_HELD});
    });
    let unrecorded = agent::screenshot(client, None, None, None, Size::Standard, false).await;
    *looking.saying.hello.lock().unwrap() = None;
    let guest = looking.serving.fixture.guest.clone();
    looking.between(move || *guest.runner.lock().unwrap() = atlas_vm_state("closed", 1));
    let changed = agent::screenshot(client, None, None, None, Size::Standard, false).await;
    *looking.serving.fixture.guest.runner.lock().unwrap() = atlas_vm_state("closed", 1);
    let home = Home::new();
    home.serving(&looking);
    let missing = crate::collect::collect(client, &["atlas-work/missing.txt".into()], 1024).await;
    let outside = crate::collect::collect(client, &["../a.txt".into()], 1024).await;

    let unrecorded_next = unrecorded.as_ref().unwrap_err().details["next"].clone();
    assert_eq!(
        unrecorded_next,
        "Ask the person to open work's window with atlas desktop maintain work and allow Atlas \
         in Privacy & Security, Screen & System Audio Recording, there."
    );
    assert_eq!(changed.as_ref().unwrap_err().code, Code::SessionChanged);
    assert!(changed.as_ref().unwrap_err().details.contains_key("next"));
    let missing_next = missing.as_ref().unwrap()["next"].clone();
    assert_eq!(
        missing_next,
        "Read reason: the file was not copied, and nothing in work changed."
    );
    for outcome in [&unrecorded, &changed, &missing, &outside] {
        let shown = said(outcome);
        assert!(!shown.contains("helper"), "{shown}");
    }
}
