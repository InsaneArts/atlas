use super::*;
use atlas_core::{desktop::Desktop, user::User};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering::SeqCst},
};
use tokio::io::{DuplexStream, duplex};

/// A station's record kept in memory, whose writes can be made to fail,
/// or to fail only for one state, or to wait while the test looks.
#[derive(Default)]
struct Kept {
    record: Mutex<Option<Enrolment>>,
    failing: AtomicBool,
    /// Writes of a paired record fail: the station goes before keeping it.
    pairing_fails: AtomicBool,
    damaged: AtomicBool,
    /// Held while a pending record is being kept.
    keeping: Option<std::sync::Arc<std::sync::Barrier>>,
}

impl StationRecords for Kept {
    fn read(&self) -> Result<Option<Enrolment>> {
        if self.damaged.load(SeqCst) {
            return Err(Error::new(Code::Config, "The pairing record is damaged."));
        }
        Ok(self.record.lock().unwrap().clone())
    }

    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()> {
        if let (Some(keeping), Enrolment::Pending { .. }) = (&self.keeping, record) {
            keeping.wait();
            keeping.wait();
        }
        let failing = self.failing.load(SeqCst)
            || (self.pairing_fails.load(SeqCst) && matches!(record, Enrolment::Paired(_)));
        if failing {
            return Err(Error::new(Code::Io, "The disk is full."));
        }
        let mut kept = self.record.lock().unwrap();
        if kept.as_ref() != expected {
            return Err(Error::new(Code::Busy, "The pairing changed meanwhile."));
        }
        *kept = Some(record.clone());
        Ok(())
    }
}

impl Kept {
    fn now(&self) -> Option<Enrolment> {
        self.record.lock().unwrap().clone()
    }

    /// Keep `record` as if a station before had.
    fn keep(&self, record: Enrolment) {
        *self.record.lock().unwrap() = Some(record);
    }
}

/// A guest's side kept in memory, whose commit can be made to fail.
struct Guest {
    committed: Mutex<Option<(PeerKey, Token)>>,
    secret: Mutex<Option<[u8; 32]>>,
    failing: AtomicBool,
    forgetting_fails: AtomicBool,
}

impl Guest {
    fn holding(secret: &PairingSecret) -> Self {
        Self {
            committed: Mutex::new(None),
            secret: Mutex::new(Some(*secret.as_bytes())),
            failing: AtomicBool::new(false),
            forgetting_fails: AtomicBool::new(false),
        }
    }
}

impl GuestRecords for Guest {
    fn committed(&self) -> Result<Option<(PeerKey, Token)>> {
        Ok(self.committed.lock().unwrap().clone())
    }

    fn commit(&self, station: PeerKey, change: &Token) -> Result<()> {
        if self.failing.load(SeqCst) {
            return Err(Error::new(Code::Io, "The guest's disk is full."));
        }
        let mut committed = self.committed.lock().unwrap();
        match &*committed {
            None => *committed = Some((station, change.clone())),
            Some((kept, kept_change)) if *kept == station && kept_change == change => {}
            Some(_) => return Err(Error::new(Code::Identity, "Committed to another station.")),
        }
        Ok(())
    }

    fn secret(&self) -> Result<Option<PairingSecret>> {
        Ok(self.secret.lock().unwrap().map(PairingSecret::from_bytes))
    }

