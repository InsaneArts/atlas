//! A Noise connection over any byte stream, as a byte stream itself.
//!
//! The guest is always the Noise initiator and the station the responder,
//! whichever of them opened the stream, so the pairing secret `XXpsk3`
//! mixes in at its last message is always the guest's to prove. A pairing
//! runs `XXpsk3`; every later connection runs `KK` with both keys pinned.
//! Handshakes carry no payload, since a recorded first message can be
//! replayed. The prologue binds the version, the desktop and what the
//! connection is for, so a mismatch fails the handshake.
//!
//! What follows is frames of at most 65,535 bytes behind a two-byte length,
//! each a type byte and its data. A frame that does not decrypt (one
//! replayed, out of order or changed) breaks the connection for good. An
//! end that finishes says so in a close frame; a stream cut without one is
//! a failure, never a clean end.
use crate::keys::{Keypair, PairingSecret};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    session::key::PeerKey,
};
use snow::{Builder, HandshakeState, TransportState};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll, Waker, ready},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

/// The largest Noise frame.
const FRAME: usize = u16::MAX as usize;
/// What each frame spends on its tag.
const TAG: usize = 16;
/// The most data one frame carries, after its type byte.
const DATA_PER_FRAME: usize = FRAME - TAG - 1;
/// The half of a connection that reads, and the half that writes.
const READS: usize = 0;
const WRITES: usize = 1;
/// The type byte of a frame of data.
const DATA: u8 = 0;
/// The type byte of the frame an end sends when it finishes.
const CLOSE: u8 = 1;
const PAIRING: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";
const PINNED: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";

/// What a connection is for, bound into its handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Pairing,
    Control,
    Lane,
}

impl Purpose {
    const fn word(self) -> &'static str {
        match self {
            Self::Pairing => "pairing",
            Self::Control => "control",
            Self::Lane => "lane",
        }
    }
}

/// The prologue both ends must agree on.
fn prologue(desktop: &DesktopId, purpose: Purpose) -> Vec<u8> {
    format!("atlas session 1\n{}\n{}", desktop.as_str(), purpose.word()).into_bytes()
}

/// Pair as the guest: prove `secret` and learn the station's key.
///
/// The guest's handshake ends with its own last message, before the station
/// has shown it holds the secret too. The station proves that with its
/// first message: only a station holding the secret seals one that opens
/// here. So the guest commits nothing until that message has arrived and
/// opened.
///
/// # Errors
///
/// Fails when the stream fails or ends during the handshake. A station
/// without the secret is found out only at its first message, which fails
/// to open.
pub async fn pair_as_guest<S>(
    stream: S,
    own: &Keypair,
    secret: &PairingSecret,
    desktop: &DesktopId,
) -> Result<Secure<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let prologue = prologue(desktop, Purpose::Pairing);
    let state = pairing(own, secret, &prologue)?
        .build_initiator()
        .map_err(noise)?;
    handshake(stream, state, own).await
}

/// Pair as the station: the guest proves `secret`, and its key is the
/// connection's peer. The station proves the secret back with its first
/// message, which only opens at a guest that holds it.
///
/// # Errors
///
/// Fails when the stream fails or ends, or the guest does not prove the
/// secret.
pub async fn pair_as_station<S>(
    stream: S,
    own: &Keypair,
    secret: &PairingSecret,
    desktop: &DesktopId,
) -> Result<Secure<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let prologue = prologue(desktop, Purpose::Pairing);
    let state = pairing(own, secret, &prologue)?
        .build_responder()
        .map_err(noise)?;
    handshake(stream, state, own).await
}

/// Connect as the guest to the station whose key is `station`. Its second
/// handshake message proves the station holds `station`'s secret.
///
/// # Errors
///
/// Fails when the stream fails or ends, or the other end does not hold
/// `station`'s secret or does not admit this guest's key.
pub async fn connect_as_guest<S>(
    stream: S,
    own: &Keypair,
    station: PeerKey,
    desktop: &DesktopId,
    purpose: Purpose,
) -> Result<Secure<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let prologue = prologue(desktop, purpose);
    let state = pinned(own, &station, &prologue)?
        .build_initiator()
        .map_err(noise)?;
    handshake(stream, state, own).await
}

