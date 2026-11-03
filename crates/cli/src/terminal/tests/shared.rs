use super::*;

/// Against the menu bar running on this Mac: the terminal finds its frame
/// service, reads committed pictures from it, and takes control on one
/// through the menu bar's own routes.
#[tokio::test]
#[ignore = "needs the installed menu bar running on this Mac"]
async fn the_terminal_shares_the_running_menu_bar_frame_service() {
    let config =
        atlas_core::config::Config::load(&atlas_core::config::installed_path().unwrap()).unwrap();
    let client = Client::new(config).unwrap();
    let (url, descriptor) = shared(&client).await.expect("a running frame service");
    let mut frames = Frames::Shared {
        descriptor,
        client: None,
    };
    let began = std::time::Instant::now();
    let mut last = None;
    for _ in 0..60 {
        let frame = frames.next().await.unwrap();
        let rgb = frame.rgb.as_ref().expect("raw pixels");
        assert_eq!(rgb.len() as u32, frame.pixels.0 * frame.pixels.1 * 3);
        last = Some(frame.key);
    }
    let rate = 60.0 / began.elapsed().as_secs_f64();
    eprintln!("shared-terminal fps={rate:.1}");
    assert!(rate > 10.0);
    let key = last.unwrap();
    let granted = tokio::task::spawn_blocking(move || {
        let lease = viewer::post_local(&url, "control/acquire", &json!({"frame": key}))?;
        let lease = lease["lease"].as_str().unwrap_or_default().to_owned();
        viewer::post_local(&url, "control/release", &json!({"lease": lease}))
    })
    .await
    .unwrap();
    assert!(granted.is_ok(), "{granted:?}");
}
