use super::super::ffi;
use super::*;
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

fn seen(number: i32, parent: Option<usize>, element: Element) -> Seen {
    Seen {
        object: Kept(object(number)),
        parent,
        element,
        unproven: Vec::new(),
        refused: Vec::new(),
    }
}

/// A look at `instance` of `count` elements, each reached from the one
/// before it and each its own object, numbered from `first`.
fn chained(instance: Instance, first: i32, count: usize) -> Read {
    let seen = (0..count)
        .map(|index| {
            let element = button(index, &format!("Button {first}.{index}"));
            seen(
                first + i32::try_from(index).unwrap(),
                index.checked_sub(1),
                element,
            )
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
    let shared = Kept(object(1));
    let seen = (0..count)
        .map(|index| Seen {
            object: shared.clone(),
            parent: None,
            element: button(index, "Same"),
            unproven: Vec::new(),
            refused: Vec::new(),
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
            found.chain[0].element.title.as_deref(),
            Some(&*format!("Button {base}.2"))
        );
        let objects: Vec<bool> = found
            .chain
            .iter()
            .zip([base + 2, base + 1, base])
            .map(|(seen, number)| *seen.object.0 == *object(number))
            .collect();
        assert_eq!(
            objects,
            [true, true, true],
            "its own objects, nearest first"
        );
    }
}

#[test]
fn every_look_is_named_by_this_service_and_a_count_it_never_gives_twice() {
    let looks = Looks::default();
    let now = Instant::now();
    let names: Vec<Look> = (0..2000)
        .map(|_| looks.keep(epoch(), sized(editor(), 1), now).unwrap().0)
        .collect();
    assert_eq!(names.iter().collect::<HashSet<_>>().len(), 2000);
    assert!(names.iter().all(|name| name.service == std::process::id()));
    assert_eq!(names[0].sequence, 0);
    assert_eq!(names[1999].sequence, 1999);

    let first = names[0];
    looks.shelf.lock().unwrap().next = Some(Look {
        sequence: u64::MAX,
        ..first
    });
    looks.keep(epoch(), sized(editor(), 1), now).unwrap();
    let error = looks.keep(epoch(), sized(editor(), 1), now).err().unwrap();
    assert_eq!(error.code, Code::Elements);
    assert!(error.error.contains("atlas start-input"), "{}", error.error);
}

/// A name from another service process, or from an earlier one given the
/// same process ID, names nothing here, though this one holds that count.
#[test]
fn a_look_from_another_service_is_refused_even_where_this_one_has_its_count() {
    let looks = Looks::default();
    let now = Instant::now();
    let (look, _) = looks.keep(epoch(), chained(editor(), 1, 1), now).unwrap();
    for earlier in [
        Look {
            service: look.service.wrapping_add(1).max(1),
            ..look
        },
        Look {
            incarnation: (look.incarnation + 1) % INCARNATIONS,
            ..look
        },
    ] {
        let error = looks.find(at(earlier, 0), &epoch(), now).err().unwrap();
        assert_stale(&error);
        assert_eq!(
            error.error,
            "That look was taken by an earlier input service"
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
    let unknown = look.after().unwrap();
    assert_stale(&looks.find(at(unknown, 0), &epoch(), now).err().unwrap());
}

#[test]
fn a_replaced_or_unnamed_process_is_refused_before_its_app_is_read_again() {
    let looks = Looks::default();
    let me = i32::try_from(std::process::id()).unwrap();
    let reused = Instance::new(me, [1, 0]);
    let (look, _) = looks
        .keep(epoch(), chained(reused, 1, 1), Instant::now())
        .unwrap();
    let error = resolve(&looks, at(look, 0), &epoch(), None).err().unwrap();
    assert_stale(&error);
    assert_eq!(error.error, "The app it is in quit or was replaced");

    let error = resolve(&looks, at(look, 0), &epoch(), Some(editor()))
        .err()
        .unwrap();
    assert_stale(&error);
    assert_eq!(
        error.error,
        "That look is of another app than the one named"
    );
}

/// An element an action names is kept only for the app a batch names, and
/// a replaced process is refused before anything of it is read again.
#[test]
fn an_element_to_act_on_is_kept_for_its_own_app_and_met_only_while_it_runs() {
    let looks = Looks::default();
    let me = i32::try_from(std::process::id()).unwrap();
    let reused = Instance::new(me, [1, 0]);
    let (look, _) = looks
        .keep(epoch(), chained(reused, 1, 1), Instant::now())
        .unwrap();
    assert_stale(
        &kept(&looks, at(look, 0), &epoch(), Some(editor()))
            .err()
            .unwrap(),
    );
    let found = kept(&looks, at(look, 0), &epoch(), Some(reused)).unwrap();
    assert_eq!(found.instance(), reused);
    let error = meet(&found, RESOLVE).err().unwrap();
    assert_stale(&error);
    assert_eq!(error.error, "The app it is in quit or was replaced");
}

/// An element is in a web page when it, or any element its look reached
/// it from, is the page's web area; nothing else is.
#[test]
fn an_element_is_in_a_web_page_when_its_look_reached_it_through_one() {
    let looks = Looks::default();
    let now = Instant::now();
    let mut page = chained(editor(), 1, 4);
    page.seen[1].element.role = Some("AXWebArea".into());
    let (look, _) = looks.keep(epoch(), page, now).unwrap();
    let found = |index| looks.find(at(look, index), &epoch(), now).unwrap();

    assert!(!found(0).in_web_page(), "the window above the page");
    assert!(found(1).in_web_page(), "the page itself");
    assert!(found(3).in_web_page(), "a field deep in the page");

    let (app, _) = looks.keep(epoch(), chained(editor(), 10, 3), now).unwrap();
    assert!(!looks.find(at(app, 2), &epoch(), now).unwrap().in_web_page());
}

/// An option the look saw reached from its control lies that far below it,
/// and nothing else lies below anything.
#[test]
fn an_element_lies_above_only_what_its_look_saw_reached_from_it() {
    let looks = Looks::default();
    let now = Instant::now();
    let (look, _) = looks.keep(epoch(), chained(editor(), 1, 3), now).unwrap();
    let (other, _) = looks.keep(epoch(), chained(editor(), 10, 3), now).unwrap();
    let found = |look, index| looks.find(at(look, index), &epoch(), now).unwrap();

    assert_eq!(found(look, 0).above(&found(look, 2)), Some(2));
    assert_eq!(found(look, 1).above(&found(look, 2)), Some(1));
    assert_eq!(found(look, 2).above(&found(look, 0)), None);
    assert_eq!(found(look, 2).above(&found(look, 2)), None);
    assert_eq!(found(other, 0).above(&found(look, 2)), None);
}

/// An app that shows no readable window proves no path into it, so no
/// element of it is found again.
#[test]
fn a_path_that_cannot_be_read_again_is_never_proven() {
    let me = i32::try_from(std::process::id()).unwrap();
    let elsewhere = object(1);
    let path: [&AXUIElement; 1] = [&elsewhere];
    assert!(!matches!(
        retrace(me, &path, Duration::from_millis(100)),
        Ok(Some(_))
    ));
}

fn again(element: Element, unproven: &[&'static str]) -> Retraced {
    Retraced {
        element,
        unproven: unproven.to_vec(),
        refused: Vec::new(),
        object: object(1),
    }
}

#[test]
fn an_element_is_the_same_only_while_what_identifies_it_holds() {
    let then = seen(1, None, button(4, "Save"));
    let mut typed = then.element.clone();
    typed.value = Some("draft".into());
    typed.frame = Some(Bounds::display(10.0, 10.0));
    assert!(
        same(&then, &again(typed, &["AXValue"])),
        "its value and frame may change"
    );

    let mut renamed = then.element.clone();
    renamed.title = Some("Save As…".into());
    let mut menu = then.element.clone();
    menu.menu = true;
    let mut other = then.element.clone();
    other.identifier = Some("other".into());
    let mut kind = then.element.clone();
    kind.subrole = Some("AXCloseButton".into());
    for changed in [renamed, menu, other, kind] {
        assert!(!same(&then, &again(changed.clone(), &[])), "{changed:?}");
    }
}

/// A failed, malformed or cut read is no proof, however alike the two
/// reads look, whichever of them it was.
#[test]
fn an_identifying_attribute_either_read_left_unproven_is_no_proof() {
    let element = button(4, "Save");
    for attribute in [
        "AXRole",
        "AXSubrole",
        "AXTitle",
        "AXDescription",
        "AXIdentifier",
    ] {
        let mut doubted = seen(1, None, element.clone());
        assert!(
            !same(&doubted, &again(element.clone(), &[attribute])),
            "{attribute} unproven now"
        );
        doubted.unproven = vec![attribute];
        assert!(
            !same(&doubted, &again(element.clone(), &[])),
            "{attribute} unproven then"
        );
    }
}

/// A text area whose description getter fails as AppKit's does, in the
/// look and again now, is the same text area; one whose getter answered
/// otherwise in either read is not.
#[test]
fn a_getter_that_fails_alike_in_both_reads_still_proves_the_element() {
    let mut area = button(2, "");
    area.role = Some("AXTextArea".into());
    area.title = None;
    area.identifier = Some("First Text View".into());
    let mut then = seen(1, None, area.clone());
    then.refused = vec!["AXDescription"];
    let now = |refused: &[&'static str]| Retraced {
        refused: refused.to_vec(),
        ..again(area.clone(), &["AXValue"])
    };

    assert!(same(&then, &now(&["AXDescription"])));
    assert!(
        same(&then, &now(&["AXDescription", "AXValue"])),
        "a text area's value is no identity"
    );
    assert!(!same(&then, &now(&[])), "described now");
    assert!(!same(&then, &now(&["AXDescription", "AXTitle"])));

    let mut label = button(4, "");
    label.role = Some("AXStaticText".into());
    label.title = None;
    let mut then = seen(1, None, label.clone());
    then.refused = vec!["AXValue"];
    let refused_now = |refused: &[&'static str]| Retraced {
        refused: refused.to_vec(),
        ..again(label.clone(), &[])
    };
    assert!(same(&then, &refused_now(&["AXValue"])));
    assert!(
        !same(&then, &refused_now(&[])),
        "a static text's value is its identity"
    );
}

#[test]
fn a_static_text_is_the_same_only_while_its_value_is_and_is_proven() {
    let mut label = button(4, "");
    label.role = Some("AXStaticText".into());
    label.title = None;
    label.value = Some("Invoice 12".into());
    let then = seen(1, None, label.clone());
    assert!(same(&then, &again(label.clone(), &[])));

    let mut relabelled = label.clone();
    relabelled.value = Some("Invoice 13".into());
    assert!(!same(&then, &again(relabelled, &[])));
    assert!(!same(&then, &again(label, &["AXValue"])));
}
