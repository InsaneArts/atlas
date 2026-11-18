//! The helper's running apps, decoded once so malformed observations never
//! read as nothing running. Output payloads retain field presence and extensions.
use super::{Client, Verified};
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Result},
    process::Start,
    protocol::SessionPin,
    transport::refusal,
};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::Arc};

/// A complete helper observation, independent of public app filtering.
#[derive(Clone, Debug)]
pub struct AppList {
    rows: Vec<AppRow>,
    /// The helper's process identities. Public listing discards a malformed
    /// map, while target resolution must refuse it.
    starts: std::result::Result<BTreeMap<i64, Start>, Arc<serde_json::Error>>,
    projection: Map<String, Value>,
    /// Whose apps they are, as its refusals say it: "The helper's", or a
    /// VM's by its name.
    whose: String,
}

/// Validated decision fields and the original row for output only.
#[derive(Clone, Debug)]
pub struct AppRow {
    pid: u64,
    name: Option<String>,
    bundle_id: Option<String>,
    executable: Option<String>,
    active: bool,
    policy: Option<String>,
    launched_at: Option<u64>,
    projection: Map<String, Value>,
}

impl AppList {
    /// Decode accepted rows without tightening optional fields.
    /// A helper refusal keeps its code; other shapes are protocol failures.
    ///
    /// # Errors
    ///
    /// Returns the helper's refusal, or an error for rows that are malformed.
    pub fn decode(reply: Value) -> Result<Self> {
        Self::decode_of(reply, "The helper's")
    }

    /// [`AppList::decode`] of the apps of the desktop `whose` names, as its
    /// refusals say it.
    pub(crate) fn decode_of(reply: Value, whose: &str) -> Result<Self> {
        if reply["ok"] == false {
            return Err(refusal(&reply));
        }
        let malformed = || {
            Error::new(
                Code::Protocol,
                format!(
                    "{whose} list of running apps was malformed, so Atlas cannot tell what is \
                     running"
                ),
            )
        };
        if reply["ok"] != true {
            return Err(malformed());
        }
        let Value::Object(mut projection) = reply else {
            return Err(malformed());
        };
        let Some(Value::Array(apps)) = projection.remove("apps") else {
            return Err(malformed());
        };
        let rows = apps
            .into_iter()
            .map(AppRow::decode)
            .collect::<Option<_>>()
            .ok_or_else(malformed)?;
        let starts = serde_json::from_value(projection.remove("starts").unwrap_or_default())
            .map_err(Arc::new);
        Ok(Self {
            rows,
            starts,
            projection,
            whose: whose.to_owned(),
        })
    }

    /// A known empty observation for paths that require no helper lookup.
    pub(crate) fn empty() -> Self {
        Self {
            rows: Vec::new(),
            starts: Ok(BTreeMap::new()),
            projection: Map::from_iter([("ok".to_owned(), Value::Bool(true))]),
            whose: "The helper's".to_owned(),
        }
    }

    pub fn rows(&self) -> &[AppRow] {
        &self.rows
    }

    pub(crate) fn starts(&self) -> Result<&BTreeMap<i64, Start>> {
        self.starts.as_ref().map_err(|cause| {
            Error::new(
                Code::Protocol,
                format!("{} process identities were malformed", self.whose),
            )
            .with_source(cause.clone())
        })
    }

    /// Public output retains every original field except internal starts.
    pub fn project(self) -> Value {
        let mut projection = self.projection;
        projection.insert(
            "apps".to_owned(),
            Value::Array(
                self.rows
                    .into_iter()
                    .map(|row| Value::Object(row.projection))
                    .collect(),
            ),
        );
        Value::Object(projection)
    }
}

impl AppRow {
    fn decode(app: Value) -> Option<Self> {
        let Value::Object(fields) = app else {
            return None;
        };
        let pid = fields
            .get("pid")?
            .as_u64()
            .filter(|pid| *pid > 0 && i32::try_from(*pid).is_ok())?;
        // Absent and null text both read as none; another type refuses the row.
        let text = |key: &str| match fields.get(key) {
            None | Some(Value::Null) => Some(None),
            Some(value) => value.as_str().map(|text| Some(text.to_owned())),
        };
        let flag = |key: &str| fields.get(key).map_or(Some(false), Value::as_bool);
        // A row whose hidden flag is not one is malformed, though nothing
        // reads it.
        if fields.get("policy").is_some_and(Value::is_null) || flag("hidden").is_none() {
            return None;
        }
        Some(Self {
            pid,
            name: text("name")?,
            bundle_id: text("bundleId")?,
            executable: text("executable")?,
            active: flag("active")?,
            policy: text("policy")?,
            launched_at: fields.get("launchedAt").and_then(Value::as_u64),
            projection: fields,
        })
    }

    pub fn pid(&self) -> u64 {
        self.pid
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub(crate) fn bundle_id(&self) -> Option<&str> {
        self.bundle_id.as_deref()
    }
    pub fn executable(&self) -> Option<&str> {
        self.executable.as_deref()
    }
    pub(crate) fn active(&self) -> bool {
        self.active
    }
    pub(crate) fn policy(&self) -> Option<&str> {
        self.policy.as_deref()
    }
    /// Its launch, in milliseconds since 1970, when the helper says.
    pub fn launched_at(&self) -> Option<u64> {
        self.launched_at
    }
}

impl Client {
    /// The helper's running apps, or why they could not be read. Read
    /// failures are certain; malformed rows never mean nothing is running.
    ///
    /// # Errors
    ///
    /// Returns helper identity, transport or refusal errors. Malformed app rows are a protocol
    /// failure, never evidence that no applications are running.
    pub async fn apps(&self) -> Result<Value> {
        // On a VM only `open`, `close` and a cleanup read its apps for now.
        self.second_login_only()?;
        self.running_apps().await.map(AppList::project)
    }

    /// The complete, identity-checked observation for internal decisions.
    ///
    /// # Errors
    ///
    /// Returns the failure to reach or verify the helper, or a malformed list.
    pub async fn running_apps(&self) -> Result<AppList> {
        self.read_apps(None).await.map_err(Error::certain)
    }

    /// The app observation in the helper session `verified` names, and a
    /// VM's in the generation its proof was taken in.
    pub(crate) async fn apps_in(&self, verified: &Verified) -> Result<AppList> {
        let generation = verified.machine().map(|machine| machine.generation);
        self.read_apps(Some((verified.pin()?, generation))).await
    }

    /// One app list, in the session `pinned` names and a VM's generation it
    /// names, or after verifying the helper when there is none. Only the
    /// read itself is made certain here.
    async fn read_apps(&self, pinned: Option<(SessionPin, Option<u64>)>) -> Result<AppList> {
        let mut request = json!({"op": "apps"});
        // A VM's guest is proved afresh, and read in the session and the
        // generation its proof pinned.
        let pinned = match pinned {
            None if matches!(self.desktop().kind(), Kind::Vm(_)) => {
                let verified = self.verify_vm().await?;
                let generation = verified.machine().map(|machine| machine.generation);
                Some((verified.pin()?, generation))
            }
            pinned => pinned,
        };
        let generation = match pinned {
            Some((pin, generation)) => {
                request["session"] = json!(pin);
                generation
            }
            None => {
                self.identity().await?;
                None
            }
        };
        // A VM's refusals name the VM.
        let whose = self.owner();
        let read =
            async { AppList::decode_of(self.link.call_in(&request, generation).await?, &whose) };
        read.await.map_err(Error::certain)
    }
}

#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;
