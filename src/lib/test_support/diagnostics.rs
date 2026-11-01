use crate::diagnostics::Diagnostics;
use std::{
    io::{self, Write},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

struct Stalled {
    started: Option<Sender<()>>,
    resume: Receiver<()>,
}

impl Write for Stalled {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
            let _ = self.resume.recv_timeout(Duration::from_secs(10));
        }
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn stalled_diagnostics() -> (Diagnostics, Receiver<()>, Sender<()>) {
    let (started, writing) = mpsc::channel();
    let (resume, stalled) = mpsc::channel();
    let diagnostics = Diagnostics::new(Stalled {
        started: Some(started),
        resume: stalled,
    });
    (diagnostics, writing, resume)
}
