//! One accepted socket's two halves. The reader decodes packets and hands
//! them to the broker; the writer sends what the broker queues, a picture's
//! pixels in bounded pieces straight from the shared picture. Each lane is
//! its own socket, so bulk pixels never queue control behind them.
use super::source::Picture;
use atlas_core::frame_pipe::{
    model::Positive,
    wire::{CHUNK, Decoder, Head, Item, Kind, Lane, encode},
};
use std::{sync::Arc, time::Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc,
};

/// The most packets queued for one lane's writer.
pub(super) const QUEUE: usize = 16;
/// How long one packet, a whole picture included, may take to write; a peer
/// that stops reading loses its lane rather than holding a writer.
const WRITE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);

/// A connection, by the order it was accepted.
pub(super) type Id = u64;

/// What the broker sends on a lane.
pub(super) enum Outgoing {
    Packet {
        kind: Kind,
        request: u64,
        metadata: Vec<u8>,
    },
    /// A full picture for `credit`, answering its Credit `request`.
    Frame {
        request: u64,
        credit: Positive,
        metadata: Vec<u8>,
        picture: Arc<Picture>,
    },
}

/// What a lane tells the broker.
pub(super) enum Event {
    Packet {
        connection: Id,
        head: Head,
        metadata: Vec<u8>,
    },
    /// A picture's last byte was written.
    Delivered {
        connection: Id,
        credit: Positive,
        at: Instant,
    },
    /// The lane ended or broke; the broker closes its pair.
    Ended { connection: Id },
}

/// The broker's hold on a lane. Dropping it stops reading at once; the
/// writer sends what is already queued, a closing error included, then
/// ends the socket.
pub(super) struct Socket {
    pub(super) queue: mpsc::Sender<Outgoing>,
    reader: tokio::task::JoinHandle<()>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// Start `stream`'s halves as connection `connection` on `lane`.
pub(super) fn start(
    stream: UnixStream,
    lane: Lane,
    connection: Id,
    events: mpsc::Sender<Event>,
) -> Socket {
    let (read, write) = stream.into_split();
    let (queue, outgoing) = mpsc::channel(QUEUE);
    Socket {
        queue,
        reader: {
            tokio::spawn(write_lane(write, connection, outgoing, events.clone()));
            tokio::spawn(read_lane(read, lane, connection, events))
        },
    }
}

async fn read_lane(
    mut read: OwnedReadHalf,
    lane: Lane,
    connection: Id,
    events: mpsc::Sender<Event>,
) {
    let mut decoder = Decoder::new(lane);
    let mut buffer = vec![0; CHUNK];
    'reading: while let Ok(count @ 1..) = read.read(&mut buffer).await {
        let mut input = &buffer[..count];
        loop {
            match decoder.next(&mut input) {
                Ok(None) => break,
                Ok(Some(Item::Packet(head, metadata))) => {
                    let packet = Event::Packet {
                        connection,
                        head,
                        metadata,
                    };
                    if events.send(packet).await.is_err() {
                        return;
                    }
                }
                // Clients send no pictures, and no refused lane resumes.
                Ok(Some(Item::Payload(_))) | Err(_) => break 'reading,
            }
        }
    }
    let _ = events.send(Event::Ended { connection }).await;
}

async fn write_lane(
    mut write: OwnedWriteHalf,
    connection: Id,
    mut outgoing: mpsc::Receiver<Outgoing>,
    events: mpsc::Sender<Event>,
) {
    while let Some(item) = outgoing.recv().await {
        let writing = write_item(&mut write, connection, item, &events);
        if !matches!(
            tokio::time::timeout(WRITE_DEADLINE, writing).await,
            Ok(Ok(()))
        ) {
            break;
        }
    }
    let _ = write.shutdown().await;
    let _ = events.send(Event::Ended { connection }).await;
}

/// Write one queued packet whole; a picture's pixels go in bounded pieces
/// straight from the shared picture, and its last byte is reported.
async fn write_item(
    write: &mut OwnedWriteHalf,
    connection: Id,
    item: Outgoing,
    events: &mpsc::Sender<Event>,
) -> std::io::Result<()> {
    match item {
        Outgoing::Packet {
            kind,
            request,
            metadata,
        } => {
            let bytes = encode(kind, request, &metadata, 0)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
            write.write_all(&bytes).await
        }
        Outgoing::Frame {
            request,
            credit,
            metadata,
            picture,
        } => {
            let pixels = picture.pixels.as_slice();
            let head = encode(Kind::FullFrame, request, &metadata, pixels.len() as u64)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
            write.write_all(&head).await?;
            for piece in pixels.chunks(CHUNK) {
                write.write_all(piece).await?;
            }
            let delivered = Event::Delivered {
                connection,
                credit,
                at: Instant::now(),
            };
            let _ = events.send(delivered).await;
            Ok(())
        }
    }
}
