use super::said_of;

#[test]
fn what_the_station_and_the_guest_say_of_the_helper_is_said_of_the_vm() {
    for (said, of_the_vm) in [
        (
            "The helper daemon stopped unexpectedly",
            "work's daemon stopped unexpectedly",
        ),
        (
            "This helper daemon cannot keep background jobs; run atlas restart",
            "work's daemon cannot keep background jobs; run atlas restart",
        ),
        (
            "Screen Recording is not allowed for Atlas in the helper account",
            "Screen Recording is not allowed for Atlas in work",
        ),
        (
            "The helper's list of running apps was malformed",
            "work's list of running apps was malformed",
        ),
        (
            "No background job with that ID is known in the helper",
            "No background job with that ID is known in work",
        ),
        (
            "Wait until the person gives control back, then look at the helper before trying \
             again.",
            "Wait until the person gives control back, then look at work before trying again.",
        ),
        (
            "Atlas couldn't reach the helper.",
            "Atlas couldn't reach work.",
        ),
        (
            "Helper did not confirm operation completion",
            "work did not confirm operation completion",
        ),
        (
            "It may be asking in the hidden helper, (the helper's window)",
            "It may be asking in work, (work's window)",
        ),
        (
            "macOS could not open that; look with atlas apps",
            "macOS could not open that; look with atlas windows",
        ),
    ] {
        assert_eq!(said_of(said, "work"), of_the_vm, "{said}");
    }
}

#[test]
fn names_and_paths_with_helper_in_them_are_kept_as_they_are() {
    for said in [
        "BuildHelper is not running: run atlas open --app BuildHelper",
        "No window of the helperTool is on screen",
        "Read /tmp/helper/x, then the helper.app/Contents and the helper_tool",
        "It stays pending until helper-work answers; atlas --desktop helper-work status shows it.",
        "Run atlas apps-list or atlas appsy, not atlas-apps",
        "The helpers are many",
    ] {
        assert_eq!(said_of(said, "work"), said, "{said}");
    }
}

#[test]
fn a_vm_named_with_helper_in_it_is_put_in_once_and_never_read_again() {
    assert_eq!(
        said_of("Look at the helper, then the helper's apps", "the helper"),
        "Look at the helper, then the helper's apps"
    );
    assert_eq!(
        said_of("The helper daemon is restarting", "helper-work"),
        "helper-work's daemon is restarting"
    );
}
