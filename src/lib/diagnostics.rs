//! Best-effort diagnostics that never wait for their output sink.
use std::{
    io::{self, Write},
    sync::{OnceLock, mpsc},
};

pub(crate) struct Diagnostics(Option<mpsc::SyncSender<String>>);

impl Diagnostics {
    pub(crate) fn new(mut sink: impl Write + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<String>(16);
        let worker = std::thread::Builder::new()
            .name("atlas-diagnostics".into())
            .spawn(move || {
                for message in receiver {
                    let _ = writeln!(sink, "{message}");
                }
            });
        Self(worker.ok().map(|_| sender))
    }

    pub(crate) fn report(&self, message: String) {
        if let Some(sender) = &self.0 {
            // A full or disconnected queue must not delay cleanup or supervision.
            let _ = sender.try_send(message);
        }
    }
}

pub(crate) fn report(message: String) {
    static DIAGNOSTICS: OnceLock<Diagnostics> = OnceLock::new();
    DIAGNOSTICS
        .get_or_init(|| Diagnostics::new(io::stderr()))
        .report(message);
}
