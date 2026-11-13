use super::*;

#[test]
fn malformed_or_interrupted_mutation_replies_remain_uncertain() {
    for reply in [
        json!({"timedOut": true, "signal": null}),
        json!({"timedOut": false, "signal": "SIGTERM"}),
        json!({"timedOut": false, "signal": null, "exitCode": 0, "events": []}),
        json!({"timedOut": false, "signal": null, "exitCode": 0, "events": [{"event": "output", "data": "invalid!"}]}),
    ] {
        assert!(
            decode_reply(&reply, true, "request-ab12")
                .unwrap_err()
                .uncertain
        );
    }
}
