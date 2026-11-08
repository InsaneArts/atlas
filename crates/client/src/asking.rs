//! Asking the person before an agent sends from an account they listed.
//!
//! An agent's input carries the person's accounts and the grants that may
//! cover it, only while the person lists any: with none, the request is
//! byte for byte what it always was. The input service judges the whole
//! request before its first event; a refusal to ask has sent nothing.
//!
//! Then the agent's command lets go of control and of the input service's
//! grant, puts the question up (or joins the one this session already
//! asks), and waits for the person, Stop or the rest of its wait, sleeping
//! on the asks folder. Once allowed, the command runs again from the start:
//! control taken again, the accounts and grants read again, the target
//! looked at afresh, and the reply is the ordinary one.
use crate::client::Client;
use atlas_control::asks::{Answer, Ask, Waited};
use atlas_core::{
    error::{Code, Error, Result},
    time::now_ms,
};
use serde_json::{Value, json};
use std::time::Instant;

/// What an agent's input carries for the input service to judge it by:
/// none while the person lists no account.
pub(crate) fn policy(client: &Client) -> Option<Value> {
    policy_by(client, crate::sign_ins::listed())
}

/// [`policy`] with the person's list as `listed` says it.
fn policy_by(client: &Client, listed: crate::sign_ins::Listed) -> Option<Value> {
    use crate::sign_ins::Listed;
    let now = now_ms();
    let session = session(client);
    let mut grants: Vec<Value> = client
        .controller
        .asks()
        .allowed(now)
        .into_iter()
        .filter(|allowed| allowed.session == session)
        .map(|allowed| json!({"scope": "task", "account": allowed.account}))
        .collect();
    grants.extend(client.granted());
    let mut policy = match listed {
        Listed::Accounts(accounts) if accounts.is_empty() => return None,
        Listed::Accounts(accounts) => json!({"accounts": accounts, "grants": grants}),
        // A list that can't be read never reads as none: every app asks.
        Listed::Unknown => json!({"unknown": true, "grants": grants}),
    };
    if client.checking() {
        policy["dry"] = json!(true);
    }
    Some(policy)
}

/// The agent session asking, by its key; `terminal` for none.
fn session(client: &Client) -> String {
    client
        .host_session
        .as_ref()
        .map_or_else(|| "terminal".to_owned(), |host| host.key().to_string())
}

/// Who asks, as the person reads it: the agent, and the project it works
/// in, such as "Claude Code · atlas".
fn who(client: &Client) -> String {
    let Some(label) = client
        .host_session
        .as_ref()
        .map(atlas_core::host::HostSession::label)
    else {
        return "An agent".to_owned();
    };
    match label.folder {
        Some(folder) => format!("{} · {folder}", label.host.label()),
        None => label.host.label().to_owned(),
    }
}

/// Run `attempt`, the whole command, again for as long as the person must
/// be asked and answers yes in time. Each attempt takes control itself and
/// lets it go before returning, so nothing is held while the person reads.
///
/// # Errors
///
/// Returns `waiting-for-person` when the person hasn't answered in time
/// (the question stays up), `denied` when they said no, `stopped` when
/// they stopped agents, and whatever the command itself returns.
pub async fn after_asking(
    client: &Client,
    mut attempt: impl AsyncFnMut() -> Result<Value>,
) -> Result<Value> {
    let until = Instant::now() + client.controller.remaining();
    loop {
        let outcome = attempt().await;
        client.clear_granted();
        let Err(error) = &outcome else {
            return outcome;
        };
        // Only a refusal that sent nothing becomes a question.
        if error.code != Code::WaitingForPerson || error.uncertain {
            return outcome;
        }
        let question = error.details.get("ask").cloned().unwrap_or(Value::Null);
        let asks = client.controller.asks();
        let ask = Ask {
            id: String::new(),
            session: session(client),
            who: who(client),
            task: client
                .task
                .as_ref()
                .and_then(|task| task.name().map(ToString::to_string)),
            account: question["account"]
                .as_str()
                .unwrap_or("this account")
                .to_owned(),
            question: question.clone(),
            asked: 0,
            ends: 0,
        };
        let id = asks.ask(ask, now_ms())?;
        let waited = tokio::task::spawn_blocking({
            let asks = asks.clone();
            let id = id.clone();
            move || asks.wait(&id, until)
        })
        .await
        .map_err(|cause| {
            Error::new(Code::State, "The wait for the person ended").with_source(cause)
        })??;
        match waited {
            Waited::Unanswered => return outcome,
            Waited::Stopped => return Err(Error::stopped()),
            Waited::Answered(Answer::Deny) => {
                asks.close(&id);
                return Err(Error::new(
                    Code::Denied,
                    format!(
                        "The person said no to sending this in {}. Nothing was sent.",
                        question["account"].as_str().unwrap_or("this account")
                    ),
                ));
            }
            Waited::Answered(Answer::Task) => asks.close(&id),
            // A command's check before typing covers the rest of that one
            // command; a request's own ask covers that request alone.
            Waited::Answered(Answer::Once) if question["dry"] == true => {
                asks.close(&id);
                client.grant(json!({"scope": "command", "account": question["account"]}));
            }
            Waited::Answered(Answer::Once) => {
                asks.close(&id);
                client.grant(json!({
                    "scope": "once",
                    "account": question["account"],
                    "batch": question["batch"],
                    "value": question["message"]["digest"],
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests;
