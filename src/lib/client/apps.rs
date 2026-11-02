//! The helper's running apps, decoded once so malformed observations never
//! read as nothing running. Output payloads retain field presence and extensions.
use super::{Client, Verified};
use crate::{
    error::{Code, Error, Result},
    process::Start,
    protocol::SessionPin,
    transport::refusal,
};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, sync::Arc};

/// A complete helper observation, independent of public app filtering.
#[derive(Clone, Debug)]
pub(crate) struct AppList {
    rows: Vec<AppRow>,
    /// The helper's process identities. Public listing discards a malformed
    /// map, while target resolution must refuse it.
    starts: std::result::Result<BTreeMap<i64, Start>, Arc<serde_json::Error>>,
    projection: Map<String, Value>,
}

/// Validated decision fields and the original row for output only.
#[derive(Clone, Debug)]
pub(crate) struct AppRow {
    pid: u64,
    name: Option<String>,
    bundle_id: Option<String>,
    executable: Option<String>,
    active: Option<bool>,
    policy: Option<String>,
    launched_at: Option<u64>,
    projection: Map<String, Value>,
}

impl AppList {
    /// Decode accepted rows without tightening optional fields.
    /// A helper refusal keeps its code; other shapes are protocol failures.
    pub(crate) fn decode(reply: Value) -> Result<Self> {
        if reply["ok"] == false {
            return Err(refusal(&reply));
        }
        let malformed = || {
            Error::new(
                Code::Protocol,
                "The helper's list of running apps was malformed, so Atlas cannot tell what is running",
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
        })
    }

    /// A known empty observation for paths that require no helper lookup.
    pub(crate) fn empty() -> Self {
        Self {
            rows: Vec::new(),
            starts: Ok(BTreeMap::new()),
            projection: Map::from_iter([("ok".to_owned(), Value::Bool(true))]),
        }
    }

    pub(crate) fn rows(&self) -> &[AppRow] {
        &self.rows
    }

    pub(crate) fn starts(&self) -> Result<&BTreeMap<i64, Start>> {
        self.starts.as_ref().map_err(|cause| {
            Error::new(
                Code::Protocol,
                "The helper's process identities were malformed",
            )
            .with_source(cause.clone())
        })
    }

    /// Public output retains every original field except internal starts.
    pub(crate) fn project(self) -> Value {
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
        let text = |key: &str| {
            fields
                .get(key)
                .is_none_or(|value| value.is_null() || value.is_string())
        };
        let flag = |key: &str| fields.get(key).is_none_or(Value::is_boolean);
        if !["name", "bundleId", "executable"].into_iter().all(text)
            || !["active", "hidden"].into_iter().all(flag)
            || !fields.get("policy").is_none_or(Value::is_string)
        {
            return None;
        }
        let text = |key: &str| fields.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            pid,
            name: text("name"),
            bundle_id: text("bundleId"),
            executable: text("executable"),
            active: fields.get("active").and_then(Value::as_bool),
            policy: text("policy"),
            launched_at: fields.get("launchedAt").and_then(Value::as_u64),
            projection: fields,
        })
    }

    pub(crate) fn pid(&self) -> u64 {
        self.pid
    }
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub(crate) fn bundle_id(&self) -> Option<&str> {
        self.bundle_id.as_deref()
    }
    pub(crate) fn executable(&self) -> Option<&str> {
        self.executable.as_deref()
    }
    pub(crate) fn active(&self) -> bool {
        self.active == Some(true)
    }
    pub(crate) fn policy(&self) -> Option<&str> {
        self.policy.as_deref()
    }
    /// Its launch, in milliseconds since 1970, when the helper says.
    pub(crate) fn launched_at(&self) -> Option<u64> {
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
        self.running_apps().await.map(AppList::project)
    }

    /// The complete, identity-checked observation for internal decisions.
    pub(crate) async fn running_apps(&self) -> Result<AppList> {
        self.read_apps(None).await.map_err(Error::certain)
    }

    /// The app observation in the helper session `verified` names.
    pub(crate) async fn apps_in(&self, verified: &Verified) -> Result<AppList> {
        self.read_apps(Some(verified.pin()?)).await
    }

    /// One app list, in the session `pin` names, or after verifying the helper
    /// when there is no pin. Only the read itself is made certain here.
    async fn read_apps(&self, pin: Option<SessionPin>) -> Result<AppList> {
        let mut request = json!({"op": "apps"});
        if let Some(pin) = pin {
            request["session"] = json!(pin);
        } else {
            self.identity().await?;
        }
        let read = async { AppList::decode(self.transport.call(&request).await?) };
        read.await.map_err(Error::certain)
    }
}

#[cfg(test)]
mod tests;
