use super::*;
use std::os::{fd::AsFd, unix::net::UnixStream};

#[test]
fn a_peer_is_the_program_it_connected_as_and_nothing_else() {
    let (here, there) = UnixStream::pair().unwrap();
    let me = std::env::current_exe().unwrap();
    // This test's own process is the peer: its file, but not Atlas's
    // signature, nor another program's file.
    assert!(verified(here.as_fd(), &me, "anchor apple").is_none());
    assert!(verified(here.as_fd(), Path::new("/usr/bin/true"), "always").is_none());
    assert!(peer_token(there.as_fd()).is_some());
    // Under a requirement any valid code meets, it is the program it runs.
    assert_eq!(
        verified(here.as_fd(), &me, "always"),
        Some(std::process::id())
    );
}

#[test]
fn an_apps_signature_names_the_program_inside_its_bundle() {
    let bundle = CFURL::from_file_path("/System/Applications/Calculator.app").unwrap();
    let mut on_disk: *const SecStaticCode = std::ptr::null();
    // SAFETY: `on_disk` is a valid place for the call's result.
    let status = unsafe {
        SecStaticCode::create_with_path(&bundle, SecCSFlags::empty(), NonNull::from(&mut on_disk))
    };
    assert_eq!(status, 0);
    // SAFETY: on success the call returned a retained static code object.
    let on_disk = unsafe { CFRetained::from_raw(NonNull::new(on_disk.cast_mut()).unwrap()) };
    assert_eq!(
        main_executable(&on_disk),
        Some("/System/Applications/Calculator.app/Contents/MacOS/Calculator".into())
    );
}

#[test]
fn a_running_peer_is_named_by_its_token_with_the_build_it_runs() {
    let (here, _there) = UnixStream::pair().unwrap();

    assert!(running(here.as_fd(), "anchor apple").is_none());
    let peer = running(here.as_fd(), "always").unwrap();

    assert_eq!(peer.pid, std::process::id());
    assert_eq!(peer.uid, nix::unistd::geteuid().as_raw());
    assert!(peer.pid_version > 0);
    // A code directory hash is twenty bytes.
    assert_eq!(peer.cdhash.len(), 20);
}

#[test]
fn a_signed_programs_cdhash_is_the_one_it_runs_with() {
    let (here, _there) = UnixStream::pair().unwrap();
    let me = std::env::current_exe().unwrap();

    let on_disk = file_cdhash(&me).unwrap();

    assert_eq!(
        Some(on_disk),
        running(here.as_fd(), "always").map(|peer| peer.cdhash)
    );
    assert!(file_cdhash(Path::new("/nonexistent")).is_none());
}

#[test]
fn this_programs_own_code_is_the_build_its_file_is_signed_as() {
    let me = std::env::current_exe().unwrap();

    assert_eq!(own_code(), file_cdhash(&me));
    assert_eq!(own_code().map(|code| code.len()), Some(20));
}
