use super::*;

fn running() -> Record {
    Record {
        version: VERSION,
        id: JobId::parse("7c2e91a04b3f").unwrap(),
        name: "nc".into(),
        task: None,
        session: None,
        uid: 502,
        boot: "6A1E0000-0000-0000-0000-000000000000".into(),
        pid: Some(4312),
        start: Some([1_761_650_000, 123_456]),
        pgid: Some(4312),
        state: JobState::Running,
        started_at: 1,
        ends_at: u64::MAX,
        adopted: false,
        seen: Vec::new(),
        stopping: None,
        ended: None,
        log_resets: 0,
    }
}

fn stopping() -> Stopping {
    Stopping::new(How::Stopped, 5, 1000)
}

fn ended() -> Ended {
    Ended {
        how: How::Stopped,
        at: 9,
        signal: Some("SIGTERM".into()),
        exit_code: None,
        exit_signal: None,
        left: Vec::new(),
    }
}

fn with(state: JobState, stopping: Option<Stopping>, ended: Option<Ended>) -> Record {
    Record {
        state,
        stopping,
        ended,
        ..running()
    }
}

#[test]
fn a_stopping_job_without_stop_data_is_not_a_sound_record() {
    let record = with(JobState::Stopping, None, None);
    assert!(record.identity().is_some(), "its process identity is sound");
    assert!(!record.sound());
    assert!(with(JobState::Stopping, Some(stopping()), None).sound());
}

#[test]
fn a_live_job_with_completion_data_is_not_a_sound_record() {
    for (state, stopping, ended) in [
        (JobState::Launched, Some(stopping()), None),
        (JobState::Launched, None, Some(ended())),
        (JobState::Running, Some(stopping()), None),
        (JobState::Running, None, Some(ended())),
        (JobState::Running, Some(stopping()), Some(ended())),
        (JobState::Stopping, None, Some(ended())),
        (JobState::Stopping, Some(stopping()), Some(ended())),
        (JobState::Ended, None, None),
        (JobState::Ended, Some(stopping()), None),
    ] {
        let record = with(state, stopping.clone(), ended.clone());
        assert!(
            !record.sound(),
            "{state:?} with stopping {} and ended {}",
            stopping.is_some(),
            ended.is_some()
        );
    }
    for state in [JobState::Launched, JobState::Running] {
        assert!(with(state, None, None).sound(), "{state:?}");
    }
}

#[test]
fn an_ended_job_may_retain_its_stopping_history() {
    assert!(with(JobState::Ended, None, Some(ended())).sound());
    assert!(with(JobState::Ended, Some(stopping()), Some(ended())).sound());
}

#[test]
fn a_persisted_starting_job_is_never_a_sound_record() {
    let mut record = with(JobState::Starting, None, None);
    (record.pid, record.start, record.pgid) = (None, None, None);
    assert!(!record.sound());
}
