use super::*;
use std::os::unix::net::UnixListener;

#[test]
fn a_reply_line_is_read_whole_and_a_peer_that_ends_without_one_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let socket = folder.path().join("control");
    let listener = UnixListener::bind(&socket).unwrap();
    let serving = std::thread::spawn(move || {
        for reply in [&b"{\"ok\":true}\n"[..], &b"{\"ok\""[..]] {
            let (mut peer, _) = listener.accept().unwrap();
            let mut asked = [0_u8; 64];
            let _ = peer.read(&mut asked).unwrap();
            peer.write_all(reply).unwrap();
        }
    });

    let answered = ask(&socket, &serde_json::json!({"op": "hello"})).unwrap();
    let cut = ask(&socket, &serde_json::json!({"op": "hello"}));

    serving.join().unwrap();
    assert_eq!(answered["ok"], true);
    assert!(cut.is_err());
}

#[test]
fn no_time_left_is_refused_before_anything_waits() {
    assert!(left(Instant::now()).is_err());
}
