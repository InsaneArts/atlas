use super::*;

/// A bundle named `name` in `folder`, its program `program`.
fn bundle(folder: &Path, name: &str, program: &str) {
    let inside = folder.join(name).join("Contents/MacOS");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::write(inside.join("atlas"), program).unwrap();
}

fn program(bundle: &Path) -> Option<String> {
    std::fs::read_to_string(bundle.join("Contents/MacOS/atlas")).ok()
}

#[test]
fn a_next_copy_takes_the_place_of_the_own_one_which_goes() {
    let home = tempfile::tempdir().unwrap();
    let from = tempfile::tempdir().unwrap();
    bundle(home.path(), OWN, "old");
    bundle(from.path(), "Next.app", "new");
    let wanted = |bundle: &Path| program(bundle).as_deref() == Some("new");

    take(
        home.path(),
        Some((&File::open(from.path()).unwrap(), &name("Next.app"))),
        &wanted,
    )
    .unwrap();

    assert_eq!(program(&home.path().join(OWN)).as_deref(), Some("new"));
    assert!(!home.path().join(INCOMING).exists());
    assert!(!from.path().join("Next.app").exists());
}

#[test]
fn a_swap_cut_short_before_the_replaced_copy_went_finishes_without_swapping_back() {
    let home = tempfile::tempdir().unwrap();
    bundle(home.path(), OWN, "new");
    bundle(home.path(), INCOMING, "old");
    let wanted = |bundle: &Path| program(bundle).as_deref() == Some("new");

    take(home.path(), None, &wanted).unwrap();

    assert_eq!(program(&home.path().join(OWN)).as_deref(), Some("new"));
    assert!(!home.path().join(INCOMING).exists());
}

#[test]
fn a_move_cut_short_beside_the_own_copy_is_swapped_in_when_asked_again() {
    let home = tempfile::tempdir().unwrap();
    bundle(home.path(), OWN, "old");
    bundle(home.path(), INCOMING, "new");
    let wanted = |bundle: &Path| program(bundle).as_deref() == Some("new");

    take(home.path(), None, &wanted).unwrap();

    assert_eq!(program(&home.path().join(OWN)).as_deref(), Some("new"));
    assert!(!home.path().join(INCOMING).exists());
}

#[test]
fn no_copy_of_the_wanted_code_is_refused_and_the_own_one_is_left() {
    let home = tempfile::tempdir().unwrap();
    bundle(home.path(), OWN, "old");
    let wanted = |bundle: &Path| program(bundle).as_deref() == Some("new");

    assert!(take(home.path(), None, &wanted).is_err());

    assert_eq!(program(&home.path().join(OWN)).as_deref(), Some("old"));
}