    fn forget_secret(&self) -> Result<()> {
        if self.forgetting_fails.load(SeqCst) {
            return Err(Error::new(Code::Io, "The secret could not be removed."));
        }
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
}

/// Everything one pairing needs.
struct Setup {
    station: Keypair,
    guest: Keypair,
    secret: PairingSecret,
    desktop: DesktopId,
    change: Token,
    facts: GuestFacts,
}

fn setup() -> Setup {
    Setup {
        station: Keypair::generate().unwrap(),
        guest: Keypair::generate().unwrap(),
        secret: PairingSecret::generate().unwrap(),
        desktop: Desktop::from(User { uid: 502 }).id(),
        change: Token::new(),
        facts: GuestFacts {
            uid: 501,
            boot: "boot-1".to_owned(),
        },
    }
}

fn stored(station: &Arc<Kept>) -> Arc<dyn StationRecords> {
    station.clone()
}

fn kept_by(guest: &Arc<Guest>) -> Arc<dyn GuestRecords> {
    guest.clone()
}

fn pipe() -> (DuplexStream, DuplexStream) {
    duplex(1 << 16)
}

/// One pairing attempt, both ends at once.
async fn attempt(
    with: &Setup,
    secret: &PairingSecret,
    station: &Arc<Kept>,
    guest: &Arc<Guest>,
) -> (Result<Paired>, Result<PeerKey>) {
    let (station, guest) = (stored(station), kept_by(guest));
    let (station, guest) = (&station, &guest);
    let (guest_end, station_end) = pipe();
    tokio::join!(
        station_pairs(
            station_end,
            &with.station,
            secret,
            &with.desktop,
            &with.change,
            station
        ),
        guest_pairs(
            guest_end,
            &with.guest,
            &with.desktop,
            &with.change,
            &with.facts,
            guest
        ),
    )
}

/// The station completing over pinned keys while the committed guest acks.
async fn completion(
    with: &Setup,
    station: &Arc<Kept>,
    guest: &Arc<Guest>,
) -> (Result<Paired>, Result<PeerKey>) {
    let (station, guest) = (stored(station), kept_by(guest));
    let (station, guest) = (&station, &guest);
    let (guest_end, station_end) = pipe();
    tokio::join!(
        station_completes(
            station_end,
            &with.station,
            &with.desktop,
            &with.change,
            station
        ),
        guest_pairs(
            guest_end,
            &with.guest,
            &with.desktop,
            &with.change,
            &with.facts,
            guest
        ),
    )
}

#[tokio::test]
async fn a_pairing_completes_and_each_end_keeps_the_others_key() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );

    let (paired, station_key) = attempt(&with, &with.secret, &station, &guest).await;

    let paired = paired.unwrap();
    assert_eq!(paired.guest, with.guest.public());
    assert_eq!(paired.station, with.station.public().fingerprint());
    assert_eq!((paired.uid, paired.boot.as_str()), (501, "boot-1"));
    assert_eq!(station.now(), Some(Enrolment::Paired(paired)));
    assert_eq!(station_key.unwrap(), with.station.public());
    assert_eq!(
        *guest.committed.lock().unwrap(),
        Some((with.station.public(), with.change.clone()))
    );
    assert!(
        guest.secret().unwrap().is_none(),
        "the secret outlived the pairing"
    );
    assert_eq!(admitted(&*station).unwrap(), Some(with.guest.public()));
}

#[tokio::test]
async fn a_guest_without_the_secret_is_refused_and_commits_nothing() {
    let with = setup();
    let other = PairingSecret::generate().unwrap();
    let (station, guest) = (Arc::new(Kept::default()), Arc::new(Guest::holding(&other)));

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert_eq!(paired.unwrap_err().code, Code::Identity);
    assert!(committed.is_err());
    assert_eq!(station.now(), None);
    assert!(guest.committed.lock().unwrap().is_none());
}

#[tokio::test]
async fn a_station_without_the_secret_gets_no_commitment() {
    let with = setup();
    let other = PairingSecret::generate().unwrap();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );

    let (paired, committed) = attempt(&with, &other, &station, &guest).await;

    assert!(paired.is_err());
    assert!(committed.is_err());
    assert!(guest.committed.lock().unwrap().is_none());
    assert!(
        guest.secret().unwrap().is_some(),
        "the secret went without a pairing"
    );
}

