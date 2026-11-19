use super::*;
use crate::frame_pipe::client::{LONGEST, PipeClient};
use atlas_client::test_support::{TAKEN_OPEN, vm_through_keeper};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

/// The frame service of the scripted VM, every picture shown to every
/// observer, counting the pictures observers committed.
async fn served(client: Client) -> (Service, Arc<AtomicUsize>) {
    let committed = Arc::new(AtomicUsize::new(0));
    let counted = committed.clone();
    let privacy = Privacy {
        visible: Arc::new({
            let controller = client.controller.clone();
            move || controller.screen_visible().ok()
        }),
        exempt: Arc::new(|_| false),
    };
    let service = start(
        Arc::new(client),
        LONGEST,
        Arc::new(move |_, _, _| {
            counted.fetch_add(1, Ordering::SeqCst);
        }),
        privacy,
        Arc::new(Notify::new()),
    )
    .await
    .unwrap();
    (service, committed)
}

/// Why a fresh observer of `descriptor` is refused now: the service's
/// fixed words for the reason its source gave, which the app words for the
/// desktop it shows.
async fn refused_now(descriptor: &serde_json::Value) -> String {
    PipeClient::open(descriptor, LONGEST)
        .await
        .err()
        .expect("refused")
        .error
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_captured_while_the_vms_window_was_open_never_reaches_the_app() {
    let _alone = SCRIPTED_VM.lock().await;
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = vm_through_keeper(directory.path());
    let (service, committed) = served(client).await;
    let descriptor = service.descriptor().clone();
    assert_eq!(descriptor["desktopKey"], "vm-0123456789abcdef01234567");
    assert!(
        descriptor.get("helperUID").is_none(),
        "its guest's account is none of this Mac's"
    );

    let mut app = PipeClient::open(&descriptor, LONGEST).await.unwrap();
    for _ in 0..3 {
        let shown = app.committed().await.unwrap();
        assert!(!shown.rgb.contains(&TAKEN_OPEN));
    }
    *keeper.window.lock().unwrap() = ("open", 1);
    while let Ok(shown) = app.committed().await {
        assert!(
            !shown.rgb.contains(&TAKEN_OPEN),
            "a picture taken while the window was open was shown"
        );
    }
    assert_eq!(
        refused_now(&descriptor).await,
        "The frame service refused: hidden",
        "said withheld, and why"
    );
    let before = committed.load(Ordering::SeqCst);

    *keeper.window.lock().unwrap() = ("closed", 1);
    let began = std::time::Instant::now();
    let shown = loop {
        match PipeClient::open(&descriptor, LONGEST).await {
            Ok(mut app) => {
                if let Ok(shown) = app.committed().await {
                    break shown;
                }
            }
            Err(withheld) => assert_eq!(withheld.error, "The frame service refused: hidden"),
        }
        assert!(
            began.elapsed() < Duration::from_secs(15),
            "never shown again"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(!shown.rgb.contains(&TAKEN_OPEN));
    assert!(committed.load(Ordering::SeqCst) > before);
    assert!(
        keeper.asked_for("picture").len() <= 4,
        "reopened with backoff: {} live pictures",
        keeper.asked_for("picture").len()
    );
    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_pictures_stop_while_the_person_maintains_it_and_are_said_withheld() {
    let _alone = SCRIPTED_VM.lock().await;
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = vm_through_keeper(directory.path());
    let id = client.desktop().id();
    let (service, _) = served(client).await;
    let descriptor = service.descriptor().clone();
    let mut app = PipeClient::open(&descriptor, LONGEST).await.unwrap();
    app.committed().await.unwrap();

    atlas_client::test_support::under_maintenance(
        directory.path(),
        &id,
        atlas_client::vm::lifecycle::record::Phase::Open,
    );
    let began = std::time::Instant::now();
    while app.committed().await.is_ok() {
        assert!(began.elapsed() < Duration::from_secs(10), "still shown");
    }

    assert_eq!(
        refused_now(&descriptor).await,
        "The frame service refused: paused"
    );
    let pictures = keeper.asked_for("picture").len();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        refused_now(&descriptor).await,
        "The frame service refused: paused"
    );
    assert_eq!(
        keeper.asked_for("picture").len(),
        pictures,
        "no live picture is asked while it is maintained"
    );
    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vm_without_screen_recording_is_said_withheld_and_asked_for_no_picture() {
    let _alone = SCRIPTED_VM.lock().await;
    let directory = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let (client, keeper) = vm_through_keeper(directory.path());
    *keeper.observing.lock().unwrap() = Some(|observed| {
        observed["status"]["hello"]["permissions"]["screenCapture"] = false.into();
    });
    let (service, _) = served(client).await;

    assert_eq!(
        refused_now(service.descriptor()).await,
        "The frame service refused: consent-changed"
    );
    assert!(keeper.asked_for("picture").is_empty());
    service.stop().await;
}
