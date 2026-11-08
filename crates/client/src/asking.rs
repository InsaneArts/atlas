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
    // Tests never read the person's own accounts on the Mac running them,
    // this crate's or any crate's built with its test support.
    if cfg!(any(test, feature = "test-support")) {
        return None;
    }
    policy_by(client, crate::sign_ins::listed())
}

/// [`policy`] with the person's list as `listed` says it.
fn policy_by(client: &Client, listed: crate::sign_ins::Listed) -> Option<Value> {
    use crate::sign_ins::Listed;
    let now = now_ms();
    let session = session(client);
    let process = process(client);
    let mut grants: Vec<Value> = client
        .controller
        .asks()
        .allowed(now)
        .into_iter()
        .filter(|allowed| allowed.session == session && allowed.process == process)
        .filter(|allowed| lives(client, allowed))
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
    if let Some(typing) = client.typing() {
        policy["typing"] = json!(typing);
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

/// The host process the agent session runs in, when it is known.
fn process(client: &Client) -> Option<atlas_core::process::Instance> {
    client
        .host_session
        .as_ref()
        .and_then(atlas_core::host::HostSession::process)
}

/// Whether Allow for This Task, `allowed`, still holds: the host process it
/// was given in still runs, and its agent session's record, ended ones
/// included, neither ended nor moved to another host process. A session key
/// resumed in another process gets none of it.
pub fn lives(client: &Client, allowed: &atlas_control::asks::Allowed) -> bool {
    let record = atlas_core::host::Key::parse(&allowed.session).and_then(|key| {
        let output = client.output_dir().ok()?;
        crate::host_session::store::Store::at(output).read(&key)
    });
    allowed.process.is_none_or(|process| process.running())
        && record.is_none_or(|record| {
            record.ended_at().is_none()
                && record
                    .host_process
                    .is_none_or(|recorded| Some(recorded) == allowed.process)
        })
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
    attempt: impl AsyncFnMut() -> Result<Value>,
) -> Result<Value> {
    let outcome = asking(client, attempt).await;
    // What the person allowed once was for this command alone.
    client.clear_granted();
    outcome
}

/// [`after_asking`]. What the person allowed once is kept across the
/// attempts that sent nothing, so several places in one request can each
/// be allowed; the command's end spends it.
async fn asking(client: &Client, mut attempt: impl AsyncFnMut() -> Result<Value>) -> Result<Value> {
    let until = Instant::now() + client.controller.remaining();
    loop {
        let outcome = attempt().await;
        // Only a refusal that sent nothing becomes a question.
        let Some(question) = asked(&outcome) else {
            return private(outcome);
        };
        let asks = client.controller.asks();
        let ask = Ask {
            id: String::new(),
            session: session(client),
            process: process(client),
            who: who(client),
            task: client
                .task
                .as_ref()
                .and_then(|task| task.name().map(ToString::to_string)),
            account: question["account"]
                .as_str()
                .unwrap_or("this account")
                .to_owned(),
            question,
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
        // What the person saw is what they answered: the grant comes from
        // the stored question, never from what a rerun read. Just This Once
        // goes to the one command that claims it first; another joined to
        // the same question asks again.
        let stored = match waited {
            Waited::Answered(Answer::Once) => match asks.claim_once(&id, now_ms())? {
                Some(stored) => stored,
                None => continue,
            },
            Waited::Answered(Answer::Task | Answer::Deny)
            | Waited::Unanswered
            | Waited::Stopped => match asks.open(now_ms()).into_iter().find(|ask| ask.id == id) {
                Some(stored) => stored,
                None => return private(outcome),
            },
        };
        let question = stored.question;
        match waited {
            Waited::Unanswered => return private(outcome),
            Waited::Stopped => return Err(Error::stopped()),
            Waited::Answered(Answer::Deny) => {
                asks.close(&id);
                return Err(Error::new(
                    Code::Denied,
                    format!(
                        "The person said no to sending this in {}. Nothing was sent.",
                        stored.account
                    ),
                ));
            }
            Waited::Answered(Answer::Task) => asks.close(&id),
            // A command's check before typing covers that command's own
            // text, typed into the field it was asked about; a request's own
            // ask covers that request alone, where it was asked about.
            Waited::Answered(Answer::Once) if question["dry"] == true => {
                client.grant(json!({
                    "scope": "command",
                    "account": question["accountKey"],
                    "target": question["bound"],
                    "typing": question["typing"],
                }));
            }
            Waited::Answered(Answer::Once) => {
                client.grant(json!({
                    "scope": "once",
                    "account": question["accountKey"],
                    "batch": question["batch"],
                    "target": question["bound"],
                    "value": question["message"]["digest"],
                }));
            }
        }
    }
}

/// The question in `outcome` when it is a refusal to ask the person that
/// sent nothing, whether it came back as an error or as a reply saying so.
fn asked(outcome: &Result<Value>) -> Option<Value> {
    let (code, uncertain, performed, ask) = match outcome {
        Ok(reply) if reply["ok"] == false => (
            Code::of_reply(reply, Code::Native),
            reply["uncertain"] == true,
            reply["performed"].as_u64().unwrap_or(0),
            reply["ask"].clone(),
        ),
        Ok(_) => return None,
        Err(error) => (
            error.code,
            error.uncertain,
            error
                .details
                .get("performed")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            error.details.get("ask").cloned().unwrap_or(Value::Null),
        ),
    };
    (code == Code::WaitingForPerson && !uncertain && performed == 0 && ask.is_object())
        .then_some(ask)
}

/// `outcome` as the agent may see it: of a question, only the account and
/// what it would do, never the message, the element or where it lands.
fn private(outcome: Result<Value>) -> Result<Value> {
    let trim = |ask: &mut Value| {
        let kept = json!({"account": ask["account"], "what": ask["what"]});
        *ask = kept;
    };
    match outcome {
        Ok(mut reply) => {
            if let Some(ask) = reply.get_mut("ask") {
                trim(ask);
            }
            Ok(reply)
        }
        Err(mut error) => {
            if let Some(ask) = error.details.get_mut("ask") {
                trim(ask);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
