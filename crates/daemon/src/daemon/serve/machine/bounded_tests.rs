use super::*;
use crate::machine::{pairing::Pairing, vault::Memory};

fn records(permits: usize) -> Bounded {
    let folder = std::env::temp_dir().join("atlas-bounded-tests");
    Bounded {
        records: Arc::new(Pairing::new(Memory::default(), folder.join("secret"))),
        permits: Arc::new(Semaphore::new(permits)),
    }
}

#[test]
fn records_are_read_while_a_permit_is_free_and_refused_as_busy_once_none_is() {
    assert_eq!(records(1).committed().unwrap(), None);

    let busy = records(0).committed().unwrap_err();
    assert_eq!(busy.code, Code::Busy);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_off_the_runtime_holds_its_permit_until_it_returns() {
    let permits = Arc::new(Semaphore::new(1));
    let (started, wait) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let running = tokio::spawn({
        let permits = permits.clone();
        async move {
            off_runtime(&permits, move || {
                started.send(()).unwrap();
                released.recv().unwrap();
            })
            .await
        }
    });
    wait.recv().unwrap();
    // Whoever waited gives up; the work runs on, holding its permit.
    running.abort();
    assert!(off_runtime(&permits, || ()).await.is_none());

    release.send(()).unwrap();
    for _ in 0..200 {
        if permits.available_permits() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(off_runtime(&permits, || 7).await, Some(7));
}
