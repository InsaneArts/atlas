//! The station's end of one generation's control connection, as the station
//! holds it: opened within [`OPENING`], what the guest's hello names
//! settled at once, and what the guest says told to the connection's owner
//! as [`Event`]s, never counted here. An owner that falls [`EVENTS`] behind
//! loses the generation rather than any event.
//!
//! The person's Stops and Resumes are numbered in the generation and said as
//! such only to a guest that acknowledges them; each is answered once the
//! guest carried it out. Said to a guest that does not, or lost with its
//! generation before the answer came, one is unconfirmed: a later read of
//! the guest's own state is what tells.
use crate::{
    compatibility::Declared,
    control::{self, GuestStop, Outbox, Outcome, Said, Station},
    credit::Credit,
    generation::Generation,
    keys::Keypair,
    mirror::{Endpoint, Refused},
    secure::{Purpose, connect_as_station},
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
    session::key::PeerKey,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, BufReader},
    sync::{Notify, mpsc, oneshot},
    task::AbortHandle,
};

/// The longest the handshake and the guest's hello may take together: the
/// guest's own bound on a connection's opening.
pub const OPENING: Duration = Duration::from_secs(10);

/// What the guest said, or what became of its connection, in the order it
/// came.
#[derive(Debug)]
pub enum Event {
    /// Nothing came for [`control::SILENCE`].
    Silent,
    /// The guest is heard again in the same generation.
    HeardAgain,
    /// What the Stop or Resume numbered `seq` came to.
    Applied { seq: u64, outcome: Outcome },
    /// What of mirror list `list` the guest could not listen on.
    Taken { list: u64, refused: Vec<Refused> },
    /// The guest accepted a mirrored connection and asks for its lane.
    Connect {
        connection: u64,
        endpoint: Endpoint,
        list: u64,
    },
    /// The connection ended, and why: the generation is gone for the
    /// station, and nothing more comes.
    Lost(Error),
}

/// Where a connection's events come, until it is lost.
pub type Events = mpsc::Receiver<Event>;

/// The most events waiting for the owner: one that falls further behind
/// loses the generation.
pub const EVENTS: usize = 256;

/// What a Stop or Resume the station said came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The guest answered it.
    Applied(Outcome),
    /// No answer will come: the guest does not acknowledge, or the
    /// generation was lost first.
    Unconfirmed,
}

/// A Stop or Resume said on control, and its answer to come.
#[derive(Debug)]
pub struct Asked {
    seq: Option<u64>,
    answer: Option<oneshot::Receiver<Outcome>>,
}

impl Asked {
    /// Its number in the generation; none when said unnumbered.
    #[must_use]
    pub const fn seq(&self) -> Option<u64> {
        self.seq
    }

    /// The guest's answer once it comes, or unconfirmed once none can.
    pub async fn answer(self) -> Answer {
        match self.answer {
            Some(answer) => answer.await.map_or(Answer::Unconfirmed, Answer::Applied),
            None => Answer::Unconfirmed,
        }
    }
}

/// The numbers given in a generation, and the answers still awaited.
#[derive(Debug, Default)]
struct Numbering(Mutex<Numbered>);

#[derive(Debug, Default)]
struct Numbered {
    last: u64,
    waiting: BTreeMap<u64, oneshot::Sender<Outcome>>,
    lost: bool,
}

impl Numbering {
    fn lock(&self) -> MutexGuard<'_, Numbered> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn applied(&self, seq: u64, outcome: Outcome) {
        if let Some(waiting) = self.lock().waiting.remove(&seq) {
            let _ = waiting.send(outcome);
        }
    }

    /// The generation is gone: every answer awaited is unconfirmed.
    fn lose(&self) {
        let mut numbered = self.lock();
        numbered.lost = true;
        numbered.waiting.clear();
    }
}

/// What the reader hears, handed to the owner.
struct Told {
    events: mpsc::Sender<Event>,
    numbering: Arc<Numbering>,
    /// Told once the owner fell [`EVENTS`] behind.
    behind: Arc<Notify>,
}

