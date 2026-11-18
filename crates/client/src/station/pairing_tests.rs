use super::*;
use crate::station::enrolment::{Enrolling, Enrolments};
use atlas_core::session::key::PeerKey;
use atlas_session::{
    keys::PairingSecret,
    pairing::{GuestFacts, GuestRecords, guest_pairs},
    standin::{
        StandIn,
        link::{Conditions, Connection, Link},
    },
    station::{Dialing, Raw},
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering::SeqCst},
};

/// The station's record in memory, whose paired write fails while `fails`
/// says, as a station that went before keeping it.
#[derive(Default)]
struct Kept {
    record: Mutex<Option<Enrolment>>,
    fails: AtomicUsize,
}

impl StationRecords for Kept {
    fn read(&self) -> Result<Option<Enrolment>> {
        Ok(self.record.lock().unwrap().clone())
    }

    fn replace(&self, expected: Option<&Enrolment>, record: &Enrolment) -> Result<()> {
        if matches!(record, Enrolment::Paired(_))
            && self
                .fails
                .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
                .is_ok()
        {
            return Err(Error::new(Code::Io, "The station went."));
        }
        let mut kept = self.record.lock().unwrap();
        if kept.as_ref() != expected {
            return Err(Error::new(Code::Busy, "Changed meanwhile."));
        }
        *kept = Some(record.clone());
        Ok(())
    }
}

/// The guest's side of its pairing, in memory.
struct Held {
    committed: Mutex<Option<(PeerKey, Token)>>,
    secret: Mutex<Option<[u8; 32]>>,
}

impl GuestRecords for Held {
    fn committed(&self) -> Result<Option<(PeerKey, Token)>> {
        Ok(self.committed.lock().unwrap().clone())
    }

    fn commit(&self, station: PeerKey, change: &Token) -> Result<()> {
        let mut committed = self.committed.lock().unwrap();
        match &*committed {
            None => *committed = Some((station, change.clone())),
            Some(kept) if *kept == (station, change.clone()) => {}
            Some(_) => return Err(Error::new(Code::Identity, "Committed elsewhere.")),
        }
        Ok(())
    }

    fn secret(&self) -> Result<Option<PairingSecret>> {
        Ok(self.secret.lock().unwrap().map(PairingSecret::from_bytes))
    }

    fn forget_secret(&self) -> Result<()> {
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
}

/// Enrolments in memory.
#[derive(Default)]
struct Memory(Mutex<Option<([u8; 32], Token)>>);

impl Enrolments for Memory {
    fn keep(&self, _: &DesktopId, enrolling: &Enrolling) -> Result<()> {
        *self.0.lock().unwrap() = Some((*enrolling.secret.as_bytes(), enrolling.change.clone()));
        Ok(())
    }

    fn kept(&self, _: &DesktopId) -> Result<Option<Enrolling>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .clone()
            .map(|(secret, change)| Enrolling {
                change,
                secret: PairingSecret::from_bytes(secret),
            }))
    }

    fn forget(&self, _: &DesktopId) -> Result<bool> {
        Ok(self.0.lock().unwrap().take().is_some())
    }
}

/// The stand-in reached across a link, each connection answered by the
/// guest's own pairing.
struct Guest {
    link: Link,
    standin: Arc<StandIn>,
    change: Token,
    held: Arc<dyn GuestRecords>,
    dials: AtomicUsize,
    connections: Mutex<Vec<Connection>>,
}

impl Dial for Guest {
    fn dial(&self) -> Dialing<'_> {
        self.dials.fetch_add(1, SeqCst);
        let (station, guest, connection) = self.link.connect();
        self.connections.lock().unwrap().push(connection);
        let (standin, change, held) =
            (self.standin.clone(), self.change.clone(), self.held.clone());
        tokio::spawn(async move {
            let facts = GuestFacts {
                uid: 502,
                boot: "boot-1".to_owned(),
            };
            let _ = guest_pairs(
                guest,
                &standin.keys,
                &standin.desktop,
                &change,
                &facts,
                &held,
            )
            .await;
        });
        Box::pin(async move { Ok(Box::new(station) as Raw) })
    }
}

/// A VM not paired yet whose guest holds `secret` for `change`, and the
/// station's records and enrolments, the enrolment kept.
struct Fixture {
    guest: Guest,
    held: Arc<Held>,
    own: Keypair,
    kept: Arc<Kept>,
    enrolments: Arc<Memory>,
}

