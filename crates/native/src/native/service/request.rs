//! Wire decoding preserves last-member values and deferred field refusals.
use crate::native::lease::Who;
use atlas_core::{
    actions::{Action, Expect},
    error::{Code, Error, Result},
    session::InputEpoch,
};
use serde::Deserialize;
use serde_json::Value;

/// Field refusals remain typed until the operation reaches its original guard.
#[derive(Debug)]
pub(super) enum Request {
    Hello,
    Shutdown {
        pid: u64,
    },
    Hold {
        on: Result<bool>,
    },
    Release {
        owner: String,
    },
    Renew {
        owner: String,
    },
    Acquire {
        owner: String,
        epoch: Result<InputEpoch>,
        who: Who,
    },
    Reset {
        epoch: Result<InputEpoch>,
    },
    Unknown {
        epoch: Result<InputEpoch>,
    },
    Input {
        owner: String,
        epoch: Result<InputEpoch>,
        actions: Result<Vec<Action>>,
        expect: Result<Option<Expect>>,
        app: bool,
    },
}

impl Request {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        // Materialize once: duplicate members use their last value, and nested
        // serde refusals follow sorted member order rather than source order.
        let value: Value = serde_json::from_slice(bytes)?;
        let operation = value["op"]
            .as_str()
            .ok_or_else(|| Error::new(Code::Input, "Native operation requires a string op"))?;
        let owner = || value["owner"].as_str().unwrap_or_default().to_owned();
        let epoch = || InputEpoch::deserialize(&value["epoch"]).map_err(Error::from);
        Ok(match operation {
            "hello" => Self::Hello,
            "shutdown" => Self::Shutdown {
                pid: value["pid"].as_u64().unwrap_or_default(),
            },
            "hold" => Self::Hold {
                on: value["on"]
                    .as_bool()
                    .ok_or_else(|| Error::new(Code::Input, "Hold requires a boolean on")),
            },
            "release" => Self::Release { owner: owner() },
            "renew" => Self::Renew { owner: owner() },
            "acquire" => Self::Acquire {
                owner: owner(),
                epoch: epoch(),
                who: if value["person"] == true {
                    Who::Person
                } else {
                    Who::Agent
                },
            },
            "reset" => Self::Reset { epoch: epoch() },
            "input" => Self::Input {
                owner: owner(),
                epoch: epoch(),
                actions: Vec::deserialize(&value["actions"]).map_err(Error::from),
                expect: Expect::from_request(&value["expect"]),
                app: value["app"] == true,
            },
            _ => Self::Unknown { epoch: epoch() },
        })
    }
}
