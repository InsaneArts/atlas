//! A native observer of the frame service, as the terminal runs it: both
//! lanes checked to be served by the descriptor's process and this account,
//! one picture per credit, and each committed before input may name it.
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
    frame_pipe::{
        diagnostics::Reason,
        model::{
            Attach, Attached, ClientKind, CreditOnly, Demand, ErrorMetadata, FrameMetadata, Hello,
            Hex32, Hex64, Longest, Metadata, Pair, Positive, Ref, Right, Role, Source, UpdateKind,
            Welcome, decode, encode,
            flow::{Barrier, Credit, Phase, Settle, State, Subscribe, SubscribeReason, Subscribed},
        },
        wire::{CHUNK, Decoder, Head, Item, Kind, Lane, encode as packet},
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path, time::Duration};

/// How long one picture may take, credit to pixels, before the observer
/// gives up: the source failing, or the service gone quiet.
const PICTURE_DEADLINE: Duration = Duration::from_secs(3);
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

/// One received packet and its whole payload.
pub(crate) struct Received {
    head: Head,
    metadata: Vec<u8>,
    payload: Option<Buffer>,
}

/// One lane and what it has read but not yet handed out. The packet being
/// read stays here, so a receive cancelled part way loses nothing.
struct Side {
    stream: UnixStream,
    decoder: Decoder,
    pending: Vec<u8>,
    partial: Option<(Head, Vec<u8>, Buffer, usize)>,
}

impl Side {
    async fn connect(path: &Path, lane: Lane, broker: u32) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let peer = stream.peer_cred()?;
        if peer.uid() != nix::unistd::getuid().as_raw()
            || peer.pid().and_then(|pid| u32::try_from(pid).ok()) != Some(broker)
        {
            return Err(Error::new(
                Code::Identity,
                "Another process answers for the frame service",
            ));
        }
        Ok(Self {
            stream,
            decoder: Decoder::new(lane),
            pending: Vec::new(),
            partial: None,
        })
    }

    async fn send<T: Serialize>(&mut self, kind: Kind, request: u64, metadata: &T) -> Result<()> {
        let metadata = encode(metadata).map_err(refused)?;
        let bytes = packet(kind, request, &metadata, 0).map_err(refused)?;
        self.stream.write_all(&bytes).await?;
        Ok(())
    }

    /// The next whole packet. Cancel-safe: what was read stays for the next
    /// call.
    async fn receive(&mut self) -> Result<Received> {
        loop {
            let mut input = self.pending.as_slice();
            let mut done = None;
            while done.is_none() {
                let Some(item) = self.decoder.next(&mut input).map_err(refused)? else {
                    break;
                };
                match item {
                    Item::Packet(head, metadata) if head.payload == 0 => {
                        done = Some(Received {
                            head,
                            metadata,
                            payload: None,
                        });
                    }
                    Item::Packet(head, metadata) => {
                        let length = usize::try_from(head.payload).map_err(|_| {
                            refused(atlas_core::frame_pipe::wire::Refusal::Oversized)
                        })?;
                        self.partial = Some((head, metadata, Buffer::zeroed(length)?, 0));
                    }
                    Item::Payload(piece) => {
                        let Some((_, _, buffer, filled)) = &mut self.partial else {
                            return Err(protocol());
                        };
                        buffer[*filled..*filled + piece.len()].copy_from_slice(&piece);
                        *filled += piece.len();
                        if *filled == buffer.len()
                            && let Some((head, metadata, buffer, _)) = self.partial.take()
                        {
                            done = Some(Received {
                                head,
                                metadata,
                                payload: Some(buffer),
                            });
                        }
                    }
                }
            }
            self.pending = input.to_vec();
            if let Some(done) = done {
                return Ok(done);
            }
            let mut buffer = vec![0; CHUNK];
            let count = self.stream.read(&mut buffer).await?;
            if count == 0 {
                return Err(Error::new(Code::Io, "The frame service closed the stream"));
            }
            self.pending.extend_from_slice(&buffer[..count]);
        }
    }
}

