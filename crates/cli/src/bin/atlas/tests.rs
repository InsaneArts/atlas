use super::*;

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
