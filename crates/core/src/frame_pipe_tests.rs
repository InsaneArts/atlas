//! The shared golden vectors, read from the Swift package's fixtures so both
//! sides decode the same bytes.
use super::{
    model::{Attached, CreditOnly, FrameMetadata, Rectangle, decode, rectangles},
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

/// One decoded packet: its kind, request ID, metadata and whole payload.
struct Packet {
    kind: Kind,
    request: u64,
    metadata: Vec<u8>,
    payload: Vec<u8>,
}

/// Every packet in `bytes`, fed `step` bytes at a time.
fn packets(lane: Lane, bytes: &[u8], step: usize) -> Result<Vec<Packet>, Refusal> {
    let mut decoder = Decoder::new(lane);
    let mut found: Vec<Packet> = Vec::new();
    for piece in bytes.chunks(step) {
        let mut input = piece;
        while let Some(item) = decoder.next(&mut input)? {
            match item {
                Item::Packet(head, metadata) => found.push(Packet {
                    kind: head.kind,
                    request: head.request,
                    metadata,
                    payload: Vec::new(),
                }),
                Item::Payload(piece) => found.last_mut().unwrap().payload.extend(piece),
            }
        }
    }
    decoder.end()?;
    Ok(found)
}

/// Checks that `bytes` read in two pieces, cut anywhere, decode to `whole`.
fn every_cut(lane: Lane, bytes: &[u8], whole: &[Packet], name: &str) {
    for cut in 1..bytes.len() {
        let mut decoder = Decoder::new(lane);
        let mut found: Vec<Packet> = Vec::new();
        for piece in [&bytes[..cut], &bytes[cut..]] {
            let mut input = piece;
            while let Some(item) = decoder.next(&mut input).unwrap() {
                match item {
                    Item::Packet(head, metadata) => found.push(Packet {
                        kind: head.kind,
                        request: head.request,
                        metadata,
                        payload: Vec::new(),
                    }),
                    Item::Payload(piece) => found.last_mut().unwrap().payload.extend(piece),
                }
            }
        }
        decoder.end().unwrap();
        let shape = |packets: &[Packet]| -> Vec<(Kind, u64, Vec<u8>, Vec<u8>)> {
            packets
                .iter()
                .map(|p| (p.kind, p.request, p.metadata.clone(), p.payload.clone()))
                .collect()
        };
        assert_eq!(shape(&found), shape(whole), "{name} cut at {cut}");
    }
}

/// The packet's metadata decoded as its kind's type, back as JSON.
fn typed(packet: &Packet) -> Value {
    let value = match packet.kind {
        Kind::FullFrame | Kind::DeltaFrame | Kind::UnchangedFrame => {
            let frame: FrameMetadata = decode(&packet.metadata).unwrap();
            frame
                .check(packet.kind, packet.payload.len() as u64)
                .unwrap();
            serde_json::to_value(frame)
        }
        Kind::Attached => serde_json::to_value(decode::<Attached>(&packet.metadata).unwrap()),
        Kind::CreditExpired => {
            serde_json::to_value(decode::<CreditOnly>(&packet.metadata).unwrap())
        }
        other => panic!("no vector of {other:?}"),
    };
    value.unwrap()
}

/// `base` with each rectangle's pixels from `pixels` written in, row by row.
fn apply(base: &[u8], rectangles: &[Rectangle], pixels: &[u8], width: u32) -> Vec<u8> {
    let mut picture = base.to_vec();
    let mut at = 0;
    for rectangle in rectangles {
        let row = rectangle.width as usize * 3;
        for line in 0..rectangle.height {
            let start = ((rectangle.y + line) * width + rectangle.x) as usize * 3;
            picture[start..start + row].copy_from_slice(&pixels[at..at + row]);
            at += row;
        }
    }
    picture
}

#[test]
fn every_golden_vector_decodes_as_its_manifest_says_at_every_split() {
    let manifest: Vec<Value> =
        serde_json::from_slice(&std::fs::read(fixtures().join("manifest.json")).unwrap()).unwrap();
    let full_vector = manifest
        .iter()
        .find(|vector| vector["file"] == "full-4x4.bin")
        .unwrap();
    assert_eq!(full_vector["packets"][0]["kind"], Kind::FullFrame.code());
    let full = unhex(full_vector["packets"][0]["payload"].as_str().unwrap());
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
                "truncated" => assert_eq!(decoded.err(), Some(Refusal::Truncated), "{name}"),
                "oversized" => assert_eq!(decoded.err(), Some(Refusal::Oversized), "{name}"),
                "invalid-metadata" => {
                    let packet = decoded.unwrap().remove(0);
                    assert_eq!(packet.kind, Kind::FullFrame);
                    assert_eq!(
                        decode::<FrameMetadata>(&packet.metadata),
                        Err(Refusal::Invalid),
                        "{name}"
                    );
                }
                _ => {
                    let decoded = decoded.unwrap();
                    if step == 1 {
                        every_cut(lane, &bytes, &decoded, name);
                    }
                    let expected = vector["packets"].as_array().unwrap();
                    assert_eq!(decoded.len(), expected.len(), "{name}");
                    for (packet, expected) in decoded.iter().zip(expected) {
                        assert_eq!(
                            u64::from(packet.kind.code()),
                            expected["kind"].as_u64().unwrap()
                        );
                        assert_eq!(packet.request, expected["request"].as_u64().unwrap());
                        assert_eq!(
                            packet.payload,
                            unhex(expected["payload"].as_str().unwrap()),
                            "{name}"
                        );
                        assert_eq!(typed(packet), expected["metadata"], "{name}");
                        if packet.kind == Kind::DeltaFrame {
                            let frame: FrameMetadata = decode(&packet.metadata).unwrap();
                            let records = frame.rectangle_count as usize * 16;
                            let found = rectangles(
                                &packet.payload[..records],
                                frame.width.get(),
                                frame.height.get(),
                                packet.payload.len() as u64,
                            )
                            .unwrap();
                            let corners: Vec<Value> = found
                                .iter()
                                .map(|r| serde_json::json!([r.x, r.y, r.width, r.height]))
                                .collect();
                            assert_eq!(&Value::from(corners), &expected["rectangles"], "{name}");
                            if let Some(applied) = expected["applied"].as_str() {
                                assert_eq!(
                                    apply(
                                        &full,
                                        &found,
                                        &packet.payload[records..],
                                        frame.width.get()
                                    ),
                                    unhex(applied),
                                    "{name}"
                                );
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
    let bytes = std::fs::read(fixtures().join("full-4x4.bin")).unwrap();
    let packet = packets(Lane::Frame, &bytes, bytes.len()).unwrap().remove(0);
    let frame: FrameMetadata = decode(&packet.metadata).unwrap();
    assert!(frame.check(Kind::FullFrame, 48).is_ok());
    let delta = FrameMetadata {
        rectangle_count: 1,
        base_image_sequence: 1,
        ..frame.clone()
    };
    assert!(delta.check(Kind::DeltaFrame, 17).is_ok());
    assert!(delta.check(Kind::DeltaFrame, 47).is_ok());
    for marked in [
        FrameMetadata {
            base_image_sequence: 1,
            ..frame.clone()
        },
        FrameMetadata {
            rectangle_count: 1,
            ..frame.clone()
        },
    ] {
        assert_eq!(
            marked.check(Kind::FullFrame, 48),
            Err(Refusal::Invalid),
            "a full picture names no base or rectangles"
        );
    }
    for (kind, payload) in [
        (Kind::FullFrame, 47),
        (Kind::FullFrame, 49),
        (Kind::UnchangedFrame, 0),
        (Kind::DeltaFrame, 48),
        (Kind::Credit, 48),
    ] {
        assert_eq!(
            frame.check(kind, payload),
            Err(Refusal::Invalid),
            "{kind:?} {payload}"
        );
    }
}
