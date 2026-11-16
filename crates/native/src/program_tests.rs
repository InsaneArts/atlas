use super::*;
use std::os::unix::fs::MetadataExt;

#[test]
fn this_programs_image_is_mapped_from_the_file_it_was_started_from() {
    let started = std::fs::metadata(std::env::current_exe().unwrap()).unwrap();

    let mapped = mapped().expect("the kernel names the file");

    assert_eq!(mapped.inode, started.ino());
    assert_eq!(mapped.device, started.dev());
}
