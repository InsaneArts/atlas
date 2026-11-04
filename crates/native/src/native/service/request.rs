//! Wire decoding preserves last-member values and deferred field refusals.
use crate::native::lease::Who;
use atlas_core::{
    actions::{Action, Expect},
    error::{Code, Error, Result},
    session::InputEpoch,
};
use serde::Deserialize;
use serde_json::Value;

fn owner(value: &Value) -> String {
    value["owner"].as_str().unwrap_or_default().to_owned()
}

fn epoch(value: &Value) -> Result<InputEpoch> {
    InputEpoch::deserialize(&value["epoch"]).map_err(Error::from)
}

/// Each operation's shape, wire word and decoder are one row, so a declared
/// operation cannot go undecoded. Any other word is `Unknown`.
macro_rules! requests {
    ($value:ident => $($word:literal $variant:ident $({$($field:tt)*})? = $decode:expr),+ $(,)?) => {
        /// Field refusals remain typed until the operation reaches its original guard.
        #[derive(Debug)]
        pub(super) enum Request {
            $($variant $({$($field)*})?,)+
            Unknown { epoch: Result<InputEpoch> },
        }

        impl Request {
            fn named(word: &str, $value: &Value) -> Self {
                match word {
                    $($word => $decode,)+
                    _ => Self::Unknown { epoch: epoch($value) },
                }
            }
        }
    };
}

requests! { value =>
    "hello" Hello = Self::Hello,
    "shutdown" Shutdown { pid: u64 } = Self::Shutdown {
        pid: value["pid"].as_u64().unwrap_or_default(),
    },
    "hold" Hold { on: Result<bool> } = Self::Hold {
        on: value["on"]
            .as_bool()
            .ok_or_else(|| Error::new(Code::Input, "Hold requires a boolean on")),
    },
    "release" Release { owner: String } = Self::Release { owner: owner(value) },
    "renew" Renew { owner: String } = Self::Renew { owner: owner(value) },
    "acquire" Acquire {
        owner: String,
        epoch: Result<InputEpoch>,
        who: Who,
    } = Self::Acquire {
        owner: owner(value),
        epoch: epoch(value),
        who: if value["person"] == true {
            Who::Person
        } else {
            Who::Agent
        },
    },
    "reset" Reset { epoch: Result<InputEpoch> } = Self::Reset { epoch: epoch(value) },
    "input" Input {
        owner: String,
        epoch: Result<InputEpoch>,
        actions: Result<Vec<Action>>,
        expect: Result<Option<Expect>>,
        app: bool,
    } = Self::Input {
        owner: owner(value),
        epoch: epoch(value),
        actions: Vec::deserialize(&value["actions"]).map_err(Error::from),
        expect: Expect::from_request(&value["expect"]),
        app: value["app"] == true,
    },
    "elements" Elements {
        epoch: Result<InputEpoch>,
        expect: Result<Option<Expect>>,
    } = Self::Elements {
        epoch: epoch(value),
        expect: Expect::from_request(&value["expect"]),
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
        Ok(Self::named(operation, &value))
    }
}
