use super::*;
use atlas_core::test_support::TestChild;
use std::{process::Command, sync::Arc, time::Instant};

/// A child that sleeps, killed and reaped when dropped, whatever its test
/// came to.
fn sleeper() -> (TestChild, i32) {
    let child = TestChild::spawn(Command::new("/bin/sleep").arg("30"));
    let pid = child.process.id() as i32;
    (child, pid)
}

/// Whether `wanted` is told within five seconds.
fn told(watch: &Watch, wanted: Event) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if watch
            .wait(Duration::from_millis(200))
            .unwrap()
            .contains(&wanted)
        {
            return true;
        }
    }
    false
}

#[test]
fn a_parents_start_of_a_process_is_told() {
    let watch = Watch::new().unwrap();
    let me = std::process::id() as i32;
    watch.spawns(me).unwrap();

    let _child = sleeper();
    assert!(told(&watch, Event::Spawned(me)));
}

#[test]
fn a_watched_process_tells_when_it_becomes_another_program_and_when_it_ends() {
    let watch = Watch::new().unwrap();
    // bash itself, as macOS's sh becomes another shell when it starts.
    let mut child =
        TestChild::spawn(Command::new("/bin/bash").args(["-c", "read line; exec /bin/sleep 30"]));
    let pid = child.process.id() as i32;
    watch.process(pid).unwrap();
    assert_eq!(watch.wait(Duration::from_millis(50)).unwrap(), []);

    child.permit("go");
    assert!(told(&watch, Event::Became(pid)));
    let name = atlas_core::process::info(pid).unwrap().unwrap().name;
    assert_eq!(name, "sleep");

    drop(child);
    assert!(told(&watch, Event::Ended(pid)));
}

#[test]
fn one_notice_can_tell_that_a_process_became_another_program_and_ended() {
    let notice = KEvent::new(
        40,
        EventFilter::EVFILT_PROC,
        EvFlags::empty(),
        FilterFlag::NOTE_EXEC | FilterFlag::NOTE_EXIT,
        0,
        0,
    );
    assert_eq!(events(&notice), [Event::Became(40), Event::Ended(40)]);

    let wake = KEvent::new(
        WAKE,
        EventFilter::EVFILT_USER,
        EvFlags::empty(),
        FilterFlag::empty(),
        0,
        0,
    );
    assert_eq!(events(&wake), [Event::Woken]);
}

#[test]
fn a_forgotten_process_tells_nothing() {
    let watch = Watch::new().unwrap();
    let (child, pid) = sleeper();
    watch.process(pid).unwrap();
    watch.forget(pid);

    drop(child);
    assert_eq!(watch.wait(Duration::from_millis(100)).unwrap(), []);
}

#[test]
fn a_process_that_is_gone_cannot_be_watched() {
    let watch = Watch::new().unwrap();
    let (child, pid) = sleeper();
    drop(child);

    assert!(watch.process(pid).is_err());
    assert!(watch.spawns(pid).is_err());
    assert!(watch.process(-1).is_err());
    watch.forget(pid);
}

#[test]
fn a_wake_ends_the_wait_from_another_thread() {
    let watch = Arc::new(Watch::new().unwrap());
    let waker = watch.clone();
    let woke = std::thread::spawn(move || waker.wake());

    // A wake before the wait began is kept for it, so either order ends it.
    let began = Instant::now();
    assert_eq!(watch.wait(Duration::from_secs(10)).unwrap(), [Event::Woken]);
    assert!(began.elapsed() < Duration::from_secs(5));
    woke.join().unwrap();
    assert_eq!(watch.wait(Duration::from_millis(10)).unwrap(), []);
}

#[test]
fn a_wait_with_nothing_to_tell_ends_empty_when_its_time_is_up() {
    let watch = Watch::new().unwrap();
    let began = Instant::now();
    assert_eq!(watch.wait(Duration::from_millis(30)).unwrap(), []);
    assert!(began.elapsed() >= Duration::from_millis(25));
}
