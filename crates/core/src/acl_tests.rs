use super::*;
use std::process::Command;

#[test]
fn entries_text_and_clear_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, b"x").unwrap();
    assert!(!present(&path).unwrap());
    let user = std::env::var("USER").unwrap();

    assert!(
        crate::spawn::status(
            Command::new("/bin/chmod")
                .args(["+a", &format!("user:{user} allow write")])
                .arg(&path)
        )
        .unwrap()
        .success()
    );

    let acl = Acl::of_path(&path).unwrap().unwrap();
    let entries = acl.entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].allow && entries[0].mask & WRITE_RIGHTS != 0);
    assert!(acl.text().unwrap().contains(&format!(":{user}:")));

    let file = File::open(&path).unwrap();
    assert!(present_on(&file).unwrap());
    clear(&file).unwrap();
    assert!(!present(&path).unwrap());
    assert!(!present_on(&file).unwrap());
    assert!(
        Acl::of_file(&file)
            .unwrap()
            .is_none_or(|acl| acl.entries().unwrap().is_empty())
    );
}
