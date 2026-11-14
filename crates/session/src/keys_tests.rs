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
