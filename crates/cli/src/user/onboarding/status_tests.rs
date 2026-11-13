use super::*;

#[test]
fn permissions_without_the_jobs_answer_are_unknown_never_off() {
    assert_eq!(permissions_from(None), Value::Null);
    let hello = json!({"permissions": {"screenCapture": true, "accessibility": false}});
    assert_eq!(
        permissions_from(Some(&hello)),
        json!({"screenCapture": true, "accessibility": false})
    );
}
