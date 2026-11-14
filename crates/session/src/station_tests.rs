use super::*;
use crate::{
    authority::Retired,
    lane::LaneKind,
    standin::{
        link::{Conditions, Link},
        serve::{Answer, Reached, reach},
    },
};
use atlas_core::files::Token;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A stand-in across a healthy link that answers each request with its
/// lane's kind.
async fn reached() -> Reached {
    let answer: Answer =
        Arc::new(|header, request| json!({"ok": true, "kind": header.kind, "request": request}));
    reach(&Arc::new(Link::new(Conditions::default())), answer).await
}

/// The one reply `opened` gives to `request`.
async fn ask(opened: Opened, request: &Value) -> std::io::Result<Value> {
    let mut stream = opened.stream;
    let mut line = serde_json::to_vec(request).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await?;
    stream.flush().await?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).await?;
    if reply.is_empty() {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(serde_json::from_str(&reply).unwrap())
}

#[tokio::test]
async fn a_lane_names_its_generation_id_and_kind_and_carries_one_request() {
    let reached = reached().await;
    let change = Token::default();

    let read = reached.desktop.open(LaneKind::Read).await.unwrap();
    let changing = reached
        .desktop
        .open(LaneKind::Change {
            change: change.clone(),
        })
        .await
        .unwrap();
    assert_ne!(read.lane, changing.lane);
    let read_reply = ask(read, &json!({"op": "hello"})).await.unwrap();
    let change_reply = ask(changing, &json!({"op": "run"})).await.unwrap();

    assert_eq!(read_reply["kind"], "read");
    assert_eq!(read_reply["request"]["op"], "hello");
    assert_eq!(change_reply["kind"]["change"]["change"], json!(change));
    let admitted = reached.admitted.lock().unwrap().clone();
    assert_eq!(admitted.len(), 2);
    assert!(
        admitted
            .iter()
            .all(|header| header.generation == reached.live.authority.generation())
    );
}

#[tokio::test]
async fn a_lane_of_a_retired_generation_is_refused_before_its_request() {
    let reached = reached().await;
    reached.live.authority.retire(Retired::Revoked);

    let opened = reached.desktop.open(LaneKind::Read).await.unwrap();

    assert!(ask(opened, &json!({"op": "hello"})).await.is_err());
    assert!(reached.admitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_cancel_reaches_the_desktop_over_control() {
    let reached = reached().await;
    let opened = reached.desktop.open(LaneKind::Read).await.unwrap();

    reached.desktop.cancel(opened.lane);

    tokio::time::timeout(Duration::from_secs(5), async {
        while !reached.live.lanes.cancelled(opened.lane) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
