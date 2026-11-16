use super::*;
use crate::{
    compatibility::{ACKNOWLEDGED, PROTOCOL},
    secure::{Secure, connect_as_guest},
    standin::{
        Live, StandIn,
        link::{Conditions, Connection, Link},
    },
};
use atlas_core::{desktop::Desktop, user::User};
use serde_json::{Value, json};
use std::sync::atomic::Ordering::SeqCst;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

fn desktop() -> DesktopId {
    Desktop::from(User { uid: 502 }).id()
}

/// The stand-in live over `link`, and the station's own end of it.
async fn opened(
    link: &Link,
    standin: &Arc<StandIn>,
    station: &Keypair,
) -> (Live, Control, Events, Connection) {
    let (station_end, desktop_end, connection) = link.connect();
    let (live, control) = tokio::join!(
        standin.control(desktop_end, station.public()),
        open(
            station_end,
            station,
            standin.keys.public(),
            &standin.desktop,
            |_| true
        ),
    );
    let (control, events) = control.unwrap();
    (live.unwrap(), control, events, connection)
}

/// The next event, within `within`.
async fn next(events: &mut Events, within: Duration) -> Option<Event> {
    tokio::time::timeout(within, events.recv())
        .await
        .ok()
        .flatten()
}

/// Open the station's end on `station_end` against a guest that says
/// `hello` once authenticated, claiming nothing: the station's result and
/// the guest's stream.
async fn meeting(
    hello: Value,
    claimed: impl Fn(&Token) -> bool,
) -> (Result<(Control, Events)>, BufReader<Secure<DuplexStream>>) {
    let (station, guest) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let (station_end, guest_end) = tokio::io::duplex(1 << 16);
    let guest_side = async {
        let mut secure = connect_as_guest(
            guest_end,
            &guest,
            station.public(),
            &desktop(),
            Purpose::Control,
        )
        .await
        .unwrap();
        let mut line = serde_json::to_vec(&hello).unwrap();
        line.push(b'\n');
        secure.write_all(&line).await.unwrap();
        secure.flush().await.unwrap();
        BufReader::new(secure)
    };
    let named = desktop();
    let station_side = open(station_end, &station, guest.public(), &named, claimed);
    let (guest, opened) = tokio::join!(guest_side, station_side);
    (opened, guest)
}

/// The next message the station says that is not a heartbeat.
async fn said(guest: &mut BufReader<Secure<DuplexStream>>) -> Value {
    loop {
        let mut line = String::new();
        let read = tokio::time::timeout(Duration::from_secs(5), guest.read_line(&mut line))
            .await
            .expect("the station said something")
            .unwrap();
        assert!(read > 0, "the station closed");
        let said: Value = serde_json::from_str(&line).unwrap();
        if said != json!("beat") {
            return said;
        }
    }
}

fn hello(generation: u64, unsettled: &[Token], capabilities: &[&str]) -> Value {
    json!({"hello": {
        "generation": generation,
        "unsettled": unsettled,
        "protocol": PROTOCOL,
        "capabilities": capabilities,
        "version": "0.1.3",
    }})
}

#[tokio::test(start_paused = true)]
async fn the_station_opens_on_the_guests_hello_and_takes_its_generation_and_capabilities() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );

    let (live, control, _events, _connection) = opened(&link, &standin, &station).await;

    assert_eq!(control.generation(), live.authority.generation());
    assert!(control.declared().acknowledges());
    assert!(control.unsettled().is_empty());
}

#[tokio::test(start_paused = true)]
async fn heartbeats_across_a_slow_link_keep_the_station_from_falling_silent() {
    let link = Link::new(Conditions {
        delay: Duration::from_millis(200),
        bytes_per_second: Some(64 * 1024),
        partitioned: false,
    });
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (live, _control, mut events, _connection) = opened(&link, &standin, &station).await;

    assert!(next(&mut events, Duration::from_secs(30)).await.is_none());
    assert!(live.authority.check().is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_silence_and_the_guest_heard_again_are_told_to_the_owner() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_live, _control, mut events, _connection) = opened(&link, &standin, &station).await;

    link.set(Conditions {
        partitioned: true,
        ..Conditions::default()
    });
    let silent = next(&mut events, Duration::from_secs(10)).await;
    assert!(matches!(silent, Some(Event::Silent)), "{silent:?}");
    link.set(Conditions::default());
    let again = next(&mut events, Duration::from_secs(10)).await;
    assert!(matches!(again, Some(Event::HeardAgain)), "{again:?}");
}

