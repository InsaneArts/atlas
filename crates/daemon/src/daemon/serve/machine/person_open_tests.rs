use super::*;

fn asked(app: &str, url: &str, lease: &Token) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"op": "person-open", "app": app, "url": url, "lease": lease,
        "person": true, "change": Token::new()}),
    )
    .unwrap()
}

#[test]
fn a_web_page_opens_in_a_window_of_its_own_in_chrome_and_where_firefox_puts_it() {
    let lease = Token::new();
    let chrome = PersonOpen::of(&asked("Google Chrome", "https://github.com", &lease)).unwrap();
    let settings = PersonOpen::of(&asked(
        "Google Chrome",
        "chrome://settings/clearBrowserData",
        &lease,
    ))
    .unwrap();
    let firefox = PersonOpen::of(&asked("Firefox", "https://github.com", &lease)).unwrap();

    assert_eq!(
        chrome.argv(),
        [
            OPEN,
            "-n",
            "-a",
            "Google Chrome",
            "--args",
            "--new-window",
            "https://github.com"
        ]
    );
    assert_eq!(
        settings.argv(),
        [
            OPEN,
            "-a",
            "Google Chrome",
            "--",
            "chrome://settings/clearBrowserData"
        ]
    );
    assert_eq!(
        firefox.argv(),
        [OPEN, "-a", "Firefox", "--", "https://github.com"]
    );
    assert_eq!(chrome.lease, lease);
}

#[test]
fn only_a_site_or_the_browsers_own_settings_opens_and_only_in_chrome_or_firefox() {
    let lease = Token::new();
    let long = format!("https://github.com/{}", "a".repeat(LONGEST));

    for (app, url) in [
        ("Safari", "https://github.com"),
        ("Terminal", "https://github.com"),
        ("Google Chrome", "http://github.com"),
        ("Google Chrome", "file:///etc/passwd"),
        ("Google Chrome", "about:preferences#privacy"),
        ("Firefox", "chrome://settings/"),
        ("Google Chrome", "https://github.com /etc"),
        ("Google Chrome", "https://github.com\n--args"),
        ("Google Chrome", long.as_str()),
    ] {
        let refused = PersonOpen::of(&asked(app, url, &lease)).unwrap_err();
        assert_eq!(refused["ok"], false, "{app} {url}");
        assert_ne!(refused["uncertain"], true, "{app} {url}");
    }
    assert!(PersonOpen::of(&asked("Firefox", "about:preferences#privacy", &lease)).is_ok());
}

#[test]
fn a_request_without_the_persons_mark_or_an_owner_is_refused() {
    let lease = Token::new();
    let unmarked = serde_json::to_vec(&json!({"op": "person-open", "app": "Firefox",
        "url": "https://github.com", "lease": lease}))
    .unwrap();
    let ownerless = serde_json::to_vec(&json!({"op": "person-open", "app": "Firefox",
        "url": "https://github.com", "person": true}))
    .unwrap();

    for line in [unmarked, ownerless] {
        let refused = PersonOpen::of(&line).unwrap_err();
        assert_eq!(refused["code"], "bad-request", "{refused}");
    }
    assert!(PersonOpen::asked(&asked(
        "Firefox",
        "https://github.com",
        &lease
    )));
    assert!(!PersonOpen::asked(br#"{"op": "acquire", "person": true}"#));
}
