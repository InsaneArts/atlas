//! Asking the person before an agent sends from an account they listed.
//!
//! An agent's input carries the person's accounts and the grants that may
//! cover it, only while the person lists any: with none, the request is
//! byte for byte what it always was. The input service judges each action
//! before it is sent. A typing precheck sends nothing; an ordinary batch
//! may reach a sending action after a confirmed prefix already ran.
//!
//! Then the agent's command lets go of control and of the input service's
//! grant, puts the question up (or joins the one this session already
//! asks), and waits for the person, Stop or the rest of its wait, sleeping
//! on the asks folder. Once allowed, a command that sent nothing runs again:
//! control taken again, the accounts and grants read again, the target
//! looked at afresh, and the reply is the ordinary one. A confirmed prefix
//! is never replayed: its progress is returned so only the rest is sent.
use crate::client::Client;
use atlas_control::asks::{Answer, Ask, Permit, Waited};
use atlas_core::{
    actions::Action,
    error::{Code, Error, Result},
    time::now_ms,
};
use serde_json::{Value, json};
use std::time::Instant;

/// What an agent's input carries for the input service to judge it by:
/// none while the person lists no account.
pub(crate) fn policy(client: &Client) -> Option<Value> {
    policy_for(client, &[]).0
}

/// [`policy`] for a request of `actions`, with the permits the person gave
/// for any of them, taken from their store for this request alone.
pub(crate) fn policy_for(client: &Client, actions: &[Action]) -> (Option<Value>, Vec<Permit>) {
    // Tests never read the person's own accounts on the Mac running them,
    // this crate's or any crate's built with its test support.
    #[cfg(any(test, feature = "test-support"))]
    return match client.listed.clone() {
        Some(listed) => policy_by(client, listed, actions),
        None => (None, Vec::new()),
    };
    #[cfg(not(any(test, feature = "test-support")))]
    policy_by(client, crate::sign_ins::listed(), actions)
}

