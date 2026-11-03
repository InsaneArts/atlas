//! The shared golden vectors, read from the Swift package's fixtures so both
//! sides decode the same bytes.
use super::{
    model::{FrameMetadata, decode, rectangles},
    wire::{Decoder, Item, Kind, Lane, Refusal},
};
use serde_json::Value;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/Packages/AtlasService/Tests/AtlasServiceTests/Fixtures/FramePipe/v1")
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// Every packet in `bytes`, with its whole payload, fed `step` bytes at a time.
fn packets(
    lane: Lane,
    bytes: &[u8],
    step: usize,
) -> Result<Vec<(Kind, u64, Vec<u8>, Vec<u8>)>, Refusal> {
    let mut decoder = Decoder::new(lane);
    let mut found: Vec<(Kind, u64, Vec<u8>, Vec<u8>)> = Vec::new();
    for piece in bytes.chunks(step) {
        let mut input = piece;
        while let Some(item) = decoder.next(&mut input)? {
            match item {
                Item::Packet(head, metadata) => {
                    found.push((head.kind, head.request, metadata, Vec::new()));
                }
                Item::Payload(piece) => found.last_mut().unwrap().3.extend(piece),
            }
        }
    }
    decoder.end()?;
    Ok(found)
}

#[test]
fn every_golden_vector_decodes_as_its_manifest_says_at_every_split() {
    let manifest: Vec<Value> =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.len(), 10);
    for vector in manifest {
        let name = vector["file"].as_str().unwrap();
        let bytes = std::fs::read(fixtures().join(name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            vector["bytes"].as_u64().unwrap(),
            "{name}"
        );
        let lane = if vector["lane"] == "control" {
            Lane::Control
        } else {
            Lane::Frame
        };
        for step in [1, 7, 32, bytes.len().max(1)] {
            let decoded = packets(lane, &bytes, step);
            match vector["result"].as_str().unwrap() {
                "truncated" => assert_eq!(decoded.unwrap_err(), Refusal::Truncated, "{name}"),
                "oversized" => assert_eq!(decoded.unwrap_err(), Refusal::Oversized, "{name}"),
                "invalid-metadata" => {
                    let (kind, _, metadata, _) = decoded.unwrap().remove(0);
                    assert_eq!(kind, Kind::FullFrame);
                    assert_eq!(
                        decode::<FrameMetadata>(&metadata),
                        Err(Refusal::Invalid),
                        "{name}"
                    );
                }
                _ => {
                    let decoded = decoded.unwrap();
                    let expected = vector["packets"].as_array().unwrap();
                    assert_eq!(decoded.len(), expected.len(), "{name}");
                    for ((kind, request, metadata, payload), expected) in
                        decoded.iter().zip(expected)
                    {
                        assert_eq!(u64::from(kind.code()), expected["kind"].as_u64().unwrap());
                        assert_eq!(*request, expected["request"].as_u64().unwrap());
                        assert_eq!(
                            *payload,
                            unhex(expected["payload"].as_str().unwrap()),
                            "{name}"
                        );
                        if matches!(
                            kind,
                            Kind::FullFrame | Kind::DeltaFrame | Kind::UnchangedFrame
                        ) {
                            let frame: FrameMetadata = decode(metadata).unwrap();
                            frame.check(*kind, payload.len() as u64).unwrap();
                            if *kind == Kind::DeltaFrame {
                                let count = frame.rectangle_count as usize * 16;
                                let found = rectangles(
                                    &payload[..count],
                                    frame.width.get(),
                                    frame.height.get(),
                                    payload.len() as u64,
                                )
                                .unwrap();
                                if let Some(expected) = expected["rectangles"].as_array() {
                                    let corners: Vec<Value> = found
                                        .iter()
                                        .map(|r| serde_json::json!([r.x, r.y, r.width, r.height]))
                                        .collect();
                                    assert_eq!(&corners, expected);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_frame_and_its_length_must_agree_before_any_payload_is_reserved() {
    let bytes = std::fs::read(fixtures().join("full-2x2.bin")).unwrap();
    let (_, _, metadata, _) = packets(Lane::Frame, &bytes, bytes.len()).unwrap().remove(0);
    let frame: FrameMetadata = decode(&metadata).unwrap();
    assert!(frame.check(Kind::FullFrame, 12).is_ok());
    for (kind, payload) in [
        (Kind::FullFrame, 11),
        (Kind::FullFrame, 13),
        (Kind::UnchangedFrame, 0),
        (Kind::DeltaFrame, 12),
        (Kind::Credit, 12),
    ] {
        assert_eq!(
            frame.check(kind, payload),
            Err(Refusal::Invalid),
            "{kind:?} {payload}"
        );
    }
}

#[test]
fn delta_rectangles_must_be_inside_ordered_apart_and_exactly_paid_for() {
    let record = |x: u32, y: u32, w: u32, h: u32| {
        [x, y, w, h]
            .iter()
            .flat_map(|n| n.to_be_bytes())
            .collect::<Vec<u8>>()
    };
    let two = [record(0, 0, 1, 1), record(1, 0, 1, 1)].concat();
    assert_eq!(rectangles(&two, 2, 2, 32 + 6).unwrap().len(), 2);
    for (records, payload) in [
        (two.clone(), 32 + 5),
        ([record(1, 0, 1, 1), record(0, 0, 1, 1)].concat(), 38),
        ([record(0, 0, 2, 1), record(1, 0, 1, 1)].concat(), 32 + 9),
        (record(1, 1, 2, 1), 16 + 6),
        (record(0, 0, 0, 1), 16),
        (record(u32::MAX, 0, 2, 1), 16 + 6),
        (Vec::new(), 0),
        (two[..17].to_vec(), 17 + 6),
    ] {
        assert_eq!(
            rectangles(&records, 2, 2, payload),
            Err(Refusal::Invalid),
            "{payload}"
        );
    }
}
