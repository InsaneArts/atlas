use super::*;
use crate::environment::RESERVED_VARIABLES;

fn profile(value: Value) -> Profile {
    serde_json::from_value(value).unwrap()
}

#[test]
fn profile_cannot_shadow_helper_identity_or_generated_state() {
    for name in RESERVED_VARIABLES {
        assert!(validate(&profile(json!({"version":2,"environment":[name]}))).is_err());
    }

    assert!(
        validate(&profile(
            json!({"version":2,"files":[{"source":"x","target":ENVIRONMENT_TARGET}]})
        ))
        .is_err()
    );
    assert!(validate(&profile(json!({"version":2,"environment":["KEY","KEY"]}))).is_err());
    assert!(validate(&profile(json!({"version":2,"environment":["KEY=value"]}))).is_err());
    assert!(
        validate(&profile(
            json!({"version":2,"environment":["OLLAMA_MODELS"]})
        ))
        .is_ok()
    );
}