/// A picture read from the frame service.
pub(crate) struct Picture {
    pub(crate) metadata: FrameMetadata,
    pub(crate) rgb: Buffer,
}

/// A paired, subscribed observer.
pub(crate) struct PipeClient {
    control: Side,
    frame: Side,
    observer: Hex32,
    request: u64,
    credit: u64,
    /// The accepted profile's stream and geometry generations and its
    /// subscription's barrier.
    generations: Option<(Positive, Positive, Positive)>,
    longest: u32,
}

impl PipeClient {
    /// Pair with the frame service `descriptor` describes and subscribe for
    /// pictures at most `longest` pixels on their longest side.
    ///
    /// # Errors
    ///
    /// Returns an unreadable descriptor, a lane served by another process,
    /// or any refusal or step out of order.
    pub(crate) async fn open(descriptor: &Value, longest: u32) -> Result<Self> {
        tokio::time::timeout(PICTURE_DEADLINE, Self::pair(descriptor, longest))
            .await
            .map_err(|_| Error::new(Code::StaleFrame, "The frame service did not pair in time"))?
    }

    async fn pair(descriptor: &Value, longest: u32) -> Result<Self> {
        let text = |key: &str| descriptor[key].as_str().ok_or_else(unreadable);
        let broker = descriptor["brokerPID"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or_else(unreadable)?;
        let service = Hex32::parse(text("serviceInstance")?).ok_or_else(unreadable)?;
        let bootstrap = Hex64::parse(text("observerBootstrap")?).ok_or_else(unreadable)?;
        let mut control =
            Side::connect(Path::new(text("controlPath")?), Lane::Control, broker).await?;
        let mut frame = Side::connect(Path::new(text("framePath")?), Lane::Frame, broker).await?;
        let hello = Hello {
            protocol_major: 1,
            service_instance: service,
            bootstrap_capability: bootstrap,
            client_challenge: Hex32::random(),
            role: Role::Observer,
            client_kind: ClientKind::Terminal,
            requested_rights: vec![Right::Observe],
            offered_updates: vec![UpdateKind::Full],
        };
        control.send(Kind::Hello, 1, &hello).await?;
        let welcome: Welcome = expect(&mut control, Kind::Welcome).await?;
        if welcome.client_challenge != hello.client_challenge {
            return Err(protocol());
        }
        let attach = Attach {
            service_instance: welcome.service_instance.clone(),
            observer_id: welcome.observer_id.clone(),
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id.clone(),
            frame_challenge: Hex32::random(),
            pair_token: welcome.pair_token.clone(),
        };
        frame.send(Kind::Attach, 1, &attach).await?;
        let attached: Attached = expect(&mut frame, Kind::Attached).await?;
        if attached.frame_challenge != attach.frame_challenge {
            return Err(protocol());
        }
        let pair = Pair {
            service_instance: welcome.service_instance,
            observer_id: welcome.observer_id.clone(),
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id,
            frame_connection_id: attached.frame_connection_id,
        };
        control.send(Kind::PairConfirm, 2, &pair).await?;
        let ready: Pair = expect(&mut control, Kind::PairReady).await?;
        if ready != pair {
            return Err(protocol());
        }
        let mut client = Self {
            control,
            frame,
            observer: welcome.observer_id,
            request: 2,
            credit: 0,
            generations: None,
            longest: longest.clamp(320, 4096),
        };
        client.subscribe(SubscribeReason::First).await?;
        Ok(client)
    }

    async fn subscribe(&mut self, reason: SubscribeReason) -> Result<()> {
        let subscribe = Subscribe {
            reason,
            demand: Demand {
                desired_longest: Longest::new(self.longest).ok_or_else(protocol)?,
                backend: Source::Cg,
                hard_ceiling: None,
            },
            view_generation: Positive::new(1).ok_or_else(protocol)?,
        };
        self.request += 1;
        self.control
            .send(Kind::Subscribe, self.request, &subscribe)
            .await?;
        let subscribed: Subscribed = self.reply(Kind::Subscribed).await?;
        let profile = subscribed.profile;
        self.generations = Some((
            profile.stream_generation,
            profile.geometry_generation,
            subscribed.barrier_generation,
        ));
        Ok(())
    }

    /// The next picture: one credit and the picture it earns. An expired
    /// credit is given again; a changed profile is subscribed to again.
    ///
    /// # Errors
    ///
    /// Returns any refusal or packet out of order; the observer then ends.
    pub(crate) async fn next(&mut self) -> Result<Picture> {
        tokio::time::timeout(PICTURE_DEADLINE, self.picture())
            .await
            .map_err(|_| {
                Error::new(
                    Code::StaleFrame,
                    "The frame service sent no picture in time",
                )
            })?
    }

    async fn picture(&mut self) -> Result<Picture> {
        loop {
            if self.generations.is_none() {
                self.subscribe(SubscribeReason::Reconfigure).await?;
            }
            let (stream, geometry, _) = self.generations.ok_or_else(protocol)?;
            self.credit += 1;
            self.request += 1;
            let credit = Credit {
                credit_id: Positive::new(self.credit).ok_or_else(protocol)?,
                view_generation: Positive::new(1).ok_or_else(protocol)?,
                stream_generation: stream,
                geometry_generation: geometry,
                // Subscribed answers only once a picture begun after the
                // subscription exists, so every credit takes the latest.
                barrier: Barrier::Ordinary,
                barrier_generation: None,
                input_revision: None,
            };
            self.control
                .send(Kind::Credit, self.request, &credit)
                .await?;
            loop {
                let (lane, received) = tokio::select! {
                    received = self.control.receive() => (Lane::Control, received?),
                    received = self.frame.receive() => (Lane::Frame, received?),
                };
                match (lane, received.head.kind) {
                    (Lane::Control, Kind::CreditAccepted) => {
                        let _: CreditOnly = read(&received.metadata)?;
                    }
                    (Lane::Control, Kind::CreditExpired) => break,
                    (Lane::Control, Kind::State) => self.state(&received.metadata)?,
                    (Lane::Control, Kind::Error) => {
                        let error: ErrorMetadata = read(&received.metadata)?;
                        if self.generations.is_none()
                            && error.disposition
                                == atlas_core::frame_pipe::model::Disposition::CreditRetired
                        {
                            self.subscribe(SubscribeReason::Reconfigure).await?;
                            break;
                        }
                        return Err(refusal(error.failure.reason));
                    }
                    (Lane::Frame, Kind::FullFrame) => {
                        let metadata: FrameMetadata = read(&received.metadata)?;
                        let rgb = received.payload.ok_or_else(protocol)?;
                        metadata
                            .check(Kind::FullFrame, rgb.len() as u64)
                            .map_err(refused)?;
                        if metadata.credit_id.get() != self.credit
                            || metadata.observer_id != self.observer
                        {
                            return Err(protocol());
                        }
                        return Ok(Picture { metadata, rgb });
                    }
                    _ => return Err(protocol()),
                }
            }
        }
    }

    /// Commit `metadata`'s picture: decoded and selected, so input may name
    /// it. False when the service set it aside as too old by then: the
    /// picture is not shown, and the next one is asked for.
    ///
    /// # Errors
    ///
    /// Returns a refusal, an acknowledgement that does not echo it, or no
    /// answer in time.
    pub(crate) async fn commit(&mut self, metadata: &FrameMetadata) -> Result<bool> {
        tokio::time::timeout(PICTURE_DEADLINE, self.settle(metadata))
            .await
            .map_err(|_| Error::new(Code::StaleFrame, "The frame service did not answer in time"))?
    }

    async fn settle(&mut self, metadata: &FrameMetadata) -> Result<bool> {
        let settle = Settle {
            credit_id: metadata.credit_id,
            reference: Ref {
                service_instance: metadata.service_instance.clone(),
                observer_id: metadata.observer_id.clone(),
                observer_generation: metadata.observer_generation,
                stream_generation: metadata.stream_generation,
                geometry_generation: metadata.geometry_generation,
                source_sequence: metadata.source_sequence,
                image_sequence: metadata.image_sequence,
                frame_key: metadata.frame_key.clone(),
                view_generation: metadata.view_generation,
            },
        };
        self.request += 1;
        self.control
            .send(Kind::Decoded, self.request, &settle)
            .await?;
        let echo: Settle = self.reply(Kind::DecodedAck).await?;
        if echo != settle {
            return Err(protocol());
        }
        self.request += 1;
        self.control
            .send(Kind::Commit, self.request, &settle)
            .await?;
        loop {
            let received = self.control.receive().await?;
            match received.head.kind {
                Kind::CommitAck => {
                    let echo: Settle = read(&received.metadata)?;
                    return if echo == settle {
                        Ok(true)
                    } else {
                        Err(protocol())
                    };
                }
                Kind::Error => {
                    let error: ErrorMetadata = read(&received.metadata)?;
                    if error.disposition
                        == atlas_core::frame_pipe::model::Disposition::CreditRetired
                        && error.credit_id == Some(settle.credit_id)
                    {
                        return Ok(false);
                    }
                    return Err(refusal(error.failure.reason));
                }
                Kind::CreditAccepted => {}
                Kind::State => self.state(&received.metadata)?,
                _ => return Err(protocol()),
            }
        }
    }
}

impl PipeClient {
    /// The next reply on the control lane, which must be a `kind`. An
    /// acceptance or a state change that crosses it on the other lane's
    /// schedule is handled on the way.
    async fn reply<T: Metadata>(&mut self, kind: Kind) -> Result<T> {
        loop {
            let received = self.control.receive().await?;
            match received.head.kind {
                found if found == kind => return read(&received.metadata),
                Kind::CreditAccepted => {}
                Kind::State => self.state(&received.metadata)?,
                Kind::Error => {
                    let error: ErrorMetadata = read(&received.metadata)?;
                    return Err(refusal(error.failure.reason));
                }
                _ => return Err(protocol()),
            }
        }
    }