impl Told {
    /// Hand `event` on; an owner too far behind to take it loses the
    /// generation, so nothing it was told is ever missing.
    fn tell(&self, event: Event) {
        if self.events.try_send(event).is_err() {
            self.behind.notify_one();
        }
    }
}

impl Station for Told {
    fn silent(&self) {
        self.tell(Event::Silent);
    }

    fn heard_again(&self) {
        self.tell(Event::HeardAgain);
    }

    fn taken(&self, list: u64, refused: Vec<Refused>) {
        self.tell(Event::Taken { list, refused });
    }

    fn connect(&self, connection: u64, endpoint: Endpoint, list: u64) {
        self.tell(Event::Connect {
            connection,
            endpoint,
            list,
        });
    }

    fn applied(&self, seq: u64, outcome: Outcome) {
        self.numbering.applied(seq, outcome);
        self.tell(Event::Applied { seq, outcome });
    }
}

/// The station's end of a live generation's control connection. Dropping
/// it ends the connection, as the station going would.
#[derive(Debug)]
pub struct Control {
    generation: Generation,
    unsettled: Vec<Token>,
    declared: Declared,
    outbox: Arc<Outbox>,
    credit: Arc<Credit>,
    numbering: Arc<Numbering>,
    tasks: Vec<AbortHandle>,
}

impl Drop for Control {
    fn drop(&mut self) {
        self.retire();
    }
}

/// Answer, as the station with key pair `own`, a control connection from
/// the desktop whose key is `key` on `stream`. Of the changes the guest's
/// hello names, those `claimed` holds no claim on are settled at once.
///
/// # Errors
///
/// Fails when the connection does, its first message is not a hello, the
/// guest speaks another protocol, or the handshake and hello take longer
/// than [`OPENING`].
pub async fn open<S>(
    stream: S,
    own: &Keypair,
    key: PeerKey,
    desktop: &DesktopId,
    claimed: impl Fn(&Token) -> bool,
) -> Result<(Control, Events)>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let opening = async {
        let secure = connect_as_station(stream, own, key, desktop, Purpose::Control).await?;
        let (reading, writing) = tokio::io::split(secure);
        let mut reading = BufReader::new(reading);
        // A guest of another protocol is refused here, before anything is
        // said to it: no generation of it is ever live for the station.
        let hello = control::hello(&mut reading).await?;
        Ok::<_, Error>((reading, writing, hello))
    };
    let (reading, writing, hello) =
        tokio::time::timeout(OPENING, opening).await.map_err(|_| {
            Error::new(
                Code::Transport,
                "The desktop did not open its control connection in time.",
            )
        })??;
    let control::Hello {
        generation,
        unsettled,
        declared,
    } = hello;
    let outbox = Arc::new(Outbox::default());
    let settling = control::settling(&unsettled, claimed);
    if !settling.is_empty() {
        outbox.say(Said::Settled { changes: settling })?;
    }
    let (credit, numbering) = (Arc::new(Credit::default()), Arc::new(Numbering::default()));
    let (events, heard) = mpsc::channel(EVENTS);
    let sending = tokio::spawn(control::send(writing, outbox.clone()));
    let behind = Arc::new(Notify::new());
    let told = Told {
        events: events.clone(),
        numbering: numbering.clone(),
        behind: behind.clone(),
    };
    let (credit_read, closing) = (credit.clone(), outbox.clone());
    let reading = tokio::spawn(async move {
        let ended = tokio::select! {
            ended = control::station_reads(reading, &credit_read, &told) => ended,
            () = behind.notified() => {
                // Given up whole: nothing more is said or heard on it.
                closing.close();
                Err(Error::new(
                    Code::Transport,
                    "The station fell behind the desktop's control connection and gave it up.",
                ))
            }
        };
        told.numbering.lose();
        let why = ended.err().unwrap_or_else(|| {
            Error::new(
                Code::Transport,
                "The control connection to the desktop ended.",
            )
        });
        // Waits for room: the loss is the one event never given up.
        let _ = events.send(Event::Lost(why)).await;
    });
    let control = Control {
        generation,
        unsettled,
        declared,
        outbox,
        credit,
        numbering,
        tasks: vec![sending.abort_handle(), reading.abort_handle()],
    };
    Ok((control, heard))
}

