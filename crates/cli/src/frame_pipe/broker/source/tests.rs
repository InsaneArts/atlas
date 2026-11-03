use super::*;
use std::sync::Mutex;

/// What a scripted source did, in order.
#[derive(Default)]
struct Log(Mutex<Vec<&'static str>>);

impl Log {
    fn take(&self) -> Vec<&'static str> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

/// A source that makes 2 × 1 pictures, or fails when told to.
struct Scripted {
    log: Arc<Log>,
    fail: Arc<Mutex<Option<Reason>>>,
    sequence: u64,
}

impl Acquire for Scripted {
    async fn acquire(&mut self, budget: &Budget) -> Result<Picture, Reason> {
        self.log.0.lock().unwrap().push("acquire");
        if let Some(reason) = *self.fail.lock().unwrap() {
            return Err(reason);
        }
        self.sequence += 1;
        Ok(Picture {
            pixels: FramePixels::reserve(budget, 2, 1)?,
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
        sequence: 0,
    };
    (source, log, fail)
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
    let mut handle = spawn(source, budget.clone());
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
    let mut handle = spawn(source, Budget::new(1024));
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
    let mut handle = spawn(source, Budget::new(1024));
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
    let mut handle = spawn(source, Budget::new(5));
    let demand = handle.want();
    assert!(matches!(
        answer(&mut handle, demand).await.latest,
        Latest::Failed(Reason::BudgetExhausted)
    ));
}

#[tokio::test(start_paused = true)]
async fn without_demand_the_stream_is_given_back_and_closing_gives_it_back_too() {
    let (source, log, _) = scripted();
    let mut handle = spawn(source, Budget::new(1024));
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
