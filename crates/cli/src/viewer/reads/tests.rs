use super::*;

#[tokio::test]
async fn an_account_title_is_asked_again_after_no_answer_and_kept_once_answered() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        match ASKED.fetch_add(1, Ordering::SeqCst) {
            0 => Title::Unknown,
            1 => Title::Titled("Atlas".to_owned()),
            _ => Title::Titled("Not asked".to_owned()),
        }
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(
        account_title(&known, "atlas", ask).await.as_deref(),
        Some("Atlas")
    );
    assert_eq!(ASKED.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_account_with_no_full_name_is_asked_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ASKED: AtomicUsize = AtomicUsize::new(0);
    fn ask(_: &str) -> Title {
        ASKED.fetch_add(1, Ordering::SeqCst);
        Title::Untitled
    }
    let known = std::sync::Mutex::new(None);

    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(account_title(&known, "atlas", ask).await, None);
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
}
