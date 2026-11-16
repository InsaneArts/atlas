use super::*;

#[test]
fn a_store_with_no_key_is_given_one_that_it_keeps() {
    let store = MemoryStore::default();

    let first = own_key(&store).unwrap();
    let again = own_key(&store).unwrap();

    assert_eq!(first.public(), again.public());
}

#[test]
fn a_key_pair_rebuilt_from_its_secret_has_the_same_public_key() {
    let made = Keypair::generate().unwrap();

    let rebuilt = Keypair::from_secret(SecretKey::from_bytes(*made.secret())).unwrap();

    assert_eq!(rebuilt.public(), made.public());
}

#[test]
fn two_new_keys_differ() {
    assert_ne!(
        Keypair::generate().unwrap().public(),
        Keypair::generate().unwrap().public()
    );
}

#[test]
fn a_secret_is_never_printed() {
    let made = Keypair::generate().unwrap();
    let secret = PairingSecret::generate().unwrap();

    let printed = format!("{made:?} {secret:?}");

    assert!(!printed.contains(&format!("{:?}", made.secret())));
    assert!(!printed.contains(&format!("{:?}", secret.as_bytes())));
    assert!(printed.contains(&made.public().fingerprint()[..16]));
}

/// A store that keeps one key at most, and counts what was added.
#[derive(Default)]
struct Once {
    kept: std::sync::Mutex<Option<[u8; KEY_LEN]>>,
    /// Another end keeps its key between this one's load and add.
    beaten_by: std::sync::Mutex<Option<[u8; KEY_LEN]>>,
}

impl KeptOnce for Once {
    fn load(&self) -> Result<Option<SecretKey>> {
        Ok(self.kept.lock().unwrap().map(SecretKey::from_bytes))
    }

    fn add(&self, secret: &SecretKey) -> Result<bool> {
        let mut kept = self.kept.lock().unwrap();
        if let Some(other) = self.beaten_by.lock().unwrap().take() {
            *kept = Some(other);
        }
        if kept.is_some() {
            return Ok(false);
        }
        *kept = Some(*secret.as_bytes());
        Ok(true)
    }
}

#[test]
fn a_key_kept_once_is_loaded_ever_after() {
    let store = Once::default();

    let first = kept_once(&store).unwrap();
    let again = kept_once(&store).unwrap();

    assert_eq!(first.public(), again.public());
}

#[test]
fn a_key_another_end_kept_meanwhile_is_taken_never_replaced() {
    let other = Keypair::generate().unwrap();
    let store = Once::default();
    *store.beaten_by.lock().unwrap() = Some(*other.secret());

    let taken = kept_once(&store).unwrap();

    assert_eq!(taken.public(), other.public());
    assert_eq!(store.kept.lock().unwrap().unwrap(), *other.secret());
}
