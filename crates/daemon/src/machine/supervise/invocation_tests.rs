use super::*;

fn invocation() -> Invocation {
    Invocation {
        lease: Duration::from_millis(2000),
        argv0: "echo".into(),
        program: "/bin/echo".into(),
        arguments: vec!["--".into(), "a b".into(), "".into()],
    }
}

#[test]
fn an_invocation_reads_back_from_its_words() {
    let invocation = invocation();

    assert_eq!(Invocation::parse(&invocation.words()), Some(invocation));
}

#[test]
fn words_that_are_not_an_invocation_are_refused() {
    let refused: [&[&str]; 6] = [
        &[],
        &["2000", "echo"],
        &["+2000", "echo", "/bin/echo"],
        &["0", "echo", "/bin/echo"],
        &["2000", "echo", "bin/echo"],
        &["2000", "", "/bin/echo"],
    ];
    for words in refused {
        let words: Vec<OsString> = words.iter().map(OsString::from).collect();
        assert_eq!(Invocation::parse(&words), None, "{words:?}");
    }
}

#[test]
fn a_supervisor_is_started_with_its_own_words_then_the_invocation() {
    let supervisor = Supervisor::new(
        "/usr/local/bin/atlas".into(),
        vec!["machine-supervise".into(), "--".into()],
    );

    let arguments = supervisor.arguments(&invocation());

    assert_eq!(
        arguments[..2],
        [OsString::from("machine-supervise"), OsString::from("--")]
    );
    assert_eq!(Invocation::parse(&arguments[2..]), Some(invocation()));
}

#[test]
fn a_told_end_is_heard_as_it_was() {
    for status in [
        ExitStatus::from_raw(0),
        ExitStatus::from_raw(1 << 8),
        ExitStatus::from_raw(15),
    ] {
        let frame = told(status);

        assert!(frame.len() <= FRAME);
        assert_eq!(heard(&frame), Some(status));
    }
}

#[test]
fn a_frame_that_is_not_whole_or_well_formed_is_not_heard() {
    for frame in [
        &b"status 0"[..],
        b"status \n",
        b"status x\n",
        b"status +1\n",
        b"state 0\n",
        b"status 0\n\n",
        b"",
    ] {
        assert_eq!(heard(frame), None, "{frame:?}");
    }
}
