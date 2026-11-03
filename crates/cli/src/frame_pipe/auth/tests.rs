use super::*;
use atlas_core::frame_pipe::model::{ClientKind, Role};

fn me() -> Peer {
    Peer {
        uid: nix::unistd::getuid().as_raw(),
        instance: Instance::current().unwrap(),
    }
}

fn other() -> Peer {
    Peer {
        instance: Instance::new(1, [0, 0]),
        ..me()
    }
}

fn gate() -> Gate {
    Gate::new(
        Hex32::random(),
        Hex64::random(),
        vec![Right::Observe, Right::Control],
        vec![UpdateKind::Full],
    )
}

fn hello(gate: &Gate, rights: Vec<Right>) -> Hello {
    Hello {
        protocol_major: 1,
        service_instance: gate.service.clone(),
        bootstrap_capability: gate.bootstrap.clone(),
        client_challenge: Hex32::random(),
        role: Role::Observer,
        client_kind: ClientKind::Terminal,
        requested_rights: rights,
        offered_updates: vec![UpdateKind::Full, UpdateKind::Delta],
    }
}

fn attach(welcome: &Welcome) -> Attach {
    Attach {
        service_instance: welcome.service_instance.clone(),
        observer_id: welcome.observer_id.clone(),
        observer_generation: welcome.observer_generation,
        control_connection_id: welcome.control_connection_id.clone(),
        frame_challenge: Hex32::random(),
        pair_token: welcome.pair_token.clone(),
    }
}

#[tokio::test]
async fn the_kernel_names_the_peer_and_only_the_controller_is_admitted() {
    let (left, _right) = UnixStream::pair().unwrap();
    assert_eq!(peer(&left, me().uid).unwrap(), me());
    assert_eq!(peer(&left, me().uid + 1), Err(Reason::WrongPeer));
}

#[test]
fn one_process_pairs_both_lanes_with_rights_its_capability_allows() {
    let mut gate = gate();
    let now = Instant::now();
    let hello = hello(
        &gate,
        vec![
            Right::Observe,
            Right::Revoke,
            Right::Control,
            Right::Observe,
        ],
    );
    let welcome = gate.hello(me(), &hello, now).unwrap();
    assert_eq!(welcome.client_challenge, hello.client_challenge);
    assert_eq!(welcome.granted_rights, [Right::Observe, Right::Control]);
    assert_eq!(welcome.accepted_updates, [UpdateKind::Full]);
    assert_eq!(welcome.pair_expires_in_ms, PAIR_EXPIRES_MS);
    assert_ne!(welcome.pair_token, welcome.cleanup_token);

    let attach = attach(&welcome);
    let (attached, half) = gate.attach(me(), &attach, now).unwrap();
    assert_eq!(attached.frame_challenge, attach.frame_challenge);
    let confirm = Pair {
        service_instance: welcome.service_instance.clone(),
        observer_id: welcome.observer_id.clone(),
        observer_generation: welcome.observer_generation,
        control_connection_id: welcome.control_connection_id.clone(),
        frame_connection_id: attached.frame_connection_id.clone(),
    };
    let paired = half.confirm(&confirm, now).unwrap();
    assert_eq!(paired.pair, confirm);
    assert_eq!(paired.peer, me());
    assert_eq!(paired.cleanup_token, welcome.cleanup_token);
    assert_eq!(
        gate.attach(me(), &attach, now).err(),
        Some(Reason::ExpiredCapability),
        "a token is spent once"
    );
}

#[test]
fn a_wrong_capability_service_or_right_earns_no_token() {
    let mut gate = gate();
    let now = Instant::now();
    let mut stale = hello(&gate, vec![Right::Observe]);
    stale.bootstrap_capability = Hex64::random();
    assert_eq!(
        gate.hello(me(), &stale, now).unwrap_err(),
        Reason::ExpiredCapability
    );
    let mut elsewhere = hello(&gate, vec![Right::Observe]);
    elsewhere.service_instance = Hex32::random();
    assert_eq!(
        gate.hello(me(), &elsewhere, now).unwrap_err(),
        Reason::ExpiredCapability
    );
    assert_eq!(
        gate.hello(me(), &hello(&gate, vec![Right::Control]), now)
            .unwrap_err(),
        Reason::RightsDenied,
        "control without observing is nothing"
    );
    assert!(gate.pending.is_empty());
}

#[test]
fn a_token_is_spent_by_any_attempt_and_only_by_its_own_process_in_time() {
    let mut gate = gate();
    let now = Instant::now();
    let welcome = gate
        .hello(me(), &hello(&gate, vec![Right::Observe]), now)
        .unwrap();
    assert_eq!(
        gate.attach(other(), &attach(&welcome), now).err(),
        Some(Reason::WrongPeer)
    );
    assert_eq!(
        gate.attach(me(), &attach(&welcome), now).err(),
        Some(Reason::ExpiredCapability),
        "a failed attempt spends it too"
    );

    let welcome = gate
        .hello(me(), &hello(&gate, vec![Right::Observe]), now)
        .unwrap();
    assert_eq!(
        gate.attach(me(), &attach(&welcome), now + PAIRING).err(),
        Some(Reason::ExpiredCapability)
    );

    let welcome = gate
        .hello(me(), &hello(&gate, vec![Right::Observe]), now)
        .unwrap();
    let mut crossed = attach(&welcome);
    crossed.control_connection_id = Hex32::random();
    assert_eq!(
        gate.attach(me(), &crossed, now).err(),
        Some(Reason::WrongChallenge)
    );
}

#[test]
fn a_confirmation_must_name_both_lanes_before_the_deadline() {
    let mut gate = gate();
    let now = Instant::now();
    let pair = |gate: &mut Gate| {
        let welcome = gate
            .hello(me(), &hello(gate, vec![Right::Observe]), now)
            .unwrap();
        let (attached, half) = gate.attach(me(), &attach(&welcome), now).unwrap();
        let confirm = Pair {
            service_instance: welcome.service_instance,
            observer_id: welcome.observer_id,
            observer_generation: welcome.observer_generation,
            control_connection_id: welcome.control_connection_id,
            frame_connection_id: attached.frame_connection_id,
        };
        (half, confirm)
    };
    let (half, mut confirm) = pair(&mut gate);
    confirm.frame_connection_id = Hex32::random();
    assert_eq!(
        half.confirm(&confirm, now).unwrap_err(),
        Reason::WrongChallenge
    );
    let (half, confirm) = pair(&mut gate);
    assert_eq!(
        half.confirm(&confirm, now + PAIRING).unwrap_err(),
        Reason::ExpiredCapability
    );
}

#[test]
fn pairs_under_way_are_bounded_until_they_expire() {
    let mut gate = gate();
    let now = Instant::now();
    let mut generations = Vec::new();
    for _ in 0..OBSERVERS_MAX {
        let welcome = gate
            .hello(me(), &hello(&gate, vec![Right::Observe]), now)
            .unwrap();
        generations.push(welcome.observer_generation.get());
    }
    assert_eq!(generations, [1, 2, 3, 4]);
    assert_eq!(
        gate.hello(me(), &hello(&gate, vec![Right::Observe]), now)
            .unwrap_err(),
        Reason::BudgetExhausted
    );
    assert!(
        gate.hello(me(), &hello(&gate, vec![Right::Observe]), now + PAIRING)
            .is_ok(),
        "expired pairs make room"
    );
}

#[test]
fn secrets_compare_whole() {
    assert!(same("abcd", "abcd"));
    assert!(!same("abcd", "abce"));
    assert!(!same("abcd", "abc"));
}
