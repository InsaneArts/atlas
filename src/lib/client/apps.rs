//! The helper's running apps, read from its daemon and checked for shape,
//! so a malformed reply never reads as nothing running.
use super::{Client, Verified};
use crate::{
    error::{Code, Error, Result},
    transport::refusal,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct AppList {
    pub apps: Vec<App>,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct App {
    pub pid: i32,
    #[serde(flatten)]
    pub fields: Value,
}

/// `reply` as an app list: accepted, with `apps` an array of objects, each
/// with a process ID from 1 to 2³¹−1, `name`, `bundleId` and `executable`
/// strings or null, `active` and `hidden` booleans and `policy` a string
/// when present. A refusal keeps its code; anything else is a protocol
/// failure, never an empty list.
pub(crate) fn app_list(reply: Value) -> Result<AppList> {
    if reply["ok"] == false {
        return Err(refusal(&reply));
    }
    let valid = reply["ok"] == true
        && reply["apps"]
            .as_array()
            .is_some_and(|apps| apps.iter().all(listed_app));
    if valid {
        serde_json::from_value(reply).map_err(|_| malformed())
    } else {
        Err(malformed())
    }
}

fn malformed() -> Error {
    Error::new(
        Code::Protocol,
        "The helper's list of running apps was malformed, so Atlas cannot tell what is running",
    )
}

/// Whether `app` is one entry of an app list, as [`app_list`] says.
fn listed_app(app: &Value) -> bool {
    let Some(fields) = app.as_object() else {
        return false;
    };
    let pid = app["pid"]
        .as_u64()
        .is_some_and(|pid| pid > 0 && i32::try_from(pid).is_ok());
    let text = |key: &str| {
        fields
            .get(key)
            .is_none_or(|value| value.is_null() || value.is_string())
    };
    let flag = |key: &str| fields.get(key).is_none_or(Value::is_boolean);

    pid && ["name", "bundleId", "executable"].into_iter().all(text)
        && ["active", "hidden"].into_iter().all(flag)
        && fields.get("policy").is_none_or(Value::is_string)
}

impl Client {
    /// The helper's running apps, or why they could not be read. Only a
    /// reply the helper accepted, in the shape [`app_list`] checks, counts:
    /// a refusal keeps its code (busy, apps), and a reply of any other shape
    /// is never read as "nothing is running". Reading changes nothing in the
    /// helper, so a failure here is never uncertain.
    pub async fn apps(&self) -> Result<Value> {
        let read = async {
            self.identity().await?;
            self.listed_apps(json!({"op": "apps"})).await
        };
        let mut reply = read.await.map_err(Error::certain)?;
        reply.fields.remove("starts");
        Ok(json!(reply))
    }

    /// [`Client::apps`] in the helper session `verified` names, as a pinned
    /// [`Run`](super::Run) runs: a daemon that can be told the session
    /// refuses the listing in any other, and an older one is verified again.
    pub(crate) async fn apps_in(&self, verified: &Verified) -> Result<AppList> {
        let Some(pin) = verified.pin() else {
            self.identity().await?;
            return self
                .listed_apps(json!({"op": "apps"}))
                .await
                .map_err(Error::certain);
        };

        self.listed_apps(json!({"op": "apps", "session": pin}))
            .await
            .map_err(Error::certain)
    }

    /// The app list `request` asks the helper daemon for, once the helper
    /// is verified.
    async fn listed_apps(&self, request: Value) -> Result<AppList> {
        app_list(self.transport.call(&request).await?)
    }
}