/// Answer, as the station, a connection from the guest whose key is
/// `guest`.
///
/// The handshake ends on the guest's first message, which anyone who
/// recorded it can replay. So a connection this returns proves nothing
/// until the guest's first message after the handshake has arrived and
/// opened: only a live guest, holding the keys this handshake made, can
/// seal one. Nothing is granted on the strength of the handshake alone.
///
/// # Errors
///
/// Fails when the stream fails or ends, or the first message was not made
/// with `guest`'s secret for this station.
pub async fn connect_as_station<S>(
    stream: S,
    own: &Keypair,
    guest: PeerKey,
    desktop: &DesktopId,
    purpose: Purpose,
) -> Result<Secure<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let prologue = prologue(desktop, purpose);
    let state = pinned(own, &guest, &prologue)?
        .build_responder()
        .map_err(noise)?;
    handshake(stream, state, own).await
}

fn pairing<'a>(
    own: &'a Keypair,
    secret: &'a PairingSecret,
    prologue: &'a [u8],
) -> Result<Builder<'a>> {
    Builder::new(PAIRING.parse().map_err(noise)?)
        .prologue(prologue)
        .and_then(|built| built.local_private_key(own.secret()))
        .and_then(|built| built.psk(3, secret.as_bytes()))
        .map_err(noise)
}

fn pinned<'a>(own: &'a Keypair, peer: &'a PeerKey, prologue: &'a [u8]) -> Result<Builder<'a>> {
    Builder::new(PINNED.parse().map_err(noise)?)
        .prologue(prologue)
        .and_then(|built| built.local_private_key(own.secret()))
        .and_then(|built| built.remote_public_key(peer.as_bytes()))
        .map_err(noise)
}

async fn handshake<S>(mut stream: S, mut state: HandshakeState, own: &Keypair) -> Result<Secure<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut frame = vec![0; FRAME];
    let mut payload = vec![0; FRAME];
    while !state.is_handshake_finished() {
        if state.is_my_turn() {
            let length = state.write_message(&[], &mut frame).map_err(noise)?;
            write_frame(&mut stream, &frame[..length]).await?;
        } else {
            let length = read_frame(&mut stream, &mut frame).await?;
            let carried = state
                .read_message(&frame[..length], &mut payload)
                .map_err(|_| refused())?;
            // A handshake carries nothing: what rides in one can be replayed.
            if carried != 0 {
                return Err(refused());
            }
        }
    }
    let peer = state
        .get_remote_static()
        .and_then(PeerKey::from_slice)
        .ok_or_else(refused)?;
    // An end that reached itself proves nothing of another.
    if peer == own.public() {
        return Err(refused());
    }
    let state = state.into_transport_mode().map_err(noise)?;
    Ok(Secure::new(stream, state, peer))
}

/// An open connection to the end whose key is `peer`: a byte stream whose
/// bytes go encrypted, in frames. Every frame that opens was sealed by the
/// end holding `peer`'s secret and this connection's keys. Until a first
/// frame has opened, the station side and a pairing guest have no proof
/// that the other end is live (see [`connect_as_station`] and
/// [`pair_as_guest`]).
pub struct Secure<S> {
    stream: S,
    state: TransportState,
    peer: PeerKey,
    /// Set once anything failed: nothing more is read or written.
    broken: bool,
    reading: Reading,
    writing: Writing,
    /// Who waits to read and who to write, woken when the connection
    /// breaks: split in two, a half that failed wakes the other.
    waiting: [Option<Waker>; 2],
}

/// The frame being read, and the data of the last one not handed out yet.
struct Reading {
    length: [u8; 2],
    length_read: usize,
    frame: Vec<u8>,
    frame_read: usize,
    /// The last frame opened: its type byte, then its data.
    opened: Vec<u8>,
    opened_length: usize,
    /// How far into `opened` has been handed out.
    given: usize,
    /// The other end finished.
    closed: bool,
}

/// The data waiting for a frame, and the sealed frame being written.
struct Writing {
    /// A type byte and the data that follows it.
    data: Vec<u8>,
    /// The sealed frame behind its length.
    sealed: Vec<u8>,
    sealed_length: usize,
    sealed_written: usize,
    /// This end said it finished.
    closed: bool,
}

impl<S> std::fmt::Debug for Secure<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Secure")
            .field("peer", &self.peer)
            .field("broken", &self.broken)
            .finish_non_exhaustive()
    }
}