/// [`policy_for`] with the person's list as `listed` says it.
fn policy_by(
    client: &Client,
    listed: crate::sign_ins::Listed,
    actions: &[Action],
) -> (Option<Value>, Vec<Permit>) {
    use crate::sign_ins::Listed;
    if matches!(&listed, Listed::Accounts(accounts) if accounts.is_empty()) {
        return (None, Vec::new());
    }
    let now = now_ms();
    let session = session(client);
    let process = process(client);
    let digests: Vec<String> = actions.iter().map(Action::digest).collect();
    // Taken, they are spent whatever the request comes to; one whose
    // session ended is spent unused.
    let permits: Vec<Permit> = client
        .controller
        .asks()
        .take_permits(&session, process, &digests, now)
        .unwrap_or_default()
        .into_iter()
        .filter(|permit| session_lives(client, &permit.session, permit.process))
        .collect();
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
    grants.extend(permits.iter().map(|permit| {
        json!({
            "scope": "permit",
            "account": permit.account,
            "action": permit.action,
            "target": permit.target,
            "value": permit.value,
        })
    }));
    let mut policy = match listed {
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
    (Some(policy), permits)
}

/// Which of `parts`, text a command types, the person already allowed just
/// this once after typing stopped there: each has a live permit for that
/// very typing, which its own request then takes. Nothing is taken here.
pub(crate) fn permitted(client: &Client, parts: &[String]) -> Vec<String> {
    let now = now_ms();
    let session = session(client);
    let process = process(client);
    let permits = client.controller.asks().permits_of(&session, process, now);
    parts
        .iter()
        .filter(|part| {
            let digest = Action::Type {
                text: (*part).clone(),
            }
            .digest();
            permits.iter().any(|permit| {
                permit.action == digest && session_lives(client, &permit.session, permit.process)
            })
        })
        .cloned()
        .collect()
}

/// Give `permits` back for a later request: the one that took them was
/// proven, by `outcome`, to have sent nothing, a typing command's check
/// included, which only judges.
pub(crate) fn return_permits(client: &Client, permits: Vec<Permit>, outcome: &Result<Value>) {
    let checked = outcome
        .as_ref()
        .is_ok_and(|reply| reply["ok"] == true && reply["dry"] == true && reply["performed"] == 0);
    if !permits.is_empty() && (checked || sent_nothing(outcome)) {
        // One that can't be given back is spent: the person is asked again.
        let _ = client.controller.asks().return_permits(permits, now_ms());
    }
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
    session_lives(client, &allowed.session, allowed.process)
}

/// [`lives`], for what the person allowed `session` in host `process`.
fn session_lives(
    client: &Client,
    session: &str,
    process: Option<atlas_core::process::Instance>,
) -> bool {
    let record = atlas_core::host::Key::parse(session).and_then(|key| {
        let output = client.output_dir().ok()?;
        crate::host_session::store::Store::at(output).read(&key)
    });
    process.is_none_or(|process| process.running())
        && record.is_none_or(|record| {
            record.ended_at().is_none()
                && record
                    .host_process
                    .is_none_or(|recorded| Some(recorded) == process)
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
    drop(client.take_for_ask());
    outcome
}

/// [`after_asking`]. What the person allowed once is kept across the
/// attempts that sent nothing, so several places in one request can each
/// be allowed; the command's end spends it.
async fn asking(client: &Client, mut attempt: impl AsyncFnMut() -> Result<Value>) -> Result<Value> {
    let until = Instant::now() + client.controller.remaining();
    loop {
        let outcome = attempt().await;
        // Only a clean stop becomes a question: one that sent nothing, or
        // that stopped before an action that sends, which sent none of it.
        let Some((question, later)) = asked(&outcome) else {
            return private(looked_at(outcome));
        };
        // The person's question shows the screen: the picture before a
        // request that sent nothing, or one taken now that nothing is held.
        if !client.kept_for_ask()
            && let Some((raw, admitted)) = client.picture_for_ask().await
        {
            client.keep_for_ask(raw, admitted);
        }
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
            picture: None,
        };
        let frame = ask.question["frame"].clone();
        let id = asks
            .ask(ask, now_ms())
            .map_err(|error| with_progress(&outcome, error))?;
        // A question goes up without its picture when it can't be kept, or
        // when the person began or ended a sign-in since it was taken.
        if let Some((raw, admitted)) = client.take_for_ask()
            && client.controller.still_visible(admitted).is_ok()
        {
            let _ = keep_picture(&asks, &id, &raw, &frame);
        }
        let _shown = Shown {
            asks: asks.clone(),
            id: id.clone(),
        };
        #[cfg(test)]
        if let Some(observe) = &client.ask_shown {
            observe(&id);
        }
        let waited = tokio::task::spawn_blocking({
            let asks = asks.clone();
            let id = id.clone();
            move || asks.wait(&id, until)
        })
        .await
        .map_err(|cause| {
            Error::new(Code::State, "The wait for the person ended").with_source(cause)
        })
        .and_then(|waited| waited)
        .map_err(|error| with_progress(&outcome, error))?;
        // What the person saw is what they answered: the grant comes from
        // the stored question, never from what a rerun read. Just This Once
        // goes to the one command that claims it first; another joined to
        // the same question asks again.
        let stored = match waited {
            Waited::Answered(Answer::Once) => match asks
                .claim_once(&id, now_ms())
                .map_err(|error| with_progress(&outcome, error))?
            {
                Some(stored) => stored,
                None if later => return private(went(outcome, &Went::Asked)),
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
            Waited::Unanswered if later => return private(went(outcome, &Went::Asked)),
            Waited::Unanswered => return private(outcome),
            Waited::Stopped => return Err(with_progress(&outcome, Error::stopped())),
            Waited::Answered(Answer::Deny) => {
                asks.close(&id);
                if later {
                    return private(went(outcome, &Went::Denied(stored.account)));
                }
                return Err(Error::new(
                    Code::Denied,
                    format!(
                        "The person said no to sending this in {}. Nothing was sent.",
                        stored.account
                    ),
                ));
            }
            // What ran isn't sent again: the agent sends only the rest,
            // which meets what the person allowed.
            Waited::Answered(Answer::Task) => {
                asks.close(&id);
                if later {
                    return private(went(outcome, &Went::Allowed));
                }
            }
            Waited::Answered(Answer::Once) if later => {
                asks.permit(
                    Permit {
                        session: stored.session,
                        process: stored.process,
                        account: question["accountKey"].clone(),
                        action: question["action"].as_str().unwrap_or_default().to_owned(),
                        target: question["bound"].clone(),
                        value: question["message"]["digest"].as_str().map(str::to_owned),
                        ends: u64::MAX,
                    },
                    now_ms(),
                )
                .map_err(|error| with_progress(&outcome, error))?;
                return private(went(outcome, &Went::Allowed));
            }
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

/// Preserve confirmed delivery facts when the approval infrastructure or
/// Stop replaces a partial refusal. Only reviewed progress fields cross
/// this boundary; the private question and message never do.
fn with_progress(outcome: &Result<Value>, mut error: Error) -> Error {
    let field = |key: &str| match outcome {
        Ok(reply) => reply.get(key),
        Err(previous) => previous.details.get(key),
    };
    let performed = field("performed").and_then(Value::as_u64);
    let typed = field("typed").and_then(Value::as_u64);
    let partial = field("partial").and_then(Value::as_bool);
    if performed.unwrap_or(0) == 0 && typed.unwrap_or(0) == 0 && partial != Some(true) {
        return error;
    }
    if let Some(performed) = performed {
        error = error.with("performed", json!(performed));
    }
    if let Some(typed) = typed {
        error = error.with("typed", json!(typed));
    }
    if let Some(partial) = partial {
        error = error.with("partial", json!(partial));
    }
    let uncertain = match outcome {
        Ok(reply) => reply["uncertain"] == true,
        Err(previous) => previous.uncertain,
    };
    error.uncertain |= uncertain;
    error.with("next", json!("Some input already ran. Do not repeat it. Resolve this refusal, then look at the result and send only the remaining input."))
}

/// How the question about the action that stopped a request partway went.
enum Went {
    /// It is still up.
    Asked,
    /// The person allowed it.
    Allowed,
    /// The person said no, in this account.
    Denied(String),
}

/// `outcome`, a request stopped before an action that sends after some of
/// it ran, with what the agent does next: once allowed, send only the
/// rest. It is never sent again whole.
fn went(outcome: Result<Value>, how: &Went) -> Result<Value> {
    let next = |field: &dyn Fn(&str) -> Option<u64>| {
        let (ran, rest) = match field("typed") {
            Some(typed) => (
                format!("{typed} characters went in"),
                format!(
                    "type only the rest of the text, from character {}",
                    typed + 1
                ),
            ),
            None => {
                let performed = field("performed").unwrap_or(0);
                (
                    format!("The first {performed} actions ran"),
                    format!("send only the actions after the first {performed}"),
                )
            }
        };
        match how {
            Went::Asked => format!(
                "{ran} before one that sends, which Atlas asked the person about. Do other work, then {rest}."
            ),
            Went::Allowed => format!("{ran}, and the person allowed the next one. Now {rest}."),
            Went::Denied(_) => format!(
                "{ran}, and the person said no to the next one. Don't try another way: ask them what they want."
            ),
        }
    };
    let denied = match how {
        Went::Denied(account) => Some(format!(
            "The person said no to sending this in {account}. The rest wasn't sent."
        )),
        Went::Asked | Went::Allowed => None,
    };
    match outcome {
        Ok(mut reply) => {
            reply["next"] = json!(next(&|key| reply[key].as_u64()));
            if let Some(said) = denied {
                reply["code"] = json!(Code::Denied);
                reply["error"] = json!(said);
            }
            Ok(reply)
        }
        Err(mut error) => {
            let said = next(&|key| error.details.get(key).and_then(Value::as_u64));
            error = error.with("next", json!(said));
            if let Some(said) = denied {
                error.code = Code::Denied;
                error.error = said;
            }
            Err(error)
        }
    }
}

/// `outcome`, with the step for a request stopped inside an action that
/// had sent part of itself: some of it went in, and only a look at the
/// field tells how much.
fn looked_at(outcome: Result<Value>) -> Result<Value> {
    const LOOK: &str = "Part of the last action went in before Atlas had to ask the person. Look at the field before doing anything else.";
    let stopped = |code| matches!(code, Code::WaitingForPerson | Code::Denied);
    match outcome {
        Ok(mut reply)
            if reply["uncertain"] == true && stopped(Code::of_reply(&reply, Code::Native)) =>
        {
            reply["next"] = json!(LOOK);
            Ok(reply)
        }
        Err(error) if error.uncertain && stopped(error.code) => {
            Err(error.with("next", json!(LOOK)))
        }
        outcome => outcome,
    }
}

/// The message preview of the ask a command waits on, taken off it when
/// the wait ends however it ends: answered, out of time, stopped or
/// interrupted. Only the person reads it, and only while it is waited on.
struct Shown {
    asks: atlas_control::asks::Asks,
    id: String,
}

impl Drop for Shown {
    fn drop(&mut self) {
        // One that can't be taken off goes with its ask at two minutes.
        let _ = self.asks.forget_preview(&self.id);
    }
}

/// What a request's refused `outcome` says it came to.
struct Came {
    code: Code,
    uncertain: bool,
    /// Actions it performed in full.
    performed: u64,
    /// Whether part of it was sent.
    partial: bool,
    /// Characters a typing command typed.
    typed: u64,
    ask: Value,
}

/// [`Came`], for an `outcome` that isn't a success, whether it came back as
/// an error or as a reply saying so.
fn came(outcome: &Result<Value>) -> Option<Came> {
    let reply = match outcome {
        Ok(reply) if reply["ok"] == false => reply.clone(),
        Ok(_) => return None,
        Err(error) => serde_json::to_value(error).ok()?,
    };
    Some(Came {
        code: Code::of_reply(&reply, Code::Native),
        uncertain: reply["uncertain"] == true,
        performed: reply["performed"].as_u64().unwrap_or(0),
        partial: reply["partial"] == true,
        typed: reply["typed"].as_u64().unwrap_or(0),
        ask: reply["ask"].clone(),
    })
}

/// The question in `outcome`, when it is a clean stop to ask the person,
/// and whether some of the request ran before it. Clean: the action it
/// stopped at sent nothing and letting go of what was held went well, so
/// the outcome is certain.
fn asked(outcome: &Result<Value>) -> Option<(Value, bool)> {
    let came = came(outcome)?;
    (came.code == Code::WaitingForPerson && !came.uncertain && came.ask.is_object()).then(|| {
        let later = came.performed > 0 || came.typed > 0 || came.partial;
        (came.ask, later)
    })
}

/// Whether `outcome` is a clean stop the person is asked about.
pub(crate) fn asks_person(outcome: &Result<Value>) -> bool {
    asked(outcome).is_some()
}

/// Keep `raw` as ask `id`'s picture, encoded as the action pictures are.
/// `frame` is where its element is in it: the two go up together.
fn keep_picture(
    asks: &atlas_control::asks::Asks,
    id: &str,
    raw: &crate::before::Raw,
    frame: &Value,
) -> Result<()> {
    let (width, height) = (raw.width as usize, raw.height as usize);
    let (jpeg, shown_width, shown_height) = crate::helper_info::jpeg_rgb(
        &raw.rgb,
        (width, height, width.saturating_mul(3)),
        (crate::before::SIDE, crate::before::QUALITY),
    )?;
    let size = (shown_width, shown_height);
    asks.keep_picture(id, &jpeg, &in_picture(frame, raw.display, size), size)
}

/// `frame`, in the points of a display of `display` size, in the pixels of
/// its picture of `picture` size, as the person's app draws it there.
fn in_picture(frame: &Value, display: (f64, f64), picture: (u32, u32)) -> Value {
    let (Some(x), Some(y), Some(width), Some(height)) = (
        frame["x"].as_f64(),
        frame["y"].as_f64(),
        frame["width"].as_f64(),
        frame["height"].as_f64(),
    ) else {
        return Value::Null;
    };
    if display.0 <= 0.0 || display.1 <= 0.0 {
        return Value::Null;
    }
    let across = f64::from(picture.0) / display.0;
    let down = f64::from(picture.1) / display.1;
    json!({"x": x * across, "y": y * down, "width": width * across, "height": height * down})
}

/// Whether the agent session that asked `ask` still waits on it: its host
/// process runs and its session hasn't ended.
pub fn asker_lives(client: &Client, ask: &Ask) -> bool {
    session_lives(client, &ask.session, ask.process)
}

/// Whether `outcome` proves its request sent nothing: refused before its
/// first event, to ask the person, for a Look Only account, or because
/// something else had focus or where it lands couldn't be told.
pub(crate) fn sent_nothing(outcome: &Result<Value>) -> bool {
    came(outcome).is_some_and(|came| {
        matches!(
            came.code,
            Code::WaitingForPerson | Code::Denied | Code::FocusChanged
        ) && !came.uncertain
            && came.performed == 0
            && !came.partial
            && came.typed == 0
    })
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