impl Control {
    /// Leave the generation now, whoever still holds this end: its heartbeats
    /// and reading stop, nothing more is said, and every answer awaited is
    /// unconfirmed. The guest hears the connection go.
    pub fn retire(&self) {
        self.outbox.close();
        self.numbering.lose();
        for task in &self.tasks {
            task.abort();
        }
    }

    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// What the guest's hello named as done and unsettled.
    #[must_use]
    pub fn unsettled(&self) -> &[Token] {
        &self.unsettled
    }

    /// What the guest declared it speaks and serves.
    #[must_use]
    pub const fn declared(&self) -> &Declared {
        &self.declared
    }

    /// Where what the station says goes, for the generation's lanes.
    #[must_use]
    pub const fn outbox(&self) -> &Arc<Outbox> {
        &self.outbox
    }

    /// The credit the generation's bulk lanes spend.
    #[must_use]
    pub const fn credit(&self) -> &Arc<Credit> {
        &self.credit
    }

    /// Say `said` from the station.
    ///
    /// # Errors
    ///
    /// Fails once the connection closed, or as [`Outbox::say`] refuses.
    pub fn say(&self, said: Said) -> Result<()> {
        self.outbox.say(said)
    }

    /// Say the person's Stop.
    ///
    /// # Errors
    ///
    /// Fails once the connection is lost, or its outbox closed: nothing
    /// was said.
    pub fn stop(&self) -> Result<Asked> {
        self.ask(Said::stop_to)
    }

    /// Say the person's Resume.
    ///
    /// # Errors
    ///
    /// Fails once the connection is lost, or its outbox closed: nothing
    /// was said.
    pub fn resume(&self) -> Result<Asked> {
        self.ask(Said::resume_to)
    }

    /// Say a Resume under the change `change`, which the guest keeps it
    /// under, and what it came to, until the station settles it; for the
    /// guest's own Stop `only` alone, which it lifts only while its Stop is
    /// still that one.
    ///
    /// # Errors
    ///
    /// Refuses a guest that does not keep a Resume's outcome, before
    /// anything is said; fails as [`Control::resume`] does.
    pub fn resume_as(&self, change: &Token, only: &GuestStop) -> Result<Asked> {
        if let Some(refused) = crate::compatibility::judge(
            &self.declared,
            &[
                crate::compatibility::ACKNOWLEDGED,
                crate::compatibility::RESUME_OUTCOME,
            ],
        )
        .refusal()
        {
            return Err(refused);
        }
        self.ask(|_, seq| Said::Lift {
            seq,
            change: Some(change.clone()),
            only: Some(only.clone()),
        })
    }

    /// Say what `numbered` makes of the next number: numbered, its answer
    /// awaited before it goes, for a guest that acknowledges; unnumbered
    /// and unconfirmed otherwise.
    fn ask(&self, numbered: impl FnOnce(&Declared, u64) -> Said) -> Result<Asked> {
        let mut state = self.numbering.lock();
        if state.lost {
            return Err(Error::new(
                Code::Transport,
                "The control connection to the desktop ended.",
            ));
        }
        if !self.declared.acknowledges() {
            self.outbox.say(numbered(&self.declared, 0))?;
            return Ok(Asked {
                seq: None,
                answer: None,
            });
        }
        let seq = state.last + 1;
        let (answered, answer) = oneshot::channel();
        // Awaited before it is said, so an answer that comes at once is
        // never missed; taken back when it cannot be said.
        state.waiting.insert(seq, answered);
        if let Err(error) = self.outbox.say(numbered(&self.declared, seq)) {
            state.waiting.remove(&seq);
            return Err(error);
        }
        state.last = seq;
        Ok(Asked {
            seq: Some(seq),
            answer: Some(answer),
        })
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
