use super::*;
use std::{
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};

#[test]
fn a_kept_key_of_the_wrong_length_is_damaged_never_replaced() {
    let error = decoded(&[7; KEY_LEN - 1]).unwrap_err();

    assert_eq!(error.code, Code::Identity);
    assert_eq!(decoded(&[7; KEY_LEN]).unwrap().as_bytes(), &[7; KEY_LEN]);
}

/// A store as slow as a Keychain, that replaces what it holds on every add,
/// so only the lock keeps two first starts from making two keys.
#[derive(Default)]
struct Slow {
    kept: Mutex<Option<[u8; KEY_LEN]>>,
    adds: AtomicUsize,
}

impl KeptOnce for Slow {
    fn load(&self) -> Result<Option<SecretKey>> {
        std::thread::sleep(Duration::from_millis(50));
        Ok(self.kept.lock().unwrap().map(SecretKey::from_bytes))
    }

    fn add(&self, secret: &SecretKey) -> Result<bool> {
        self.adds.fetch_add(1, SeqCst);
        *self.kept.lock().unwrap() = Some(*secret.as_bytes());
        Ok(true)
    }
}

#[test]
fn two_first_starts_at_once_end_with_one_key() {
    let folder = tempfile::tempdir().unwrap();
    let lock = folder.path().join(".key.lock");
    let store = Arc::new(Slow::default());
    let together = Arc::new(Barrier::new(2));

    let starts: Vec<_> = (0..2)
        .map(|_| {
            let (store, together, lock) = (store.clone(), together.clone(), lock.clone());
            std::thread::spawn(move || {
                together.wait();
                made_once(&*store, &lock).unwrap().public()
            })
        })
        .collect();
    let made: Vec<_> = starts
        .into_iter()
        .map(|start| start.join().unwrap())
        .collect();

    assert_eq!(made[0], made[1]);
    assert_eq!(store.adds.load(SeqCst), 1);
}

#[test]
fn a_kept_key_is_loaded_and_never_made_again() {
    let folder = tempfile::tempdir().unwrap();
    let lock = folder.path().join(".key.lock");
    let store = Slow::default();
    let kept = made_once(&store, &lock).unwrap();

    let again = made_once(&store, &lock).unwrap();

    assert_eq!(again.public(), kept.public());
    assert_eq!(store.adds.load(SeqCst), 1);
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn the_station_keeps_one_key_across_starts() {
    let service: &'static str =
        Box::leak(format!("com.atlas.tests.station-key.{}", std::process::id()).into_boxed_str());
    let item = Item {
        service,
        account: "station session key",
    };
    let store = StationKey::at(item);
    let folder = tempfile::tempdir().unwrap();
    let lock = folder.path().join(".key.lock");

    let first = store.own(&lock).unwrap();
    let again = store.own(&lock).unwrap();

    assert_eq!(first.public(), again.public());
    keychain::delete(item).unwrap();
}
