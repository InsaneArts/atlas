//! Bounded frame output. No complete presentation payload is retained here.
use std::{
    io::{self, BufWriter, Write},
    time::{Duration, Instant},
};

const CAPACITY: usize = 64 * 1024;
const BEGIN: &[u8] = b"\x1b[?2026h";
const END: &[u8] = b"\x1b[?2026l";
const APC_END: &[u8] = b"\x1b\\";

#[derive(Debug)]
pub(super) struct Sent {
    pub(super) bytes: usize,
    pub(super) write_ms: f64,
}

struct Metered<'a, W: Write> {
    out: &'a mut W,
    bytes: usize,
    elapsed: Duration,
}

impl<W: Write> Write for Metered<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let began = Instant::now();
        let result = self.out.write(bytes);
        self.elapsed += began.elapsed();
        if let Ok(written) = result {
            self.bytes += written;
        }
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        let began = Instant::now();
        let result = self.out.flush();
        self.elapsed += began.elapsed();
        result
    }
}

impl<W: Write> Metered<'_, W> {
    fn reset(&mut self) {
        // Either an APC upload or synchronized output may be incomplete. Do
        // not send any buffered payload again, and do not hide the first error.
        let _ = self.write_all(APC_END);
        let _ = self.write_all(END);
        let _ = self.flush();
    }
}

pub(super) struct Frame<'a, W: Write> {
    buffered: Option<BufWriter<Metered<'a, W>>>,
    started: bool,
    failure: Option<io::Error>,
}

impl<'a, W: Write> Frame<'a, W> {
    fn new(out: &'a mut W) -> Self {
        Self {
            buffered: Some(BufWriter::with_capacity(
                CAPACITY,
                Metered {
                    out,
                    bytes: 0,
                    elapsed: Duration::ZERO,
                },
            )),
            started: false,
            failure: None,
        }
    }

    fn buffer(&mut self) -> &mut BufWriter<Metered<'a, W>> {
        self.buffered.as_mut().expect("the frame is still open")
    }

    fn refuse(&mut self, error: io::Error) -> io::Error {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        self.previous_error().expect("the failure was retained")
    }

    fn previous_error(&self) -> Option<io::Error> {
        self.failure
            .as_ref()
            .map(|error| match error.raw_os_error() {
                Some(code) => io::Error::from_raw_os_error(code),
                None => io::Error::new(error.kind(), error.to_string()),
            })
    }

    fn begin(&mut self) -> io::Result<()> {
        if !self.started {
            // Mark this before attempting BEGIN: a partial write also needs
            // best-effort parser and synchronized-output cleanup.
            self.started = true;
            if let Err(error) = self.buffer().get_mut().write_all(BEGIN) {
                return Err(self.refuse(error));
            }
        }
        Ok(())
    }

    fn flush_buffer(&mut self) -> io::Result<()> {
        loop {
            match self.buffer().flush() {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }

    fn finish(&mut self, rendered: io::Result<()>) -> io::Result<Sent> {
        if let Err(error) = rendered
            && self.failure.is_none()
        {
            self.failure = Some(error);
        }
        if self.failure.is_none() && self.started {
            let result = self
                .buffer()
                .write_all(END)
                .and_then(|()| self.flush_buffer());
            if let Err(error) = result {
                self.failure = Some(error);
            }
        }
        let buffered = self.buffered.take().expect("finish only once");
        // Unlike dropping BufWriter or calling into_inner, into_parts never
        // attempts to flush. A failed upload must never be silently replayed.
        let (mut metered, _) = buffered.into_parts();
        if let Some(error) = self.failure.take() {
            if self.started {
                metered.reset();
            }
            return Err(error);
        }
        Ok(Sent {
            bytes: metered.bytes,
            write_ms: metered.elapsed.as_secs_f64() * 1000.0,
        })
    }
}

impl<W: Write> Write for Frame<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(error) = self.previous_error() {
            return Err(error);
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        self.begin()?;
        // BufWriter bypasses its buffer for large inputs. Limit each call as
        // well as its allocation so even a single huge write stays bounded.
        let result = self.buffer().write(&bytes[..bytes.len().min(CAPACITY)]);
        match result {
            Ok(0) => Err(self.refuse(io::Error::new(
                io::ErrorKind::WriteZero,
                "The terminal accepted no frame bytes",
            ))),
            Ok(written) => Ok(written),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(error),
            Err(error) => Err(self.refuse(error)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(error) = self.previous_error() {
            return Err(error);
        }
        if !self.started {
            return Ok(());
        }
        self.flush_buffer().map_err(|error| self.refuse(error))
    }
}

impl<W: Write> Drop for Frame<'_, W> {
    fn drop(&mut self) {
        // Unwinding a renderer must not auto-flush an incomplete upload.
        if let Some(buffered) = self.buffered.take() {
            let (mut metered, _) = buffered.into_parts();
            if self.started {
                metered.reset();
            }
        }
    }
}

pub(super) fn frame<W: Write, F>(out: &mut W, draw: F) -> io::Result<Sent>
where
    F: FnOnce(&mut Frame<'_, W>) -> io::Result<()>,
{
    let mut frame = Frame::new(out);
    let rendered = draw(&mut frame);
    frame.finish(rendered)
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
