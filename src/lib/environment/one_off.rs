//! Requests sent to the environment worker through a request folder of
//! their own, beside the shared command, that goes once the outcome is
//! known: the helper's list of its copies, and installing an app, removing
//! one or asking how installs stand. A change runs under the control its
//! caller holds; its folder stays while its outcome is not known, as every
//! change's does.
use super::{installs::ASKED, model::*, reply::InstallState, *};
use crate::files::Token;

/// Send `request` to the helper account `helper` through a request folder
/// of its own, under `held` when it changes the helper. A change may copy
/// a large app, so it is given as long as any command; a read, a command's
/// usual time.
pub(super) async fn send(
    client: &Client,
    helper: &str,
    held: Option<&Ownership>,
    request: Request,
) -> Result<Value> {
    let id = crate::files::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    create_stage(&stage, helper)?;

    let timeout = if request.changes_helper() {
        crate::client::RUN_TIMEOUT_MAX
    } else {
        crate::client::RUN_TIMEOUT
    };
    let result = dispatch_as(client, held, helper, &stage, request, timeout).await;
    if !delivery_uncertain(&result) && !requests(&stage) {
        discard(&stage);
    }
    result
}

/// `answer`, a read's, when the helper accepted it and it holds the array
/// `field`; a refusal keeps its code, and anything else is said as
/// `malformed`, never read as an empty answer.
pub(super) fn read(answer: Value, field: &str, malformed: &str) -> Result<Value> {
    if answer["ok"] == false {
        return Err(crate::transport::refusal(&answer));
    }
    if answer["ok"] != true || !answer[field].is_array() {
        return Err(Error::new("protocol", malformed.to_owned()));
    }
    Ok(answer)
}

/// Install the bundle at `source`, which the helper can read, as
/// `~/Applications/NAME` for install `id`, under `ownership`.
pub(crate) async fn install_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
    source: &Path,
    name: &str,
) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    let request = Request::InstallApp {
        id: id.as_str().to_owned(),
        source: source.to_owned(),
        name: name.to_owned(),
    };
    send(client, &helper, Some(ownership), request).await
}

/// Remove install `id`, when it is unchanged and nothing runs from it,
/// under `ownership`.
pub(crate) async fn remove_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    let request = Request::RemoveApp {
        id: id.as_str().to_owned(),
    };
    send(client, &helper, Some(ownership), request).await
}

/// How each of `ids` stands, read without control: each asked once, in as
/// many reads as the worker's limit needs, their answers in one list. An
/// answer this build cannot read is left out, as one never given.
pub(crate) async fn installs(client: &Client, ids: &[Token]) -> Result<Vec<InstallState>> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if !unique.iter().any(|known| known == id.as_str()) {
            unique.push(id.as_str().to_owned());
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    let helper = helper_name(client.config.helper_uid)?;
    let mut listed = Vec::new();
    for batch in unique.chunks(ASKED) {
        let request = Request::Installs {
            ids: batch.to_vec(),
        };
        let answer = read(
            send(client, &helper, None, request).await?,
            "installs",
            "The helper's word on its installs was malformed",
        )?;
        listed.extend(
            answer["installs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|install| serde_json::from_value(install.clone()).ok()),
        );
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
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
            .serve(installs(&client, &twice), |request| {
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
}
