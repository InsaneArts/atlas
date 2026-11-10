use super::*;
use crate::host::Key;

#[test]
fn cached_records_follow_the_observed_directory_revision_and_path() {
    static RECORDS: Records = Records {
        folder: "records",
        noun: "test",
        lock_wait: Duration::ZERO,
        durability: Durability::Kernel,
        bytes: 1024,
    };
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let folder = RecordFolder::<Key>::at(first.path(), &RECORDS);
    let other = RecordFolder::<Key>::at(second.path(), &RECORDS);
    folder.make().unwrap();
    other.make().unwrap();
    let revision = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    let set_modified = |folder: &RecordFolder<Key>, modified| {
        File::open(folder.directory())
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
    };
    set_modified(&folder, revision);
    let kept = Mutex::new(None);
    let read = folder.cached(&kept, Duration::MAX, || vec![1]);
    let reused = folder.cached(&kept, Duration::MAX, || panic!("unchanged folder"));
    assert!(Arc::ptr_eq(&read, &reused));
    let revision = revision + Duration::from_secs(1);
    set_modified(&folder, revision);
    assert_eq!(&*folder.cached(&kept, Duration::MAX, || vec![2]), &[2]);
    set_modified(&other, revision);
    assert_eq!(&*other.cached(&kept, Duration::MAX, || vec![3]), &[3]);
}

#[test]
fn record_listing_bounds_unrelated_files_and_unreadable_entries() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("unrelated"), "").unwrap();
    for unreadable in [false, true] {
        let entries = std::iter::repeat_with(|| {
            if unreadable {
                Err(std::io::Error::other("unreadable entry"))
            } else {
                fs::read_dir(directory.path()).unwrap().next().unwrap()
            }
        })
        .take(LISTED)
        .chain(std::iter::once_with(|| {
            panic!("read beyond the entry limit")
        }));
        let listing = RecordFolder::<Key>::listed_from(entries, 64);
        assert!(listing.keys.is_empty());
        assert!(!listing.complete);
    }
}

#[test]
fn a_running_job_beyond_the_scan_budget_leaves_the_listing_incomplete() {
    use crate::protocol::JobId;

    let directory = tempfile::tempdir().unwrap();
    let unrelated = directory.path().join("unrelated");
    fs::write(&unrelated, "").unwrap();
    let id = JobId::parse("7c2e91a04b3f").unwrap();
    let job = directory.path().join(format!("{id}.json"));
    fs::write(&job, r#"{"state":"running"}"#).unwrap();
    let entry = |path: &Path| {
        fs::read_dir(directory.path())
            .unwrap()
            .find(|entry| entry.as_ref().unwrap().path() == path)
            .unwrap()
    };
    let entries = std::iter::repeat_with(|| entry(&unrelated))
        .take(LISTED)
        .chain(std::iter::once_with(|| entry(&job)));
    let listing = RecordFolder::<JobId>::listed_from(entries, LISTED);
    assert!(listing.keys.is_empty());
    assert!(!listing.complete, "an omitted running job cannot mean idle");
}

#[test]
fn a_failed_entry_or_metadata_read_leaves_the_listing_incomplete() {
    let failed = std::iter::once(Err(std::io::Error::other("scan failed")));
    assert!(!RecordFolder::<Key>::listed_from(failed, LISTED).complete);

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("7c2e91a04b3f.json");
    fs::write(&path, "{}").unwrap();
    let entry = fs::read_dir(directory.path()).unwrap().next().unwrap();
    fs::remove_file(path).unwrap();
    let listing =
        RecordFolder::<crate::protocol::JobId>::listed_from(std::iter::once(entry), LISTED);
    assert!(!listing.complete);
    assert_eq!(listing.keys.len(), 1, "the unreadable record stays known");
}
