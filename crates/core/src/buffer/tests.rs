use super::*;
use std::{io::Write, sync::Arc};

#[test]
fn storage_switches_to_a_private_mapping_above_the_heap_limit() {
    for len in [0, 1, HEAP_LIMIT, HEAP_LIMIT + 1, 1024 * 1024] {
        let mut bytes = Buffer::zeroed(len).unwrap();
        assert_eq!(bytes.len(), len);
        assert_eq!(bytes.capacity(), len);
        assert!(bytes.iter().all(|byte| *byte == 0));
        assert_eq!(
            matches!(bytes.storage, Storage::Mapped(_)),
            len > HEAP_LIMIT
        );
        bytes.fill(37);
        assert!(bytes.iter().all(|byte| *byte == 37));
    }
    assert!(Buffer::with_capacity(usize::MAX).is_err());
}

#[test]
fn appending_never_reallocates_or_partially_writes_an_overflow() {
    for capacity in [8, HEAP_LIMIT + 1] {
        let mut bytes = Buffer::with_capacity(capacity).unwrap();
        let pointer = bytes.as_ptr();
        assert!(bytes.is_empty());
        bytes.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(bytes.as_slice(), &[1, 2, 3]);
        assert!(bytes.write_all(&vec![9; capacity]).is_err());
        assert_eq!(bytes.as_slice(), &[1, 2, 3]);
        bytes.write_all(&vec![7; capacity - 3]).unwrap();
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(bytes.capacity(), capacity);
        assert_eq!(&bytes[..3], &[1, 2, 3]);
        assert!(bytes[3..].iter().all(|byte| *byte == 7));
        assert!(bytes.write_all(&[8]).is_err());
    }
}

#[test]
fn copied_pixels_remain_owned_until_the_last_shared_frame_drops() {
    let pixels = vec![67; HEAP_LIMIT + 1];
    let first = Arc::new(Buffer::copy_from(&pixels).unwrap());
    let weak = Arc::downgrade(&first);
    let second = first.clone();
    assert_eq!(first.as_slice(), pixels);
    assert_ne!(first.as_ptr(), pixels.as_ptr());
    drop(first);
    assert_eq!(second.as_slice(), pixels);
    assert!(weak.upgrade().is_some());
    drop(second);
    assert!(weak.upgrade().is_none());
    assert!(Buffer::default().is_empty());
}
