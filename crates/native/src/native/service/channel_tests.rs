use super::*;

fn every() -> Vec<Frame> {
    vec![
        Frame::Open(7, 3),
        Frame::Data(7, b"{\"op\":\"hello\"}\n".to_vec()),
        Frame::Close(7),
        Frame::Renew(3, 9_000_000_000),
        Frame::Revoke(3),
        Frame::Hold { seq: 4, on: true },
        Frame::Hold { seq: 5, on: false },
        Frame::Revoked(3),
        Frame::Settled(3),
        Frame::Held { seq: 4, on: true },
    ]
}

#[tokio::test]
async fn every_frame_reads_back_as_it_was_written() {
    let (mut writing, mut reading) = tokio::io::duplex(1 << 20);
    for frame in every() {
        frame.write(&mut writing).await.unwrap();
    }

    for frame in every() {
        assert_eq!(Frame::read(&mut reading).await, Ok(frame));
    }
}

#[tokio::test]
async fn a_stream_that_ends_part_way_through_a_frame_is_lost() {
    let mut bytes = Frame::Data(1, b"hello".to_vec()).encode();
    bytes.truncate(bytes.len() - 1);

    assert_eq!(Frame::read(&mut bytes.as_slice()).await, Err(Lost));
}

#[tokio::test]
async fn what_is_not_a_frame_is_lost() {
    let mut unknown = Frame::Close(1).encode();
    unknown[0] = 99;
    let mut too_long = Frame::Data(1, b"x".to_vec()).encode();
    too_long[13..17].copy_from_slice(&u32::try_from(PAYLOAD + 1).unwrap().to_be_bytes());
    let mut bytes_where_none_go = Frame::Revoke(1).encode();
    bytes_where_none_go[13..17].copy_from_slice(&1u32.to_be_bytes());
    bytes_where_none_go.push(0);
    let mut a_hold_neither_on_nor_off = Frame::Hold { seq: 1, on: true }.encode();
    *a_hold_neither_on_nor_off.last_mut().unwrap() = 2;
    let mut a_short_deadline = Frame::Renew(1, 5).encode();
    a_short_deadline.truncate(17 + 4);
    a_short_deadline[13..17].copy_from_slice(&4u32.to_be_bytes());
    let empty_data = {
        let mut frame = Frame::Data(1, b"x".to_vec()).encode();
        frame.truncate(17);
        frame[13..17].copy_from_slice(&0u32.to_be_bytes());
        frame
    };

    for frame in [
        unknown,
        too_long,
        bytes_where_none_go,
        a_hold_neither_on_nor_off,
        a_short_deadline,
        empty_data,
    ] {
        assert_eq!(
            Frame::read(&mut frame.as_slice()).await,
            Err(Lost),
            "{frame:?}"
        );
    }
}

#[tokio::test]
async fn a_frame_too_large_or_empty_is_never_written() {
    let mut sink = Vec::new();

    assert_eq!(
        Frame::Data(1, vec![0; PAYLOAD + 1]).write(&mut sink).await,
        Err(Lost)
    );
    assert_eq!(Frame::Data(1, Vec::new()).write(&mut sink).await, Err(Lost));
    assert!(sink.is_empty());
}
