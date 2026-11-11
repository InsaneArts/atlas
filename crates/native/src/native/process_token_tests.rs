use super::*;
use atlas_core::{process, test_support::TestChild};
use std::{process::Command, time::Instant};

/// A child that sleeps, killed and reaped when dropped, whatever its test
/// came to.
fn sleeper() -> (TestChild, i32) {
    let child = TestChild::spawn(Command::new("/bin/sleep").arg("30"));
    let pid = child.process.id() as i32;
    (child, pid)
}

/// Whether `pid` comes to be stopped, or not, within five seconds.
fn becomes_stopped(pid: i32, stopped: bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if process::info(pid).unwrap().unwrap().stopped == stopped {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

#[test]
fn a_token_names_one_process_of_this_account_and_its_login() {
    let me = std::process::id() as i32;
    let mine = Token::of(me).unwrap();
    assert_eq!(mine.pid(), me);
    assert_eq!(mine.uid(), nix::unistd::geteuid().as_raw());

    let (child, pid) = sleeper();
    let token = Token::of(pid).unwrap();
    assert_eq!(token.pid(), pid);
    assert_eq!(token.session(), mine.session());
    assert_ne!(token, mine);
    assert_eq!(Token::of(pid).unwrap(), token);
    assert_eq!(token.program().unwrap(), PathBuf::from("/bin/sleep"));
    drop(child);
}

#[test]
fn a_signal_through_a_token_stops_and_continues_that_process() {
    let (child, pid) = sleeper();
    let token = Token::of(pid).unwrap();

    token.signal(Signal::SIGSTOP).unwrap();
    assert!(becomes_stopped(pid, true));
    token.signal(Signal::SIGCONT).unwrap();
    assert!(becomes_stopped(pid, false));
    drop(child);
}

#[test]
fn a_token_whose_process_is_gone_reaches_nothing() {
    let (child, pid) = sleeper();
    let token = Token::of(pid).unwrap();
    drop(child);

    assert!(token.signal(Signal::SIGCONT).is_err());
    assert!(token.program().is_err());
    assert!(Token::of(pid).is_err());
    assert!(Token::of(-1).is_err());
}

#[test]
fn a_process_ages_by_the_kernels_own_count() {
    let (child, pid) = sleeper();
    let token = Token::of(pid).unwrap();
    let first = token.age().unwrap();
    assert!(first < Duration::from_secs(30));
    let mine = Token::of(std::process::id() as i32).unwrap();
    assert!(mine.age().unwrap() > first);

    let end_by = Instant::now() + Duration::from_secs(5);
    while token.age().unwrap() <= first {
        assert!(Instant::now() < end_by, "the child did not age");
        std::thread::yield_now();
    }
    drop(child);
    assert!(token.age().is_err());
}

#[test]
fn an_age_is_given_only_for_the_tokens_own_use_of_its_pid() {
    let me = std::process::id() as i32;
    let mine = Token::of(me).unwrap();
    assert!(mine.age().is_ok());

    // This PID in another use: its age can be read, but it is not that
    // process's.
    let another = Token::made_up(me, 0, mine.uid(), mine.session());
    assert_ne!(another, mine);
    assert!(another.age().is_err());
}

#[test]
fn a_made_up_token_reads_as_it_was_made() {
    let token = Token::made_up(40, 7, 503, 99);
    assert_eq!((token.pid(), token.uid(), token.session()), (40, 503, 99));
    assert_eq!(token, Token::made_up(40, 7, 503, 99));
    assert_ne!(token, Token::made_up(40, 8, 503, 99));
}