fn fixture() -> Fixture {
    let secret = PairingSecret::generate().unwrap();
    let change = Token::new();
    let standin = StandIn::new(
        DesktopId::parse("0123456789abcdef01234567").unwrap(),
        Duration::ZERO,
    );
    let held = Arc::new(Held {
        committed: Mutex::new(None),
        secret: Mutex::new(Some(*secret.as_bytes())),
    });
    let enrolments = Arc::new(Memory::default());
    *enrolments.0.lock().unwrap() = Some((*secret.as_bytes(), change.clone()));
    Fixture {
        guest: Guest {
            link: Link::new(Conditions {
                delay: Duration::from_millis(50),
                ..Conditions::default()
            }),
            standin,
            change,
            held: held.clone(),
            dials: AtomicUsize::new(0),
            connections: Mutex::default(),
        },
        held,
        own: Keypair::generate().unwrap(),
        kept: Arc::new(Kept::default()),
        enrolments,
    }
}

impl Fixture {
    async fn pair(&self) -> Result<Paired> {
        pair(&Pairing {
            vm: &self.guest.standin.desktop,
            name: "work",
            dial: &self.guest,
            own: &self.own,
            enrolments: self.enrolments.clone(),
            records: self.kept.clone(),
        })
        .await
    }

    fn record(&self) -> Option<Enrolment> {
        self.kept.record.lock().unwrap().clone()
    }
}

#[tokio::test(start_paused = true)]
async fn a_vm_pairs_through_the_stations_own_code_and_its_enrolment_is_then_deleted() {
    let fixture = fixture();

    let paired = fixture.pair().await.unwrap();

    assert_eq!(paired.guest, fixture.guest.standin.keys.public());
    assert_eq!(paired.change, fixture.guest.change);
    assert_eq!(fixture.record(), Some(Enrolment::Paired(paired)));
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
    assert_eq!(
        fixture
            .held
            .committed
            .lock()
            .unwrap()
            .as_ref()
            .map(|(key, _)| *key),
        Some(fixture.own.public())
    );
    assert_eq!(fixture.guest.dials.load(SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_pairing_cut_short_after_the_guest_committed_completes_on_a_connection_of_its_own() {
    let fixture = fixture();
    fixture.kept.fails.store(1, SeqCst);

    let paired = fixture.pair().await.unwrap();

    assert_eq!(fixture.guest.dials.load(SeqCst), 2);
    assert_eq!(fixture.record(), Some(Enrolment::Paired(paired)));
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_pairing_cut_short_is_resumed_from_its_pending_record_and_only_then_forgets_the_enrolment()
 {
    let fixture = fixture();
    fixture.kept.fails.store(2, SeqCst);

    assert!(fixture.pair().await.is_err());
    assert!(matches!(fixture.record(), Some(Enrolment::Pending { .. })));
    assert!(
        fixture.enrolments.0.lock().unwrap().is_some(),
        "kept until paired"
    );

    let paired = fixture.pair().await.unwrap();

    assert_eq!(fixture.record(), Some(Enrolment::Paired(paired)));
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_pending_record_whose_guest_never_committed_pairs_again_with_the_secret() {
    let fixture = fixture();
    *fixture.kept.record.lock().unwrap() = Some(Enrolment::Pending {
        change: fixture.guest.change.clone(),
        guest: fixture.guest.standin.keys.public(),
    });

    let paired = fixture.pair().await.unwrap();

    assert_eq!(fixture.record(), Some(Enrolment::Paired(paired)));
    assert!(fixture.held.secret.lock().unwrap().is_none());
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_vm_paired_already_is_not_dialled_and_its_leftover_enrolment_is_deleted() {
    let fixture = fixture();
    let paired = fixture.pair().await.unwrap();
    *fixture.enrolments.0.lock().unwrap() = Some(([1; 32], Token::new()));

    assert_eq!(fixture.pair().await.unwrap(), paired);
    assert_eq!(fixture.guest.dials.load(SeqCst), 1);
    assert!(fixture.enrolments.0.lock().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_vm_with_no_enrolment_kept_or_its_pairing_revoked_is_refused_and_never_dialled() {
    let unenrolled = fixture();
    unenrolled.enrolments.0.lock().unwrap().take();
    assert_eq!(unenrolled.pair().await.unwrap_err().code, Code::Identity);

    let revoked = fixture();
    *revoked.kept.record.lock().unwrap() = Some(Enrolment::Revoked {
        change: Token::new(),
    });
    assert_eq!(revoked.pair().await.unwrap_err().code, Code::Identity);
    assert!(revoked.enrolments.0.lock().unwrap().is_some());

    assert_eq!(unenrolled.guest.dials.load(SeqCst), 0);
    assert_eq!(revoked.guest.dials.load(SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_guest_that_never_answers_ends_the_attempt_within_its_bound() {
    let fixture = fixture();
    fixture.guest.link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });

    let started = tokio::time::Instant::now();
    let refused = fixture.pair().await.unwrap_err();

    assert_eq!(refused.code, Code::Transport);
    assert!(started.elapsed() >= ATTEMPT);
    assert!(fixture.enrolments.0.lock().unwrap().is_some());
}
