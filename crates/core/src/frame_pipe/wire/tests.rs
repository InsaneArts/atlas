use super::*;

fn packet(kind: Kind, request: u64, metadata: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = encode(kind, request, metadata, payload.len() as u64).unwrap();
    bytes.extend_from_slice(payload);
    bytes
}

/// Every item `bytes` decodes to, fed `step` bytes at a time.
fn decoded(lane: Lane, bytes: &[u8], step: usize) -> Result<Vec<Item>, Refusal> {
    let mut decoder = Decoder::new(lane);
    let mut items = Vec::new();
    for piece in bytes.chunks(step.max(1)) {
        let mut input = piece;
        while let Some(item) = decoder.next(&mut input)? {
            items.push(item);
        }
        assert!(input.is_empty(), "a returned none consumed the whole piece");
    }
    decoder.end()?;
    Ok(items)
}

/// Payload pieces joined, so splits that cut them differently compare equal.
fn joined(items: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for item in items {
        match (out.last_mut(), item) {
            (Some(Item::Payload(last)), Item::Payload(next)) => last.extend(next),
            (_, item) => out.push(item),
        }
    }
    out
}

#[test]
fn the_prefix_is_exactly_its_documented_bytes() {
    let bytes = encode(Kind::Credit, 0x0102, b"{}", 0).unwrap();
    assert_eq!(&bytes[..4], b"ATFP");
    assert_eq!(&bytes[4..6], &[0, 1]);
    assert_eq!(&bytes[6..8], &[0, 9]);
    assert_eq!(&bytes[8..12], &[0; 4]);
    assert_eq!(&bytes[12..16], &[0, 0, 0, 2]);
    assert_eq!(&bytes[16..24], &[0; 8]);
    assert_eq!(&bytes[24..32], &[0, 0, 0, 0, 0, 0, 1, 2]);
    assert_eq!(&bytes[32..], b"{}");
}