#[tokio::test]
async fn a_guest_offering_another_enrolment_is_refused() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    let (guest_end, station_end) = pipe();
    let another = Token::new();
    let (records, guest_records) = (stored(&station), kept_by(&guest));

    let (paired, committed) = tokio::join!(
        station_pairs(
            station_end,
            &with.station,
            &with.secret,
            &with.desktop,
            &with.change,
            &records
        ),
        guest_pairs(
            guest_end,
            &with.guest,
            &with.desktop,
            &another,
            &with.facts,
            &guest_records
        ),
    );

    assert_eq!(paired.unwrap_err().code, Code::Identity);
    assert!(committed.is_err());
    assert_eq!(station.now(), None);
}

#[tokio::test]
async fn a_pending_enrolment_that_cannot_be_kept_is_never_confirmed() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    station.failing.store(true, SeqCst);

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert_eq!(paired.unwrap_err().code, Code::Io);
    assert!(committed.is_err());
    assert!(guest.committed.lock().unwrap().is_none());
}

#[tokio::test]
async fn a_guest_that_crashed_before_it_committed_pairs_again_with_the_same_secret() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    guest.failing.store(true, SeqCst);

    let (first, _) = attempt(&with, &with.secret, &station, &guest).await;
    assert!(first.is_err());
    assert!(matches!(station.now(), Some(Enrolment::Pending { .. })));

    guest.failing.store(false, SeqCst);
    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert_eq!(paired.unwrap().guest, with.guest.public());
    assert_eq!(committed.unwrap(), with.station.public());
}

#[tokio::test]
async fn a_station_that_crashed_before_its_confirmation_pairs_again() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    // What the station kept before it went: the guest's key, pending.
    station.keep(Enrolment::Pending {
        change: with.change.clone(),
        guest: with.guest.public(),
    });

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert!(paired.is_ok());
    assert!(committed.is_ok());
}

#[tokio::test]
async fn a_newly_authenticated_guest_takes_the_place_of_the_one_pending() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    let earlier = Keypair::generate().unwrap();
    station.keep(Enrolment::Pending {
        change: with.change.clone(),
        guest: earlier.public(),
    });

    let (paired, _) = attempt(&with, &with.secret, &station, &guest).await;

    assert_eq!(paired.unwrap().guest, with.guest.public());
}

#[tokio::test]
async fn a_station_that_crashed_after_the_guest_committed_completes_over_pinned_keys() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    // The guest committed and forgot its secret; the station went before
    // it kept the pairing.
    station.keep(Enrolment::Pending {
        change: with.change.clone(),
        guest: with.guest.public(),
    });
    guest.commit(with.station.public(), &with.change).unwrap();
    guest.forget_secret().unwrap();

    let (paired, committed) = completion(&with, &station, &guest).await;

    assert_eq!(paired.unwrap().guest, with.guest.public());
    assert_eq!(committed.unwrap(), with.station.public());
    assert!(matches!(station.now(), Some(Enrolment::Paired(_))));
}

#[tokio::test]
async fn an_ack_that_comes_again_changes_nothing() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    let (first, _) = attempt(&with, &with.secret, &station, &guest).await;
    let first = first.unwrap();

    let (again, _) = completion(&with, &station, &guest).await;

    assert_eq!(again.unwrap(), first);
    assert_eq!(station.now(), Some(Enrolment::Paired(first)));
}

#[tokio::test]
async fn a_commitment_that_outlived_its_secrets_removal_finishes_it() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    station.keep(Enrolment::Pending {
        change: with.change.clone(),
        guest: with.guest.public(),
    });
    guest.commit(with.station.public(), &with.change).unwrap();

    let (paired, _) = completion(&with, &station, &guest).await;

    assert!(paired.is_ok());
    assert!(guest.secret().unwrap().is_none());
}

