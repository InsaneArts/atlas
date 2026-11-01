//! The helper's running apps, decoded once so malformed observations never
//! read as nothing running. Output payloads retain field presence and extensions.
use super::{Client, Verified};
use crate::{
    error::{Code, Error, Result},
    process::Start,
    transport::refusal,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// A complete helper observation, independent of public app filtering.
#[derive(Clone, Debug)]
pub(crate) struct AppList {
    rows: Vec<AppRow>,
    starts: Starts,
    projection: Map<String, Value>,
}

/// Starts are optional on older helpers. Public listing discards malformed
/// starts too, while target resolution must refuse them.
#[derive(Clone, Debug)]
enum Starts {
    Absent,
    Valid(BTreeMap<i64, Start>),
    Malformed,
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
    projection: Map<String, Value>,
}

impl AppList {
    /// Decode accepted rows without tightening optional fields or starts.
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
        let starts = match projection.remove("starts") {
            None => Starts::Absent,
            Some(value) => match serde_json::from_value(value) {
                Ok(starts) => Starts::Valid(starts),
                Err(_) => Starts::Malformed,
            },
        };
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
            starts: Starts::Absent,
            projection: Map::from_iter([("ok".to_owned(), Value::Bool(true))]),
        }
    }

    pub(crate) fn rows(&self) -> &[AppRow] {
        &self.rows
    }

    pub(crate) fn starts(&self) -> Result<Option<&BTreeMap<i64, Start>>> {
        match &self.starts {
            Starts::Absent => Ok(None),
            Starts::Valid(starts) => Ok(Some(starts)),
            Starts::Malformed => Err(Error::new(
                Code::Protocol,
                "The helper's process identities were malformed",
            )),
        }
    }

    /// Public output retains every original field except internal starts.
    pub(super) fn project(self) -> Value {
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
}

impl Client {
    /// The helper's running apps, or why they could not be read. Read
    /// failures are certain; malformed rows never mean nothing is running.
    pub async fn apps(&self) -> Result<Value> {
        self.running_apps().await.map(AppList::project)
    }

    /// The complete, identity-checked observation for internal decisions.
    pub(crate) async fn running_apps(&self) -> Result<AppList> {
        let read = async {
            self.identity().await?;
            self.listed_apps(json!({"op": "apps"})).await
        };
        read.await.map_err(Error::certain)
    }

    /// The app observation in the helper session `verified` names. Older
    /// helpers cannot pin reads and are verified again instead.
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

    async fn listed_apps(&self, request: Value) -> Result<AppList> {
        AppList::decode(self.transport.call(&request).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_app_fields_retain_their_original_presence() {
        let rows = vec![
            json!({"pid": 1}),
            json!({"pid": 2, "name": null, "bundleId": null, "executable": null}),
            json!({"pid": 3, "name": "", "bundleId": "", "executable": "",
                "active": false, "hidden": true, "policy": "future",
                "launchedAt": {"future": true}, "extension": [null, 17]}),
            json!({"pid": i32::MAX, "name": "Éditeur", "bundleId": "org.editor",
                "executable": "/Applications/Editor.app/Contents/MacOS/Editor",
                "active": true, "hidden": false, "policy": "regular", "launchedAt": null}),
        ];
        let reply = json!({"ok": true, "apps": rows, "elapsedMs": 1.5,
            "extension": {"retained": true}});
        let apps = AppList::decode(reply.clone()).unwrap();
        assert_eq!(
            apps.rows().iter().map(AppRow::pid).collect::<Vec<_>>(),
            [1, 2, 3, i32::MAX as u64]
        );
        assert_eq!(apps.project(), reply);

        for field in [
            "name",
            "bundleId",
            "executable",
            "active",
            "hidden",
            "policy",
        ] {
            let values = match field {
                "active" | "hidden" => vec![json!(false), json!(true)],
                "policy" => vec![json!(""), json!("unknown")],
                _ => vec![Value::Null, json!(""), json!("text")],
            };
            for value in values {
                let mut row = json!({"pid": 1});
                row[field] = value;
                let reply = json!({"ok": true, "apps": [row]});
                assert_eq!(AppList::decode(reply.clone()).unwrap().project(), reply);
            }
        }
    }

    #[test]
    fn malformed_app_rows_never_become_an_empty_running_list() {
        for field in [
            "pid",
            "name",
            "bundleId",
            "executable",
            "active",
            "hidden",
            "policy",
        ] {
            for value in [
                Value::Null,
                json!(true),
                json!(false),
                json!(0),
                json!(-1),
                json!(1.5),
                json!("1"),
                json!([]),
                json!({}),
                json!(u64::MAX),
            ] {
                let accepted = match field {
                    "pid" => false,
                    "name" | "bundleId" | "executable" => value.is_null() || value.is_string(),
                    "active" | "hidden" => value.is_boolean(),
                    "policy" => value.is_string(),
                    _ => unreachable!(),
                };
                if accepted {
                    continue;
                }
                let mut row = json!({"pid": 812});
                row[field] = value;
                let reply = json!({"ok": true, "apps": [{"pid": 1}, row]});
                let error = AppList::decode(reply.clone()).unwrap_err();
                assert_eq!(error.code, Code::Protocol, "{reply}");
                assert!(!error.uncertain);
                assert_eq!(
                    error.error,
                    "The helper's list of running apps was malformed, so Atlas cannot tell what is running"
                );
            }
        }
        for row in [
            json!({}),
            Value::Null,
            json!(false),
            json!([]),
            json!("app"),
            json!({"pid": 2_147_483_648_u64}),
        ] {
            assert_eq!(
                AppList::decode(json!({"ok": true, "apps": [row]}))
                    .unwrap_err()
                    .code,
                Code::Protocol
            );
        }
    }

    #[test]
    fn public_apps_omit_starts_while_targets_reject_malformed_starts() {
        for starts in [
            Value::Null,
            json!(false),
            json!("legacy"),
            json!([]),
            json!({"812": null}),
            json!({"812": [123]}),
            json!({"812": [123, -1]}),
            json!({"pid": [123, 456]}),
        ] {
            let reply = json!({"ok": true, "apps": [{"pid": 812}], "starts": starts});
            let apps = AppList::decode(reply).unwrap();
            let error = apps.starts().unwrap_err();
            assert_eq!(error.code, Code::Protocol);
            assert_eq!(
                error.error,
                "The helper's process identities were malformed"
            );
            assert!(!error.uncertain);
            assert_eq!(apps.project(), json!({"ok": true, "apps": [{"pid": 812}]}));
        }
        let legacy = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}]})).unwrap();
        assert!(legacy.starts().unwrap().is_none());
        let current = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}],
            "starts": {"812": [123, 456]}}))
        .unwrap();
        assert_eq!(
            current.starts().unwrap().unwrap().get(&812),
            Some(&[123, 456])
        );
        // A decoded but invalid start stays present for snapshot identity validation.
        let stale = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}],
            "starts": {"812": [0, 1_000_000]}}))
        .unwrap();
        assert_eq!(
            stale.starts().unwrap().unwrap().get(&812),
            Some(&[0, 1_000_000])
        );
    }
}
