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
