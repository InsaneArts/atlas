use super::file_sha256;

#[test]
fn hashes_regular_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"abc").unwrap();

    assert_eq!(
        file_sha256(&file).as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert_eq!(file_sha256(&link), None);
    assert_eq!(file_sha256(dir.path()), None);

    let fifo = dir.path().join("fifo");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
    assert_eq!(file_sha256(&fifo), None, "never waits for a writer");
}

#[test]
fn a_probe_follows_only_its_own_link_into_an_app() {
    use super::probe_executable;

    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let app = root.join("Atlas.app/Contents/MacOS/atlas");
    std::fs::create_dir_all(app.parent().unwrap()).unwrap();
    std::fs::write(&app, b"x").unwrap();

    let link = root.join("probe");
    std::os::unix::fs::symlink(&app, &link).unwrap();
    assert_eq!(probe_executable(&link), Some(app.clone()));
    assert_eq!(probe_executable(&app), Some(app.clone()), "a plain file");

    let elsewhere = root.join("other");
    std::fs::write(&elsewhere, b"y").unwrap();
    let stray = root.join("stray");
    std::os::unix::fs::symlink(&elsewhere, &stray).unwrap();
    assert_eq!(probe_executable(&stray), None, "links only into an app");
}
