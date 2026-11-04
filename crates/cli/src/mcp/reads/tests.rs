use super::*;

#[test]
fn a_screenshot_call_becomes_the_command_it_takes() {
    let argv = |arguments: Value| screenshot_argv(&arguments).map_err(|error| error.error);

    assert_eq!(argv(Value::Null), Ok(vec!["screenshot".to_owned()]));
    assert_eq!(argv(json!({})), Ok(vec!["screenshot".to_owned()]));
    assert_eq!(
        argv(json!({"app": "TextEdit", "window": 31, "_atlasView": "token"})),
        Ok(vec![
            "screenshot".to_owned(),
            "--app=TextEdit".to_owned(),
            "--window=31".to_owned()
        ])
    );
    assert_eq!(
        argv(json!({"app": "-n", "region": "-10,0,200,100"})),
        Ok(vec![
            "screenshot".to_owned(),
            "--app=-n".to_owned(),
            "--region=-10,0,200,100".to_owned()
        ]),
        "a value that looks like a flag stays a value"
    );
}

#[test]
fn a_screenshot_call_with_anything_else_is_refused() {
    for arguments in [
        json!("TextEdit"),
        json!({"app": 7}),
        json!({"window": "31"}),
        json!({"window": 1.5}),
        json!({"region": null}),
        json!({"full": true}),
    ] {
        let error = screenshot_argv(&arguments).unwrap_err();
        assert_eq!(error.code, Code::BadRequest, "{arguments}");
    }
}