impl<S> Secure<S> {
    fn new(stream: S, state: TransportState, peer: PeerKey) -> Self {
        Self {
            stream,
            state,
            peer,
            broken: false,
            reading: Reading {
                length: [0; 2],
                length_read: 0,
                frame: vec![0; FRAME],
                frame_read: 0,
                opened: vec![0; FRAME],
                opened_length: 0,
                given: 0,
                closed: false,
            },
            writing: Writing {
                data: Vec::with_capacity(DATA_PER_FRAME + 1),
                sealed: vec![0; 2 + FRAME],
                sealed_length: 0,
                sealed_written: 0,
                closed: false,
            },
            waiting: [None, None],
        }
    }

    /// The other end's key, as the handshake named it; proved live once a
    /// frame from it has opened.
    pub const fn peer(&self) -> PeerKey {
        self.peer
    }

    /// Note a failure for good, as what `result` says, waking whoever
    /// waits on either half; note a wait on half `side` (0 reads, 1 writes).
    fn kept<T>(
        &mut self,
        side: usize,
        context: &Context<'_>,
        result: Poll<io::Result<T>>,
    ) -> Poll<io::Result<T>> {
        match result {
            Poll::Ready(Err(_)) => {
                self.broken = true;
                for waiting in self.waiting.iter_mut().filter_map(Option::take) {
                    waiting.wake();
                }
            }
            Poll::Pending => self.waiting[side] = Some(context.waker().clone()),
            Poll::Ready(Ok(_)) => {}
        }
        result
    }

    /// Seal the data waiting, or a close frame, into the frame to write.
    fn seal(&mut self, kind: u8) -> io::Result<()> {
        let data = &mut self.writing.data;
        if kind == CLOSE {
            data.clear();
        }
        if data.is_empty() {
            data.push(kind);
        }
        let length = self
            .state
            .write_message(data, &mut self.writing.sealed[2..])
            .map_err(|_| io::Error::other("a frame could not be sealed"))?;
        let Ok(prefix) = u16::try_from(length) else {
            return Err(io::Error::other("a sealed frame is too long"));
        };
        self.writing.sealed[..2].copy_from_slice(&prefix.to_be_bytes());
        self.writing.sealed_length = 2 + length;
        self.writing.sealed_written = 0;
        data.clear();
        Ok(())
    }
}

impl<S: AsyncRead + Unpin> Secure<S> {
    /// Fill `buffer` from `*filled` on: whether it filled before the stream
    /// ended.
    fn poll_fill(
        stream: &mut S,
        context: &mut Context<'_>,
        buffer: &mut [u8],
        filled: &mut usize,
    ) -> Poll<io::Result<bool>> {
        while *filled < buffer.len() {
            let mut read = ReadBuf::new(&mut buffer[*filled..]);
            ready!(Pin::new(&mut *stream).poll_read(context, &mut read))?;
            if read.filled().is_empty() {
                return Poll::Ready(Ok(false));
            }
            *filled += read.filled().len();
        }
        Poll::Ready(Ok(true))
    }