#[tokio::test]
async fn a_paired_desktop_is_never_paired_again_on_its_own() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    attempt(&with, &with.secret, &station, &guest)
        .await
        .0
        .unwrap();
    let kept = station.now();

    let again = Arc::new(Guest::holding(&with.secret));
    let (paired, _) = attempt(&with, &with.secret, &station, &again).await;

    assert_eq!(paired.unwrap_err().code, Code::Identity);
    assert_eq!(station.now(), kept);
}

#[tokio::test]
async fn a_revoked_pairing_admits_nothing_and_is_never_completed() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    attempt(&with, &with.secret, &station, &guest)
        .await
        .0
        .unwrap();

    revoke(&*station).unwrap();

    assert_eq!(admitted(&*station).unwrap(), None);
    let (paired, _) = completion(&with, &station, &guest).await;
    assert_eq!(paired.unwrap_err().code, Code::Identity);
}

#[tokio::test]
async fn a_damaged_record_refuses_rather_than_reads_as_none() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    station.damaged.store(true, SeqCst);

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert!(paired.is_err());
    assert!(committed.is_err());
    assert!(admitted(&*station).is_err());
}

#[tokio::test]
async fn another_enrolment_pending_is_neither_replaced_nor_completed() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    let other = Token::new();
    station.keep(Enrolment::Pending {
        change: other.clone(),
        guest: with.guest.public(),
    });

    let (paired, _) = attempt(&with, &with.secret, &station, &guest).await;
    assert_eq!(paired.unwrap_err().code, Code::Identity);

    // A late ack for this enrolment never completes the other one.
    guest.commit(with.station.public(), &with.change).unwrap();
    let (completed, _) = completion(&with, &station, &guest).await;
    assert!(completed.is_err());
    assert_eq!(
        station.now(),
        Some(Enrolment::Pending {
            change: other,
            guest: with.guest.public()
        })
    );
}

#[tokio::test]
async fn revoking_a_damaged_record_refuses_and_leaves_it_as_it_is() {
    let station = Arc::new(Kept::default());
    let kept = Enrolment::Revoked {
        change: Token::new(),
    };
    station.keep(kept.clone());
    station.damaged.store(true, SeqCst);

    assert!(revoke(&*station).is_err());

    station.damaged.store(false, SeqCst);
    assert_eq!(station.now(), Some(kept));
}

#[tokio::test]
async fn an_ack_too_long_to_be_read_is_found_out_before_anything_is_committed() {
    let mut with = setup();
    with.facts.boot = "x".repeat(LINE);
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );

    let (_, committed) = attempt(&with, &with.secret, &station, &guest).await;

    assert_eq!(committed.unwrap_err().code, Code::Protocol);
    assert!(guest.committed.lock().unwrap().is_none());
    assert!(guest.secret().unwrap().is_some());
}

/// A stream that keeps a copy of everything written to it.
struct Tapped {
    inner: DuplexStream,
    written: Arc<Mutex<Vec<u8>>>,
}