    /// Note a state change: a changed profile is subscribed to again before
    /// the next credit, and a failing source ends the observer.
    fn state(&mut self, metadata: &[u8]) -> Result<()> {
        let state: State = read(metadata)?;
        if state.phase == Phase::Unavailable {
            return Err(Error::new(
                Code::StaleFrame,
                "The helper's pictures are unavailable",
            ));
        }
        if state.profile.is_some() {
            self.generations = None;
        }
        Ok(())
    }
}

/// The next packet on `side`, which must be a `kind`.
async fn expect<T: Metadata>(side: &mut Side, kind: Kind) -> Result<T> {
    let received = side.receive().await?;
    if received.head.kind == Kind::Error {
        let error: ErrorMetadata = read(&received.metadata)?;
        return Err(refusal(error.failure.reason));
    }
    if received.head.kind != kind {
        return Err(protocol());
    }
    read(&received.metadata)
}

fn read<T: Metadata>(metadata: &[u8]) -> Result<T> {
    decode(metadata).map_err(refused)
}

fn refused(refusal: atlas_core::frame_pipe::wire::Refusal) -> Error {
    Error::new(
        Code::Protocol,
        format!(
            "The frame service sent bytes it should not: {}",
            json_word(refusal.reason())
        ),
    )
}

fn refusal(reason: Reason) -> Error {
    Error::new(
        Code::Protocol,
        format!("The frame service refused: {}", json_word(reason)),
    )
}

fn protocol() -> Error {
    Error::new(Code::Protocol, "The frame service answered out of order")
}

fn unreadable() -> Error {
    Error::new(
        Code::Protocol,
        "The frame service's description is unreadable",
    )
}

/// A reason's fixed word.
fn json_word(reason: Reason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
