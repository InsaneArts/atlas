use super::*;
use crate::test_support::{Helper, completed};

#[tokio::test]
async fn sixty_five_installs_are_asked_about_once_each_within_the_workers_limit() {
    let directory = tempfile::tempdir().unwrap();
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    let (client, helper) = Helper::staging(directory.path(), nobody.uid.as_raw());
    let ids: Vec<Token> = (0..65).map(|_| Token::new()).collect();
    let mut asked: Vec<usize> = Vec::new();
    let twice: Vec<Token> = ids.iter().chain(ids.iter().take(10)).cloned().collect();

    let answer = helper
        .serve(install_states(&client, &twice), |request| {
            let packet: Value =
                serde_json::from_slice(&fs::read(request["argv"][2].as_str()?).ok()?).ok()?;
            let batch = packet["request"]["ids"].as_array()?.clone();
            asked.push(batch.len());
            let listed: Vec<Value> = batch
                .iter()
                .map(|id| json!({"installId": id, "phase": "missing"}))
                .collect();
            Some(completed(
                &json!({"ok": true, "installs": listed}).to_string(),
            ))
        })
        .await
        .unwrap();

    assert_eq!(asked, [ASKED, 1]);
    assert_eq!(answer.len(), 65);
}