#[tokio::test(start_paused = true)]
async fn a_lost_connection_is_told_to_the_owner_and_no_stop_is_said_on_it_after() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_live, control, mut events, connection) = opened(&link, &standin, &station).await;

    connection.cut();

    let lost = next(&mut events, Duration::from_secs(10)).await;
    assert!(matches!(lost, Some(Event::Lost(_))), "{lost:?}");
    assert!(next(&mut events, Duration::from_secs(1)).await.is_none());
    assert!(control.stop().is_err());
    assert!(control.resume().is_err());
}

#[tokio::test(start_paused = true)]
async fn numbered_stops_and_resumes_are_each_answered_once_the_guest_carried_them_out() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_live, control, mut events, _connection) = opened(&link, &standin, &station).await;

    let stopped = control.stop().unwrap();
    let resumed = control.resume().unwrap();
    let again = control.stop().unwrap();

    assert_eq!(
        (stopped.seq(), resumed.seq(), again.seq()),
        (Some(1), Some(2), Some(3))
    );
    assert_eq!(stopped.answer().await, Answer::Applied(Outcome::Stopped));
    assert_eq!(resumed.answer().await, Answer::Applied(Outcome::Resumed));
    assert_eq!(again.answer().await, Answer::Applied(Outcome::Stopped));
    let mut told = Vec::new();
    while told.len() < 3 {
        if let Some(Event::Applied { seq, outcome }) =
            next(&mut events, Duration::from_secs(5)).await
        {
            told.push((seq, outcome));
        }
    }
    assert_eq!(
        told,
        [
            (1, Outcome::Stopped),
            (2, Outcome::Resumed),
            (3, Outcome::Stopped)
        ]
    );
    assert_eq!(standin.stops.load(SeqCst), 2);
    assert_eq!(standin.resumes.load(SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_new_generation_numbers_its_stops_from_the_start() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_first, first, _first_events, _first_connection) = opened(&link, &standin, &station).await;
    assert_eq!(first.stop().unwrap().seq(), Some(1));

    let (_second, second, _second_events, _second_connection) =
        opened(&link, &standin, &station).await;

    let asked = second.stop().unwrap();
    assert_eq!(asked.seq(), Some(1));
    assert_eq!(asked.answer().await, Answer::Applied(Outcome::Stopped));
}

#[tokio::test(start_paused = true)]
async fn a_guest_that_does_not_acknowledge_is_told_the_unit_stop_and_resume_unconfirmed() {
    let (opened, mut guest) = meeting(hello(4, &[], &[]), |_| true).await;
    let (control, _events) = opened.unwrap();
    assert!(!control.declared().acknowledges());

    let stopped = control.stop().unwrap();
    assert_eq!(stopped.seq(), None);
    assert_eq!(stopped.answer().await, Answer::Unconfirmed);
    assert_eq!(said(&mut guest).await, json!("stop"));
    let resumed = control.resume().unwrap();
    assert_eq!(resumed.answer().await, Answer::Unconfirmed);
    assert_eq!(said(&mut guest).await, json!("resume"));
}

#[tokio::test(start_paused = true)]
async fn a_stop_whose_generation_is_lost_before_its_answer_is_unconfirmed() {
    let (opened, mut guest) = meeting(hello(4, &[], &[ACKNOWLEDGED]), |_| true).await;
    let (control, _events) = opened.unwrap();

    let asked = control.stop().unwrap();
    assert_eq!(said(&mut guest).await, json!({"halt": {"seq": 1}}));
    drop(guest);

    let answer = tokio::time::timeout(Duration::from_secs(10), asked.answer()).await;
    assert_eq!(answer.unwrap(), Answer::Unconfirmed);
}

