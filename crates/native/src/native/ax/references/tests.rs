use super::super::{ffi, tree::Traced};
use super::*;
use objc2_application_services::AXUIElement;
use objc2_core_foundation::CFRetained;
use std::collections::HashSet;

fn epoch() -> InputEpoch {
    InputEpoch {
        uid: 501,
        security_session_id: 1,
        display_id: 2,
        width: 800.0,
        height: 600.0,
        process_id: 3,
    }
}

/// An object of its own for each number: the element of that process.
fn object(number: i32) -> CFRetained<AXUIElement> {
    ffi::application(number).unwrap()
}

fn button(index: usize, title: &str) -> Element {
    Element {
        index,
        role: Some("AXButton".into()),
        title: Some(title.into()),
        ..Element::default()
    }
}

fn empty() -> Snapshot {
    Snapshot {
        elements: Vec::new(),
        incomplete: false,
        stops: Default::default(),
        errors: Default::default(),
    }
}

/// A look at `instance` of `count` elements, each reached from the one
/// before it and each its own object, numbered from `first`.
fn chained(instance: Instance, first: i32, count: usize) -> Read {
    let seen = (0..count)
        .map(|index| Seen {
            object: Kept(object(first + i32::try_from(index).unwrap())),
            parent: index.checked_sub(1),
            element: button(index, &format!("Button {first}.{index}")),
        })
        .collect();
    Read {
        instance,
        snapshot: empty(),
        seen,
    }
}

/// A look at `instance` of `count` elements that are all one object.
fn sized(instance: Instance, count: usize) -> Read {
    let shared = object(1);
    let seen = (0..count)
        .map(|index| Seen {
            object: Kept(shared.clone()),
            parent: None,
            element: button(index, "Same"),
        })
        .collect();
    Read {
        instance,
        snapshot: empty(),
        seen,
    }
}

fn editor() -> Instance {
    Instance::new(812, [123, 456])
}

fn finder() -> Instance {
    Instance::new(900, [123, 789])
}

fn at(look: Look, index: usize) -> Reference {
    Reference { look, index }
}

fn assert_stale(error: &Error) {
    assert_eq!(error.code, Code::Elements, "{}", error.error);
    assert!(!error.uncertain);
    assert_eq!(error.details["next"], LOOK_AGAIN);
}

#[test]
fn two_callers_looks_each_keep_what_they_saw_and_one_never_retargets_the_other() {
    let looks = Looks::default();
    let now = Instant::now();
    let (first, _) = looks.keep(epoch(), chained(editor(), 100, 3), now).unwrap();
    let (second, _) = looks.keep(epoch(), chained(editor(), 200, 3), now).unwrap();
    assert_ne!(first, second);

    for (look, base) in [(first, 100), (second, 200)] {
        let found = looks.find(at(look, 2), &epoch(), now).unwrap();
        assert_eq!(found.instance, editor());
        assert_eq!(
            found.chain[0].1.title.as_deref(),
            Some(&*format!("Button {base}.2"))
        );
        let objects: Vec<bool> = found
            .chain
            .iter()
            .zip([base + 2, base + 1, base])
            .map(|((kept, _), number)| *kept.0 == *object(number))
            .collect();
        assert_eq!(
            objects,
            [true, true, true],
            "its own objects, nearest first"
        );
    }
}

#[test]
fn a_look_that_gave_way_to_newer_ones_asks_for_a_fresh_look() {
    let looks = Looks::default();
    let now = Instant::now();
    let (oldest, _) = looks.keep(epoch(), chained(editor(), 1, 1), now).unwrap();
    for number in 0..KEPT {
        looks
            .keep(
                epoch(),
                chained(finder(), 10 + i32::try_from(number).unwrap(), 1),
                now,
            )
            .unwrap();
    }
    assert_stale(&looks.find(at(oldest, 0), &epoch(), now).err().unwrap());
}

#[test]
fn looks_past_their_size_or_their_age_are_let_go() {
    let looks = Looks::default();
    let now = Instant::now();
    let (first, _) = looks
        .keep(epoch(), sized(editor(), ELEMENTS / 2), now)
        .unwrap();
    let (second, _) = looks
        .keep(epoch(), sized(editor(), ELEMENTS / 2), now)
        .unwrap();
    assert!(looks.find(at(first, 0), &epoch(), now).is_ok());
    let (third, _) = looks.keep(epoch(), sized(editor(), 1), now).unwrap();
    assert_stale(&looks.find(at(first, 0), &epoch(), now).err().unwrap());
    assert!(looks.find(at(second, 0), &epoch(), now).is_ok());

    let almost = now + LIFETIME - Duration::from_millis(1);
    assert!(looks.find(at(third, 0), &epoch(), almost).is_ok());
    assert_stale(
        &looks
            .find(at(third, 0), &epoch(), now + LIFETIME)
            .err()
            .unwrap(),
    );
}

#[test]
fn a_reference_to_another_epoch_or_a_missing_element_is_refused() {
    let looks = Looks::default();
    let now = Instant::now();
    let (look, _) = looks.keep(epoch(), chained(editor(), 1, 2), now).unwrap();
    let mut moved = epoch();
    moved.width = 1024.0;
    assert_stale(&looks.find(at(look, 0), &moved, now).err().unwrap());
    assert_stale(&looks.find(at(look, 2), &epoch(), now).err().unwrap());
    let unknown = [Look::new(0), Look::new(1)]
        .into_iter()
        .find(|unknown| *unknown != look)
        .unwrap();
    assert_stale(&looks.find(at(unknown, 0), &epoch(), now).err().unwrap());
}

