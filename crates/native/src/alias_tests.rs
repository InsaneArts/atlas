use super::*;

fn folder() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(directory.path()).unwrap();
    (directory, real)
}

#[test]
fn a_finder_alias_leads_to_what_it_was_made_for() {
    let (_directory, folder) = folder();
    let target = folder.join("secret");
    std::fs::write(&target, b"secret").unwrap();
    let alias = folder.join("notes alias");
    make(&target, &alias);

    assert_eq!(leads_to(&alias), [target]);
}

#[test]
fn an_alias_of_an_alias_leads_through_both() {
    let (_directory, folder) = folder();
    let target = folder.join("Keys");
    std::fs::create_dir(&target).unwrap();
    let first = folder.join("first");
    make(&target, &first);
    let second = folder.join("second");
    make(&first, &second);

    assert_eq!(leads_to(&second).last(), Some(&target));
}

#[test]
fn a_plain_file_or_a_missing_one_leads_nowhere() {
    let (_directory, folder) = folder();
    let plain = folder.join("plain.txt");
    std::fs::write(&plain, b"text").unwrap();

    assert!(leads_to(&plain).is_empty());
    assert!(leads_to(&folder.join("missing")).is_empty());
}