#[tokio::test(start_paused = true)]
async fn the_changes_a_hello_names_that_the_station_holds_no_claim_on_are_settled_at_once() {
    let (kept, done) = (Token::new(), Token::new());
    let claim = kept.clone();
    let (opened, mut guest) = meeting(
        hello(4, &[kept.clone(), done.clone()], &[ACKNOWLEDGED]),
        move |change| *change == claim,
    )
    .await;
    let (control, _events) = opened.unwrap();

    assert_eq!(control.unsettled(), [kept, done.clone()]);
    assert_eq!(
        said(&mut guest).await,
        json!({"settled": {"changes": [done]}})
    );
}

#[tokio::test(start_paused = true)]
async fn a_hello_that_names_nothing_unsettled_settles_nothing() {
    let (opened, mut guest) = meeting(hello(4, &[], &[ACKNOWLEDGED]), |_| false).await;
    let (control, _events) = opened.unwrap();

    control.stop().unwrap();
    assert_eq!(said(&mut guest).await, json!({"halt": {"seq": 1}}));
}

#[tokio::test(start_paused = true)]
async fn a_guest_that_never_says_its_hello_is_given_up_within_the_opening_bound() {
    let (station, guest) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let (station_end, guest_end) = tokio::io::duplex(1 << 16);
    let guest_side = async {
        let secure = connect_as_guest(
            guest_end,
            &guest,
            station.public(),
            &desktop(),
            Purpose::Control,
        )
        .await
        .unwrap();
        tokio::time::sleep(OPENING * 3).await;
        drop(secure);
    };
    let started = tokio::time::Instant::now();
    let named = desktop();
    let station_side = open(station_end, &station, guest.public(), &named, |_| true);

    let (_, opened) = tokio::join!(guest_side, async {
        let opened = station_side.await;
        (opened, started.elapsed())
    });

    let (opened, took) = opened;
    assert_eq!(opened.unwrap_err().code, Code::Transport);
    assert!(took >= OPENING && took < OPENING * 2, "{took:?}");
}

#[tokio::test(start_paused = true)]
async fn a_desktop_that_never_finishes_its_handshake_is_given_up_within_the_opening_bound() {
    let (station, guest) = (Keypair::generate().unwrap(), Keypair::generate().unwrap());
    let (station_end, _silent) = tokio::io::duplex(1 << 16);
    let started = tokio::time::Instant::now();

    let opened = open(station_end, &station, guest.public(), &desktop(), |_| true).await;

    assert_eq!(opened.unwrap_err().code, Code::Transport);
    assert!(started.elapsed() >= OPENING);
}

#[tokio::test(start_paused = true)]
async fn an_owner_that_falls_behind_its_events_loses_the_generation_never_an_answer() {
    let link = Link::new(Conditions::default());
    let (standin, station) = (
        StandIn::new(desktop(), Duration::ZERO),
        Keypair::generate().unwrap(),
    );
    let (_live, control, mut events, _connection) = opened(&link, &standin, &station).await;

    let asked: Vec<_> = (0..EVENTS + 40).map(|_| control.stop().unwrap()).collect();
    let mut answers = Vec::new();
    for asked in asked {
        answers.push(asked.answer().await);
    }

    let applied = answers
        .iter()
        .take_while(|answer| **answer == Answer::Applied(Outcome::Stopped))
        .count();
    assert!(applied > EVENTS, "{applied}");
    assert!(
        answers[applied..]
            .iter()
            .all(|answer| *answer == Answer::Unconfirmed)
    );
    let mut told = 0;
    loop {
        match next(&mut events, Duration::from_secs(5)).await {
            Some(Event::Applied { .. }) => told += 1,
            Some(Event::Lost(_)) => break,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(told, EVENTS);
    assert!(control.stop().is_err());
}