#[test]
fn no_name_comes_back_while_its_look_could_still_be_kept() {
    let looks = Looks::default();
    let now = Instant::now();
    let names: HashSet<Look> = (0..2000)
        .map(|_| looks.keep(epoch(), sized(editor(), 1), now).unwrap().0)
        .collect();
    assert_eq!(names.len(), 2000);
}

#[test]
fn a_replaced_or_unnamed_process_is_refused_before_its_app_is_read_again() {
    let looks = Looks::default();
    let me = i32::try_from(std::process::id()).unwrap();
    let reused = Instance::new(me, [1, 0]);
    let (look, _) = looks
        .keep(epoch(), chained(reused, 1, 1), Instant::now())
        .unwrap();
    let error = resolve(&looks, at(look, 0), &epoch(), None).unwrap_err();
    assert_stale(&error);
    assert_eq!(error.error, "The app it is in quit or was replaced");

    let error = resolve(&looks, at(look, 0), &epoch(), Some(editor())).unwrap_err();
    assert_stale(&error);
    assert_eq!(
        error.error,
        "That look is of another app than the one named"
    );
}

/// A read again: each element by its object's number, the index it was
/// reached from and its title.
fn observed(entries: &[(i32, Option<usize>, &str)], incomplete: bool) -> Observed {
    Observed {
        snapshot: Snapshot {
            elements: entries
                .iter()
                .enumerate()
                .map(|(index, (_, _, title))| button(index, title))
                .collect(),
            incomplete,
            ..empty()
        },
        objects: entries
            .iter()
            .map(|(number, parent, _)| Traced {
                object: object(*number),
                parent: *parent,
            })
            .collect(),
    }
}

/// Button 3, reached from 2, reached from 1 where the walk started.
fn third() -> Vec<(Kept, Element)> {
    [3, 2, 1]
        .map(|number| (Kept(object(number)), button(0, &format!("Button {number}"))))
        .into()
}

#[test]
fn an_element_is_met_again_by_its_object_wherever_its_siblings_moved() {
    let reordered = observed(
        &[
            (1, None, "Button 1"),
            (9, Some(0), "Button 9"),
            (2, Some(0), "Button 2"),
            (3, Some(2), "Button 3"),
        ],
        false,
    );
    assert_eq!(met_again(&third(), &reordered).unwrap(), 3);
}

#[test]
fn an_element_replaced_moved_or_changed_is_never_met_again() {
    for (entries, incomplete, why) in [
        (
            vec![
                (1, None, "Button 1"),
                (2, Some(0), "Button 2"),
                (4, Some(1), "Button 3"),
            ],
            false,
            "The element is no longer there",
        ),
        (
            vec![(1, None, "Button 1"), (2, Some(0), "Button 2")],
            true,
            "The element was not found again in time",
        ),
        (
            vec![
                (1, None, "Button 1"),
                (2, Some(0), "Button 2"),
                (3, Some(0), "Button 3"),
            ],
            false,
            "The element is no longer where that look saw it",
        ),
        (
            vec![
                (10, None, "Window"),
                (1, Some(0), "Button 1"),
                (2, Some(1), "Button 2"),
                (3, Some(2), "Button 3"),
            ],
            false,
            "The element is no longer where that look saw it",
        ),
        (
            vec![
                (1, None, "Button 1"),
                (2, Some(0), "Other row"),
                (3, Some(1), "Button 3"),
            ],
            false,
            "The element changed since that look",
        ),
        (
            vec![
                (1, None, "Button 1"),
                (2, Some(0), "Button 2"),
                (3, Some(1), "Renamed"),
            ],
            false,
            "The element changed since that look",
        ),
    ] {
        let error = met_again(&third(), &observed(&entries, incomplete)).unwrap_err();
        assert_stale(&error);
        assert_eq!(error.error, why, "{entries:?}");
    }
}

#[test]
fn an_element_is_the_same_only_while_what_identifies_it_holds() {
    let then = button(4, "Save");
    let mut typed = then.clone();
    typed.value = Some("draft".into());
    typed.frame = Some(Bounds::display(10.0, 10.0));
    typed.truncated = vec!["AXValue".into()];
    typed.unreadable = vec!["AXValue".into()];
    assert!(same(&then, &typed), "its value and frame may change");

    let mut renamed = then.clone();
    renamed.title = Some("Save As…".into());
    let mut unread = then.clone();
    unread.unreadable = vec!["AXTitle".into()];
    let mut cut = then.clone();
    cut.truncated = vec!["AXTitle".into()];
    let mut menu = then.clone();
    menu.menu = true;
    let mut other = then.clone();
    other.identifier = Some("other".into());
    for changed in [renamed, unread, cut, menu, other] {
        assert!(!same(&then, &changed), "{changed:?}");
    }

    // A static text says what it is by its value.
    let mut label = then.clone();
    label.role = Some("AXStaticText".into());
    label.value = Some("Invoice 12".into());
    let mut relabelled = label.clone();
    relabelled.value = Some("Invoice 13".into());
    let mut cut = label.clone();
    cut.truncated = vec!["AXValue".into()];
    assert!(same(&label, &label));
    for changed in [relabelled, cut] {
        assert!(!same(&label, &changed), "{changed:?}");
    }
}
