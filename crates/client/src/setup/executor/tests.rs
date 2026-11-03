use super::*;
#[test]
fn quiet_commands_hide_log_lines_and_keep_their_outcome() {
    use atlas_core::test_support::{TestChild, message, rerun};
    use std::{process::Stdio, sync::mpsc::RecvTimeoutError};

    const NAME: &str =
        "setup::executor::tests::quiet_commands_hide_log_lines_and_keep_their_outcome";
    const LOG: &str = "2026-10-11 14:28:19.767 sysadminctl[1:2] noise\n";
    const PROMPT: &str = "User password:";
    const OUTPUT: &str = "command output";
    if let Ok(status) = std::env::var("ATLAS_QUIET_COMMAND_FIXTURE") {
        assert!(matches!(status.as_str(), "0" | "3"));
        // The fixture asks no real password. It cannot finish its partial line
        // until the parent observes the actual prompt flush and sends a permit.
        let script = "printf '%s' \"$1\" >&2; printf '%s' \"$2\" >&2; \
                          read -r -t 10 permit && test \"$permit\" = continue || exit 9; \
                          printf '\\n' >&2; printf '%s' \"$3\"; exit \"$4\"";
        let child = Command::new("/bin/sh")
            .args(["-c", script, "quiet-fixture", LOG, PROMPT, OUTPUT, &status])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        message("quiet command started");
        let result = quietly_attended(child, "it failed");
        if status == "0" {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.unwrap_err().error, "it failed");
        }
        message("quiet command ended");
        return;
    }

    for status in ["0", "3"] {
        let mut child = TestChild::spawn(
            rerun(NAME)
                .arg("--nocapture")
                .env("ATLAS_QUIET_COMMAND_FIXTURE", status),
        );
        let receiver = forward(child.process.stderr.take().unwrap());
        child.expect("quiet command started");
        let mut seen = Vec::new();
        while seen.len() < PROMPT.len() {
            seen.extend(receiver.recv_timeout(Duration::from_secs(10)).unwrap());
        }
        assert_eq!(
            seen,
            PROMPT.as_bytes(),
            "only the partial prompt is flushed"
        );
        child.permit("continue");
        child.expect("quiet command ended");
        assert!(child.wait().success());
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)) {
                Ok(bytes) => seen.extend(bytes),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("the owned stderr reader did not finish")
                }
            }
        }
        let expected = if status == "0" {
            format!("{PROMPT}\n")
        } else {
            format!("{PROMPT}\n{LOG}{OUTPUT}")
        };
        assert_eq!(seen, expected.as_bytes(), "status {status}");
    }
}