#[test]
fn any_split_or_join_of_the_stream_decodes_the_same_packets() {
    let mut stream = packet(Kind::Attached, 7, br#"{"a":1}"#, &[]);
    let pixels: Vec<u8> = (0..CHUNK + 300).map(|n| n as u8).collect();
    stream.extend(packet(Kind::FullFrame, 8, br#"{"w":2}"#, &pixels));
    stream.extend(packet(Kind::Error, 0, br#"{"e":0}"#, &[]));
    let whole = joined(decoded(Lane::Frame, &stream, stream.len()).unwrap());
    assert_eq!(whole.len(), 4);
    assert_eq!(whole[2], Item::Payload(pixels));
    for step in (1..=PREFIX + 9).chain([1000, CHUNK - 1, CHUNK, CHUNK + 1]) {
        assert_eq!(
            joined(decoded(Lane::Frame, &stream, step).unwrap()),
            whole,
            "{step}"
        );
    }
}

#[test]
fn payload_pieces_never_pass_the_chunk_size() {
    let pixels = vec![9; CHUNK * 2 + 1];
    let stream = packet(Kind::FullFrame, 1, b"{}", &pixels);
    let items = decoded(Lane::Frame, &stream, stream.len()).unwrap();
    let sizes: Vec<usize> = items
        .iter()
        .filter_map(|item| match item {
            Item::Payload(piece) => Some(piece.len()),
            Item::Packet(..) => None,
        })
        .collect();
    assert_eq!(sizes, [CHUNK, CHUNK, 1]);
}

#[test]
fn a_lane_that_ends_inside_a_packet_is_truncated() {
    let stream = packet(Kind::FullFrame, 1, b"{}", &[1, 2, 3]);
    for cut in 1..stream.len() {
        assert_eq!(
            decoded(Lane::Frame, &stream[..cut], cut).unwrap_err(),
            Refusal::Truncated,
            "{cut}"
        );
    }
    assert!(decoded(Lane::Frame, &[], 1).unwrap().is_empty());
}

#[test]
fn a_foreign_or_malformed_prefix_is_refused_before_metadata_is_reserved() {
    let good = encode(Kind::Hello, 1, b"{}", 0).unwrap();
    let altered = |at: usize, value: u8| {
        let mut bytes = good.clone();
        bytes[at] = value;
        bytes
    };
    let mut huge = good.clone();
    huge[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
    let mut payload_on_control = good.clone();
    payload_on_control[23] = 1;
    let mut zero_request = good.clone();
    zero_request[24..32].copy_from_slice(&[0; 8]);
    for (bytes, refusal) in [
        (altered(0, b'X'), Refusal::Invalid),
        (altered(5, 2), Refusal::Version),
        (altered(7, 99), Refusal::Invalid),
        (altered(11, 1), Refusal::Invalid),
        (huge, Refusal::Oversized),
        (payload_on_control, Refusal::Invalid),
        (zero_request, Refusal::Invalid),
    ] {
        let mut decoder = Decoder::new(Lane::Control);
        let mut input = &bytes[..PREFIX];
        assert_eq!(decoder.next(&mut input).unwrap_err(), refusal);
    }
}

#[test]
fn each_kind_travels_only_on_its_lanes() {
    let hello = encode(Kind::Hello, 1, b"{}", 0).unwrap();
    assert_eq!(
        decoded(Lane::Frame, &hello, hello.len()).unwrap_err(),
        Refusal::Invalid
    );
    let frame = packet(Kind::FullFrame, 1, b"{}", &[0; 3]);
    assert_eq!(
        decoded(Lane::Control, &frame, frame.len()).unwrap_err(),
        Refusal::Invalid
    );
    for code in 1..=50 {
        let kind = Kind::of(code).unwrap();
        assert_eq!(kind.code(), code);
        assert!(
            [Lane::Control, Lane::Frame, Lane::Management]
                .iter()
                .any(|lane| kind.travels(*lane))
        );
    }
    assert!(Kind::of(0).is_none() && Kind::of(51).is_none());
}

#[test]
fn the_encoder_refuses_what_the_decoder_would() {
    assert_eq!(encode(Kind::Credit, 1, b"{}", 3), Err(Refusal::Invalid));
    assert_eq!(encode(Kind::FullFrame, 1, b"{}", 0), Err(Refusal::Invalid));
    assert_eq!(
        encode(Kind::FullFrame, 1, b"{}", PAYLOAD_MAX + 1),
        Err(Refusal::Oversized)
    );
    assert_eq!(
        encode(Kind::Credit, 1, &vec![b' '; METADATA_MAX + 1], 0),
        Err(Refusal::Oversized)
    );
    assert_eq!(encode(Kind::Credit, 1, b"", 0), Err(Refusal::Invalid));
    assert_eq!(encode(Kind::Credit, 0, b"{}", 0), Err(Refusal::Invalid));
    assert!(
        encode(Kind::State, 0, b"{}", 0).is_ok(),
        "an unsolicited state has no request"
    );
    assert!(encode(Kind::FullFrame, 1, b"{}", PAYLOAD_MAX).is_ok());
    assert_eq!(Refusal::Truncated.reason(), Reason::Truncated);
}

#[test]
fn every_kind_has_its_planned_code_and_lanes() {
    use Lane::{Control as C, Frame as F, Management as M};
    let planned: [(Kind, u16, &[Lane]); 50] = [
        (Kind::Hello, 1, &[C]),
        (Kind::Welcome, 2, &[C]),
        (Kind::Attach, 3, &[F]),
        (Kind::Attached, 4, &[F]),
        (Kind::PairConfirm, 5, &[C]),
        (Kind::PairReady, 6, &[C]),
        (Kind::Subscribe, 7, &[C]),
        (Kind::Subscribed, 8, &[C]),
        (Kind::Credit, 9, &[C]),
        (Kind::CreditAccepted, 10, &[C]),
        (Kind::FullFrame, 11, &[F]),
        (Kind::DeltaFrame, 12, &[F]),
        (Kind::UnchangedFrame, 13, &[F]),
        (Kind::Decoded, 14, &[C]),
        (Kind::DecodedAck, 15, &[C]),
        (Kind::Commit, 16, &[C]),
        (Kind::CommitAck, 17, &[C]),
        (Kind::Drop, 18, &[C]),
        (Kind::DropAck, 19, &[C]),
        (Kind::Resync, 20, &[C]),
        (Kind::ResyncAck, 21, &[C]),
        (Kind::Acquire, 22, &[C]),
        (Kind::Acquired, 23, &[C]),
        (Kind::Renew, 24, &[C]),
        (Kind::Renewed, 25, &[C]),
        (Kind::Input, 26, &[C]),
        (Kind::InputAck, 27, &[C]),
        (Kind::Release, 28, &[C]),
        (Kind::Released, 29, &[C]),
        (Kind::Stop, 30, &[C]),
        (Kind::StopResult, 31, &[C]),
        (Kind::Suspend, 32, &[C]),
        (Kind::Suspended, 33, &[C]),
        (Kind::Close, 34, &[C]),
        (Kind::Closed, 35, &[C]),
        (Kind::CreditExpired, 36, &[C]),
        (Kind::State, 37, &[C]),
        (Kind::Error, 38, &[C, F, M]),
        (Kind::RequestDiagnostics, 39, &[C]),
        (Kind::DiagnosticsReply, 40, &[C]),
        (Kind::RecoveryHello, 41, &[M]),
        (Kind::RecoveryReady, 42, &[M]),
        (Kind::Cleanup, 43, &[M]),
        (Kind::CleanupResult, 44, &[M]),
        (Kind::RegisterAppHost, 45, &[M]),
        (Kind::AppHostReady, 46, &[M]),
        (Kind::AppCommand, 47, &[M]),
        (Kind::AppCommandAck, 48, &[M]),
        (Kind::AppHostClosing, 49, &[M]),
        (Kind::AppHostClosed, 50, &[M]),
    ];
    for (kind, code, lanes) in planned {
        assert_eq!(kind.code(), code, "{kind:?}");
        assert_eq!(Kind::of(code), Some(kind), "{code}");
        for lane in [C, F, M] {
            assert_eq!(
                kind.travels(lane),
                lanes.contains(&lane),
                "{kind:?} on {lane:?}"
            );
        }
    }
    assert_eq!(Kind::of(0), None);
    assert_eq!(Kind::of(51), None);
    let state = encode(Kind::State, 1, b"{}", 0);
    assert_eq!(
        state,
        Err(Refusal::Invalid),
        "a state change answers no request"
    );
}
