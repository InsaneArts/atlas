use super::*;
use std::io::Read;

#[test]
fn the_terminal_probe_checks_compressed_rgb_without_creating_a_placement() {
    let mut wire = Vec::new();
    query(&mut wire).unwrap();
    let wire = String::from_utf8(wire).unwrap();
    let (header, payload) = wire.split_once(';').unwrap();
    assert_eq!(header, "\x1b_Ga=q,f=24,s=1,v=1,o=z,t=d,i=47472");
    assert!(!wire.contains("a=p"));
    let compressed = STANDARD
        .decode(payload.strip_suffix("\x1b\\").unwrap())
        .unwrap();
    let mut pixels = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut pixels)
        .unwrap();
    assert_eq!(pixels, [0, 0, 0]);
}

#[test]
fn root_edit_support_requires_the_specific_probe_acknowledgment() {
    let mut wire = Vec::new();
    patch_query(&mut wire).unwrap();
    patch_query_clear(&mut wire).unwrap();
    let wire = String::from_utf8(wire).unwrap();
    assert!(wire.contains("a=t,"));
    assert!(wire.contains("a=f,i=47474,r=1,X=1,x=0,y=0,f=24,s=1,v=1,o=z,t=d,q=0,"));
    assert!(wire.contains("a=d,d=I,i=47474,"));
    assert!(!wire.contains("a=p,"));
    assert!(patch_supported(b"\x1b_Gi=47474;OK\x1b\\"));
    assert!(patch_supported(b"\x1b_Gi=47474,r=1;OK\x1b\\"));
    for reply in [
        &b"\x1b_Gi=47472;OK\x1b\\"[..],
        &b"\x1b_Gi=47474;ENOTSUP: frame editing unsupported\x1b\\"[..],
        &b"\x1b_Gi=47474;OK"[..],
        &b"OK"[..],
    ] {
        assert!(!patch_supported(reply));
    }
}