    /// Read and open the next frame into the data to hand out.
    fn poll_frame(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let reading = &mut self.reading;
        if !ready!(Self::poll_fill(
            &mut self.stream,
            context,
            &mut reading.length,
            &mut reading.length_read
        ))? {
            return Poll::Ready(Err(cut()));
        }
        let length = usize::from(u16::from_be_bytes(reading.length));
        if !ready!(Self::poll_fill(
            &mut self.stream,
            context,
            &mut reading.frame[..length],
            &mut reading.frame_read
        ))? {
            return Poll::Ready(Err(cut()));
        }
        reading.length_read = 0;
        reading.frame_read = 0;
        let opened = self
            .state
            .read_message(&reading.frame[..length], &mut reading.opened)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a session frame did not open: replayed, out of order or changed",
                )
            })?;
        match reading.opened[..opened] {
            [DATA, ..] => {
                reading.opened_length = opened;
                reading.given = 1;
            }
            [CLOSE] => reading.closed = true,
            _ => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a session frame of no known type",
                )));
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl<S: AsyncWrite + Unpin> Secure<S> {
    /// Write the sealed frame, all of it.
    fn poll_sealed(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let writing = &mut self.writing;
        while writing.sealed_written < writing.sealed_length {
            let written = ready!(Pin::new(&mut self.stream).poll_write(
                context,
                &writing.sealed[writing.sealed_written..writing.sealed_length]
            ))?;
            if written == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            writing.sealed_written += written;
        }
        writing.sealed_length = 0;
        writing.sealed_written = 0;
        Poll::Ready(Ok(()))
    }

    /// Write out the frame under way, then the data waiting.
    fn poll_send(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.poll_sealed(context))?;
        if self.writing.data.len() > 1 {
            self.seal(DATA)?;
            ready!(self.poll_sealed(context))?;
        }
        Poll::Ready(Ok(()))
    }

    /// Finish: what is waiting, then the close frame, then the stream's own
    /// shutdown.
    fn poll_finish(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.writing.closed {
            ready!(self.poll_send(context))?;
            self.seal(CLOSE)?;
            self.writing.closed = true;
        }
        ready!(self.poll_sealed(context))?;
        ready!(Pin::new(&mut self.stream).poll_flush(context))?;
        Pin::new(&mut self.stream).poll_shutdown(context)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Secure<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if this.broken {
                return Poll::Ready(Err(broken()));
            }
            let reading = &mut this.reading;
            if reading.given < reading.opened_length {
                let given = (reading.opened_length - reading.given).min(buffer.remaining());
                buffer.put_slice(&reading.opened[reading.given..reading.given + given]);
                reading.given += given;
                return Poll::Ready(Ok(()));
            }
            if reading.closed {
                return Poll::Ready(Ok(()));
            }
            let framed = this.poll_frame(context);
            ready!(this.kept(READS, context, framed))?;
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Secure<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.broken || this.writing.closed {
            return Poll::Ready(Err(broken()));
        }
        // A full frame goes out before more is taken.
        if this.writing.data.len() > DATA_PER_FRAME {
            let sent = this.poll_send(context);
            ready!(this.kept(WRITES, context, sent))?;
        }
        if this.writing.data.is_empty() {
            this.writing.data.push(DATA);
        }
        let taken = data.len().min(DATA_PER_FRAME + 1 - this.writing.data.len());
        this.writing.data.extend_from_slice(&data[..taken]);
        Poll::Ready(Ok(taken))
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.broken {
            return Poll::Ready(Err(broken()));
        }
        let sent = this.poll_send(context);
        ready!(this.kept(WRITES, context, sent))?;
        let flushed = Pin::new(&mut this.stream).poll_flush(context);
        this.kept(WRITES, context, flushed)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.broken {
            return Poll::Ready(Err(broken()));
        }
        let finished = this.poll_finish(context);
        this.kept(WRITES, context, finished)
    }
}

/// Write one handshake frame: its length, then its bytes.
pub(crate) async fn write_frame<S: AsyncWrite + Unpin>(stream: &mut S, frame: &[u8]) -> Result<()> {
    let length = u16::try_from(frame.len())
        .map_err(|_| Error::new(Code::BadRequest, "A session frame is at most 65535 bytes."))?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .map_err(io_error)?;
    stream.write_all(frame).await.map_err(io_error)?;
    stream.flush().await.map_err(io_error)
}

/// Read one handshake frame into `buffer`, which holds the largest; its
/// length.
pub(crate) async fn read_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    buffer: &mut [u8],
) -> Result<usize> {
    let mut length = [0; 2];
    stream.read_exact(&mut length).await.map_err(io_error)?;
    let length = usize::from(u16::from_be_bytes(length));
    stream
        .read_exact(&mut buffer[..length])
        .await
        .map_err(io_error)?;
    Ok(length)
}

fn refused() -> Error {
    Error::new(
        Code::Identity,
        "The other end of the session did not prove the key it was paired with.",
    )
}

fn noise(error: snow::Error) -> Error {
    Error::new(Code::Io, "The session's handshake could not be made.").with_source(error)
}

fn io_error(error: io::Error) -> Error {
    Error::new(Code::Io, "The session's connection failed.").with_source(error)
}

fn cut() -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "the session's connection ended without the other end finishing",
    )
}

fn broken() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "this session connection broke earlier",
    )
}

#[cfg(test)]
#[path = "secure_tests.rs"]
mod tests;