impl tokio::io::AsyncRead for Tapped {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl tokio::io::AsyncWrite for Tapped {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let polled = std::pin::Pin::new(&mut self.inner).poll_write(context, data);
        if let std::task::Poll::Ready(Ok(written)) = polled {
            self.written
                .lock()
                .unwrap()
                .extend_from_slice(&data[..written]);
        }
        polled
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

/// How many whole frames `bytes` hold, each behind its two-byte length.
fn frames(bytes: &[u8]) -> usize {
    let (mut at, mut count) = (0, 0);
    while at + 2 <= bytes.len() {
        let length = usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
        if at + 2 + length > bytes.len() {
            break;
        }
        at += 2 + length;
        count += 1;
    }
    count
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_station_sends_no_confirmation_until_the_pending_record_is_kept() {
    let with = Arc::new(setup());
    let keeping = Arc::new(std::sync::Barrier::new(2));
    let station = Arc::new(Kept {
        keeping: Some(keeping.clone()),
        ..Kept::default()
    });
    let guest = Arc::new(Guest::holding(&with.secret));
    let (guest_end, station_end) = pipe();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let station_end = Tapped {
        inner: station_end,
        written: sent.clone(),
    };
    let pairing = {
        let (with, records) = (with.clone(), stored(&station));
        tokio::spawn(async move {
            station_pairs(
                station_end,
                &with.station,
                &with.secret,
                &with.desktop,
                &with.change,
                &records,
            )
            .await
        })
    };
    let committing = {
        let (with, records) = (with.clone(), kept_by(&guest));
        tokio::spawn(async move {
            guest_pairs(
                guest_end,
                &with.guest,
                &with.desktop,
                &with.change,
                &with.facts,
                &records,
            )
            .await
        })
    };

    // The station is keeping the pending record: all it has sent is its
    // one handshake message, and the guest has committed nothing.
    let waiting = keeping.clone();
    tokio::task::spawn_blocking(move || waiting.wait())
        .await
        .unwrap();
    assert_eq!(
        frames(&sent.lock().unwrap()),
        1,
        "sent more than its handshake"
    );
    assert!(guest.committed.lock().unwrap().is_none());
    tokio::task::spawn_blocking(move || keeping.wait())
        .await
        .unwrap();

    assert!(pairing.await.unwrap().is_ok());
    assert!(committing.await.unwrap().is_ok());
    assert!(frames(&sent.lock().unwrap()) > 1);
}

#[tokio::test]
async fn a_guest_committed_to_one_station_is_never_turned_to_another() {
    let with = setup();
    let guest = Guest::holding(&with.secret);
    let (one, other) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());

    guest.commit(one.public(), &with.change).unwrap();
    guest.commit(one.public(), &with.change).unwrap();

    assert!(guest.commit(other.public(), &with.change).is_err());
    assert!(guest.commit(one.public(), &Token::new()).is_err());
    assert_eq!(
        *guest.committed.lock().unwrap(),
        Some((one.public(), with.change.clone()))
    );
}

#[tokio::test]
async fn a_station_that_could_not_keep_the_pairing_completes_it_later() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    station.pairing_fails.store(true, SeqCst);

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;
    assert!(paired.is_err());
    assert!(committed.is_ok(), "the guest committed on the confirmation");
    assert!(matches!(station.now(), Some(Enrolment::Pending { .. })));

    station.pairing_fails.store(false, SeqCst);
    let (completed, _) = completion(&with, &station, &guest).await;
    assert_eq!(completed.unwrap().guest, with.guest.public());
}

#[tokio::test]
async fn a_guest_that_could_not_forget_its_secret_finishes_on_the_next_connection() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    guest.forgetting_fails.store(true, SeqCst);

    let (first, committed) = attempt(&with, &with.secret, &station, &guest).await;
    assert!(first.is_err());
    assert!(committed.is_err());
    assert!(guest.committed.lock().unwrap().is_some());

    guest.forgetting_fails.store(false, SeqCst);
    let (completed, _) = completion(&with, &station, &guest).await;
    assert!(completed.is_ok());
    assert!(guest.secret().unwrap().is_none());
}

#[tokio::test]
async fn a_guest_gone_after_committing_and_a_station_gone_before_its_ack_still_complete() {
    let with = setup();
    let (station, guest) = (
        Arc::new(Kept::default()),
        Arc::new(Guest::holding(&with.secret)),
    );
    // The guest goes after it committed, before it acked; the station,
    // waiting for that ack, goes with only the pending record kept.
    guest.forgetting_fails.store(true, SeqCst);

    let (paired, committed) = attempt(&with, &with.secret, &station, &guest).await;
    assert!(paired.is_err() && committed.is_err());
    assert!(matches!(station.now(), Some(Enrolment::Pending { .. })));

    guest.forgetting_fails.store(false, SeqCst);
    let (completed, acked) = completion(&with, &station, &guest).await;
    assert!(completed.is_ok() && acked.is_ok());
}
