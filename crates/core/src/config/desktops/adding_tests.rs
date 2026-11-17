use super::*;

#[test]
fn an_admission_is_shared_by_every_copy_and_looked_at_again_only_once_its_file_changed() {
    let adding = Adding::default();
    let copy = adding.clone();
    let mut looks = 0;

    adding.renew(Some((1, 10, 5, 0)), || {
        looks += 1;
        Admission::Adding
    });
    adding.renew(Some((1, 10, 5, 0)), || {
        looks += 1;
        Admission::Open
    });
    let unchanged = copy.get();
    copy.renew(Some((1, 12, 6, 0)), || {
        looks += 1;
        Admission::Open
    });

    assert_eq!(unchanged, Admission::Adding);
    assert_eq!(looks, 2);
    assert_eq!(adding.get(), Admission::Open);
}

#[test]
fn a_selection_never_looked_at_is_open_and_a_missing_file_is_looked_at_once() {
    let adding = Adding::default();
    assert_eq!(adding.get(), Admission::Open);
    let mut looks = 0;

    for _ in 0..2 {
        adding.renew(None, || {
            looks += 1;
            Admission::Adding
        });
    }

    assert_eq!(looks, 1);
    assert_eq!(adding.get(), Admission::Adding);
}

#[test]
fn a_look_that_could_not_tell_is_never_kept_and_the_next_reads_again() {
    let adding = Adding::default();
    let mark = Some((1, 10, 5, 0));
    adding.renew(mark, || Admission::Open);

    adding.doubt();
    let doubted = adding.get();
    adding.renew(mark, || Admission::Uncertain);
    let failed = adding.get();
    adding.renew(mark, || Admission::Open);

    assert_eq!(
        (doubted, failed),
        (Admission::Uncertain, Admission::Uncertain)
    );
    assert_eq!(
        adding.get(),
        Admission::Open,
        "read again with its file unchanged"
    );
}
