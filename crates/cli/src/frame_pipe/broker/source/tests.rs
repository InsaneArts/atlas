use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicU8, Ordering},
};

/// What a scripted source did, in order.
#[derive(Default)]
struct Log(Mutex<Vec<&'static str>>);

impl Log {
    fn take(&self) -> Vec<&'static str> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

/// A source that makes 2 × 1 pictures of one shade, or fails when told to.
struct Scripted {
    log: Arc<Log>,
    fail: Arc<Mutex<Option<Reason>>>,
    shade: Arc<AtomicU8>,
    sequence: u64,
}

impl Acquire for Scripted {
    async fn acquire(&mut self, budget: &Budget) -> Result<Picture, Reason> {
        self.log.0.lock().unwrap().push("acquire");
        if let Some(reason) = *self.fail.lock().unwrap() {
            return Err(reason);
        }
        self.sequence += 1;
        let mut pixels = FramePixels::reserve(budget, 2, 1)?;
        pixels
            .as_mut_slice()
            .fill(self.shade.load(Ordering::SeqCst));
        Ok(Picture {
            pixels,
            sequence: Positive::new(self.sequence).unwrap(),
            at: Instant::now(),
            display: [
                Measure::new(2.0).unwrap(),
                Measure::new(1.0).unwrap(),
                Measure::new(1.0).unwrap(),
            ],
            backend: Source::Cg,
            epoch: atlas_core::session::Epoch {
                uid: 501,
                capture_pid: 1,
                display: atlas_core::session::Display {
                    width: 2.0,
                    height: 1.0,
                    scale: 1.0,
                },
                security_session_id: 1,
                display_id: 1,
                input_pid: 1,
            },
        })
    }

    async fn stop(&mut self) -> Result<(), Reason> {
        self.log.0.lock().unwrap().push("stop");
        Ok(())
    }
}

fn scripted() -> (Scripted, Arc<Log>, Arc<Mutex<Option<Reason>>>) {
    let log = Arc::new(Log::default());
    let fail = Arc::new(Mutex::new(None));
    let source = Scripted {
        log: log.clone(),
        fail: fail.clone(),
        shade: Arc::default(),
        sequence: 0,
    };
    (source, log, fail)
}

/// A notice no input ever gives.
fn quiet() -> Arc<Notify> {
    Arc::new(Notify::new())
}

/// The next picture, asked for now, and when it was answered.
async fn next(handle: &mut Handle) -> (Arc<Picture>, tokio::time::Instant) {
    let demand = handle.want();
    let Latest::Picture(picture) = answer(handle, demand).await.latest else {
        panic!("no picture");
    };
    (picture, tokio::time::Instant::now())
}

/// The time between each of `count` pictures asked for one after another.
async fn gaps(handle: &mut Handle, count: usize) -> Vec<Duration> {
    let (_, mut previous) = next(handle).await;
    let mut gaps = Vec::new();
    for _ in 0..count {
        let (_, at) = next(handle).await;
        gaps.push(at - previous);
        previous = at;
    }
    gaps
}

/// The next answer to demand `demand` or a later one.
async fn answer(handle: &mut Handle, demand: u64) -> Answer {
    handle
        .answers
        .wait_for(|answer| answer.demand >= demand)
        .await
        .unwrap()
        .clone()
}

#[tokio::test(start_paused = true)]
async fn each_demand_gets_an_acquisition_begun_after_it() {
    let (source, log, _) = scripted();
    let budget = Budget::new(1024);
    let mut handle = spawn(source, budget.clone(), quiet());
    let first = handle.want();
    let Latest::Picture(picture) = answer(&mut handle, first).await.latest else {
        panic!("no picture");
    };
    assert_eq!(picture.sequence.get(), 1);
    assert_eq!(budget.reserved(), 6, "the picture holds its charge");

    // Demands made together share the next acquisition.
    let (second, third) = (handle.want(), handle.want());
    let Latest::Picture(picture) = answer(&mut handle, third).await.latest else {
        panic!("no picture");
    };
    assert!(second < third);
    assert_eq!(picture.sequence.get(), 2);
    assert_eq!(log.take(), ["acquire", "acquire"]);
}

#[tokio::test(start_paused = true)]
async fn acquisitions_keep_their_cadence() {
    let (source, _, _) = scripted();
    let mut handle = spawn(source, Budget::new(1024), quiet());
    let began = tokio::time::Instant::now();
    for _ in 0..4 {
        let demand = handle.want();
        answer(&mut handle, demand).await;
    }
    assert!(began.elapsed() >= 3 * CADENCE, "{:?}", began.elapsed());
}

#[tokio::test(start_paused = true)]
async fn a_failure_is_the_answer_and_the_next_demand_tries_again() {
    let (source, _, fail) = scripted();
    let mut handle = spawn(source, Budget::new(1024), quiet());
    *fail.lock().unwrap() = Some(Reason::SourceStale);
    let demand = handle.want();
    assert!(matches!(
        answer(&mut handle, demand).await.latest,
        Latest::Failed(Reason::SourceStale)
    ));
    *fail.lock().unwrap() = None;
    let demand = handle.want();
    assert!(matches!(
        answer(&mut handle, demand).await.latest,
        Latest::Picture(_)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_budget_that_cannot_hold_a_picture_fails_it() {
    let (source, _, _) = scripted();
    let mut handle = spawn(source, Budget::new(5), quiet());
    let demand = handle.want();
    assert!(matches!(
        answer(&mut handle, demand).await.latest,
        Latest::Failed(Reason::BudgetExhausted)
    ));
}

#[tokio::test(start_paused = true)]
async fn without_demand_the_stream_is_given_back_and_closing_gives_it_back_too() {
    let (source, log, _) = scripted();
    let mut handle = spawn(source, Budget::new(1024), quiet());
    let demand = handle.want();
    answer(&mut handle, demand).await;
    tokio::time::sleep(IDLE + 2 * CADENCE).await;
    assert!(matches!(handle.answers.borrow().latest, Latest::Idle));
    assert_eq!(log.take(), ["acquire", "stop"]);
    tokio::time::sleep(IDLE * 3).await;
    assert!(log.take().is_empty(), "an idle source stays quiet");
    handle.close().await;
    assert_eq!(log.take(), ["stop"]);
}

#[tokio::test(start_paused = true)]
async fn a_still_screen_is_read_less_often_but_never_less_than_four_times_a_second() {
    let (source, log, _) = scripted();
    let mut handle = spawn(source, Budget::new(1024), quiet());
    let still_after = STILL_AFTER as usize;
    let gaps = gaps(&mut handle, 4 * still_after).await;
    let (moving, still) = gaps.split_at(still_after);
    assert!(moving.iter().all(|gap| *gap == CADENCE), "{moving:?}");
    assert!(still.iter().all(|gap| *gap == STILL_CADENCE), "{still:?}");
    // Each picture is still read anew for the demand it answers.
    assert_eq!(log.take().len(), 4 * still_after + 1);
}

#[tokio::test(start_paused = true)]
async fn a_changed_picture_brings_the_normal_cadence_back() {
    let (source, _, _) = scripted();
    let shade = source.shade.clone();
    let mut handle = spawn(source, Budget::new(1024), quiet());
    let slow = gaps(&mut handle, STILL_AFTER as usize + 1).await;
    assert_eq!(slow.last(), Some(&STILL_CADENCE));
    shade.store(1, Ordering::SeqCst);
    assert_eq!(gaps(&mut handle, 2).await, [CADENCE, CADENCE]);
}

#[tokio::test(start_paused = true)]
async fn input_brings_the_normal_cadence_back_at_once() {
    let (source, _, _) = scripted();
    let input = Arc::new(Notify::new());
    let mut handle = spawn(source, Budget::new(1024), input.clone());
    let slow = gaps(&mut handle, STILL_AFTER as usize + 1).await;
    assert_eq!(slow.last(), Some(&STILL_CADENCE));
    let wanted_at = tokio::time::Instant::now();
    let demand = handle.want();
    input.notify_one();
    answer(&mut handle, demand).await;
    assert_eq!(wanted_at.elapsed(), CADENCE);
    // The same picture again is not yet a still screen.
    let after = gaps(&mut handle, 3).await;
    assert!(after.iter().all(|gap| *gap == CADENCE), "{after:?}");
}

#[tokio::test(start_paused = true)]
async fn pictures_of_a_still_screen_are_each_new_and_a_failure_lets_the_last_go() {
    let (source, _, fail) = scripted();
    let budget = Budget::new(1024);
    let mut handle = spawn(source, budget.clone(), quiet());
    let mut sequence = 0;
    for _ in 0..2 * STILL_AFTER {
        let (picture, _) = next(&mut handle).await;
        assert_eq!(picture.sequence.get(), sequence + 1, "never offered again");
        sequence = picture.sequence.get();
    }
    *fail.lock().unwrap() = Some(Reason::SourceStale);
    let demand = handle.want();
    answer(&mut handle, demand).await;
    assert_eq!(budget.reserved(), 0, "no picture is held after a failure");
    *fail.lock().unwrap() = None;
    assert_eq!(gaps(&mut handle, 1).await, [CADENCE]);
}
