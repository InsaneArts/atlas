use super::*;
use crate::viewer::http::configured;
use atlas_core::config::desktops::Selected;

/// A viewer that reads nothing, and the upkeep reads it makes, each sent
/// with the time it began.
fn keeping() -> (
    Arc<Viewer>,
    tokio::sync::mpsc::UnboundedReceiver<Instant>,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(dir.path());
    let viewer = configured(client, "127.0.0.1:0".to_owned(), None);
    let (made, reads) = tokio::sync::mpsc::unbounded_channel();
    let keeper = viewer.clone();
    let keeping = tokio::spawn(async move {
        let reading = keeper.clone();
        every_quiet_period(&keeper, move || {
            reading.status_read();
            let _ = made.send(Instant::now());
            std::future::ready(())
        })
        .await;
    });
    (viewer, reads, keeping, dir)
}

#[tokio::test(start_paused = true)]
async fn a_service_nobody_reads_reads_its_status_every_period() {
    let (_viewer, mut reads, keeping, _dir) = keeping();
    let start = Instant::now();

    assert_eq!(
        reads.recv().await.unwrap(),
        start,
        "at once, with none read yet"
    );
    assert_eq!(reads.recv().await.unwrap(), start + PERIOD);
    assert_eq!(reads.recv().await.unwrap(), start + PERIOD * 2);
    keeping.abort();
}

#[tokio::test(start_paused = true)]
async fn a_readers_read_puts_the_next_upkeep_off_by_a_whole_period() {
    let (viewer, mut reads, keeping, _dir) = keeping();
    let start = Instant::now();
    assert_eq!(reads.recv().await.unwrap(), start);

    tokio::time::sleep(PERIOD / 2).await;
    viewer.status_read();

    assert_eq!(
        reads.recv().await.unwrap(),
        start + PERIOD / 2 + PERIOD,
        "a period after the reader's, not the upkeep's own"
    );
    keeping.abort();
}

#[tokio::test]
async fn a_service_nobody_reads_follows_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let settings = base.join("u.toml");
    std::fs::write(&settings, atlas_core::test_support::SETTINGS).unwrap();
    let client = atlas_client::client::Client::new(Selected::provisional(
        atlas_core::config::Config::load(&settings).unwrap(),
    ))
    .unwrap();
    let (_, viewer) = crate::viewer::http::bind(client, 0, Some(&settings))
        .await
        .unwrap();
    std::fs::write(
        &settings,
        atlas_core::test_support::SETTINGS.replace("\"out\"", "\"moved\""),
    )
    .unwrap();
    let mut reads = viewer.last_status.subscribe();

    let keeping = tokio::spawn(keep_up(viewer.clone()));
    // The second read begins once the first, which follows the settings,
    // has ended.
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(30), reads.changed())
            .await
            .expect("the service read its status")
            .unwrap();
    }
    keeping.abort();

    assert_eq!(viewer.client().output_dir().unwrap(), base.join("moved"));
}
