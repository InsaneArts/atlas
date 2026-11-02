use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

/// An owned test process with messages on stdout and permits on stdin.
/// Dropping it on any failure kills and reaps it.
pub struct TestChild {
    pub process: Child,
    messages: Receiver<String>,
}

impl TestChild {
    pub fn spawn(command: &mut Command) -> Self {
        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = process.stdout.take().unwrap();
        let (send, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some((_, message)) = line.split_once("atlas-test:") {
                    let _ = send.send(message.to_owned());
                }
            }
        });
        Self { process, messages }
    }

    pub fn expect(&self, message: &str) {
        assert_eq!(
            self.messages.recv_timeout(Duration::from_secs(30)).unwrap(),
            message
        );
    }

    pub fn permit(&mut self, message: &str) {
        writeln!(self.process.stdin.as_mut().unwrap(), "{message}").unwrap();
    }

    pub fn wait(&mut self) -> ExitStatus {
        let began = Instant::now();
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                return status;
            }
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "child did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

pub fn message(message: &str) {
    println!("atlas-test:{message}");
    std::io::stdout().flush().unwrap();
}

pub fn permitted(message: &str) {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), message);
}
