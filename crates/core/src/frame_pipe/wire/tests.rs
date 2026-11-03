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
    let planned: [(u16, &[Lane]); 50] = [
        (1, &[C]),
        (2, &[C]),
        (3, &[F]),
        (4, &[F]),
        (5, &[C]),
        (6, &[C]),
        (7, &[C]),
        (8, &[C]),
        (9, &[C]),
        (10, &[C]),
        (11, &[F]),
        (12, &[F]),
        (13, &[F]),
        (14, &[C]),
        (15, &[C]),
        (16, &[C]),
        (17, &[C]),
        (18, &[C]),
        (19, &[C]),
        (20, &[C]),
        (21, &[C]),
        (22, &[C]),
        (23, &[C]),
        (24, &[C]),
        (25, &[C]),
        (26, &[C]),
        (27, &[C]),
        (28, &[C]),
        (29, &[C]),
        (30, &[C]),
        (31, &[C]),
        (32, &[C]),
        (33, &[C]),
        (34, &[C]),
        (35, &[C]),
        (36, &[C]),
        (37, &[C]),
        (38, &[C, F, M]),
        (39, &[C]),
        (40, &[C]),
        (41, &[M]),
        (42, &[M]),
        (43, &[M]),
        (44, &[M]),
        (45, &[M]),
        (46, &[M]),
        (47, &[M]),
        (48, &[M]),
        (49, &[M]),
        (50, &[M]),
    ];
    for (code, lanes) in planned {
        let kind = Kind::of(code).unwrap();
        for lane in [C, F, M] {
            assert_eq!(
                kind.travels(lane),
                lanes.contains(&lane),
                "{kind:?} on {lane:?}"
            );
        }
    }
    let named = [
        (Kind::Hello, 1),
        (Kind::Attach, 3),
        (Kind::FullFrame, 11),
        (Kind::DeltaFrame, 12),
        (Kind::UnchangedFrame, 13),
        (Kind::Commit, 16),
        (Kind::Input, 26),
        (Kind::Stop, 30),
        (Kind::CreditExpired, 36),
        (Kind::State, 37),
        (Kind::Error, 38),
        (Kind::RecoveryHello, 41),
        (Kind::AppHostClosed, 50),
    ];
    for (kind, code) in named {
        assert_eq!(kind.code(), code, "{kind:?}");
    }
    let state = encode(Kind::State, 1, b"{}", 0);
    assert_eq!(
        state,
        Err(Refusal::Invalid),
        "a state change answers no request"
    );
}
