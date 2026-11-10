use super::*;

#[test]
fn terminal_viewing_uses_installed_settings_instead_of_checkout_settings() {
    let command = Command::Monitor {
        terminal: true,
        read_only: false,
        max_dimension: None,
        capture_backend: None,
        port: 8765,
    };
    assert_eq!(
        configuration(Some(&command), None),
        atlas_core::config::installed_path().unwrap()
    );
    let explicit = PathBuf::from("another-helper.toml");
    assert_eq!(
        configuration(Some(&command), Some(explicit.clone())),
        explicit
    );
    let native = Command::Monitor {
        terminal: false,
        read_only: false,
        max_dimension: None,
        capture_backend: None,
        port: 8765,
    };
    assert_eq!(
        configuration(Some(&native), None),
        atlas_core::config::effective(None)
    );
}

#[test]
fn opening_a_set_up_atlas_shows_the_viewer_window() {
    let command = viewer_command(
        std::path::Path::new("/Applications/Atlas.app/Contents/MacOS/atlas"),
        std::path::Path::new("/Users/me/.config/atlas.toml"),
    );
    assert_eq!(
        command.get_program().to_str(),
        Some("/Applications/Atlas.app/Contents/MacOS/atlas")
    );
    assert_eq!(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["--config", "/Users/me/.config/atlas.toml", "monitor"]
    );
}

#[test]
fn a_signal_gives_its_status_whatever_the_result() {
    assert_eq!(exit_status(false, None), ExitCode::SUCCESS);
    assert_eq!(exit_status(true, None), ExitCode::FAILURE);
    for failed in [false, true] {
        assert_eq!(
            exit_status(failed, Some(Signal::SIGTERM)),
            ExitCode::from(143)
        );
        assert_eq!(
            exit_status(failed, Some(Signal::SIGHUP)),
            ExitCode::from(129)
        );
        assert_eq!(
            exit_status(failed, Some(Signal::SIGINT)),
            ExitCode::from(130)
        );
    }
}
