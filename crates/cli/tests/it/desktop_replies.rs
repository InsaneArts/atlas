//! Replies naming their desktop: with more than one desktop listed, every
//! reply and failure of a command that chose one names it; with one listed,
//! every byte is as it was.
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const VM: &str = "\n[[desktops]]\nname = \"work\"\nkind = \"vm\"\n\
                  id = \"0123456789abcdef01234567\"\n";
const USER: &str = "\n[[desktops]]\nname = \"main\"\nkind = \"user\"\nhelper_uid = 60001\n\
                    socket_path = \"absent.sock\"\nidentity_probe = \"absent\"\n\
                    input_socket = \"absent-input.sock\"\n";

/// Settings `text` written as `name` in `directory`.
fn settings(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// `atlas <args>` with the settings at `path`, its home and bindings in
/// `home`, given `input` on its standard input.
fn atlas(path: &Path, home: &Path, args: &[&str], input: &str) -> Output {
    let mut child = atlas_core::spawn::spawn(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", home)
            .env(atlas_client::host_session::desktops::TEST_HOME, home)
            .env("ATLAS_JSON", "1")
            .arg("--config")
            .arg(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The one line a command printed: its result, or else its failure.
fn printed(output: &Output) -> String {
    let said = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    String::from_utf8(said.clone())
        .unwrap()
        .trim_end()
        .to_owned()
}

/// `line` read as JSON.
fn read(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line}"))
}

#[test]
fn with_one_desktop_listed_every_reply_is_unchanged_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let one = settings(
        directory.path(),
        "one.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}"),
    );
    let two = settings(
        directory.path(),
        "two.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"),
    );

    for command in ["status", "windows", "screenshot"] {
        let alone = printed(&atlas(&one, &home, &[command], ""));
        let plain = read(&alone);
        assert!(plain.get("desktop").is_none(), "{command}: {alone}");
        assert_eq!(alone, plain.to_string(), "{command}: one line, as ever");

        let mut beside = read(&printed(&atlas(&two, &home, &[command], "")));
        let named = beside.as_object_mut().unwrap().remove("desktop");
        assert_eq!(
            named,
            Some(json!({"name": "main", "kind": "user"})),
            "{command}"
        );
        assert_eq!(beside.to_string(), alone, "{command}: nothing else differs");
    }
}

#[test]
fn with_two_desktops_listed_every_reply_and_failure_of_an_agent_command_names_its_desktop() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let path = settings(
        directory.path(),
        "atlas.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"),
    );

    for (desktop, kind) in [("main", "user"), ("work", "vm")] {
        for command in ["status", "windows", "screenshot", "config"] {
            let output = atlas(&path, &home, &["--desktop", desktop, command], "");
            let said = read(&printed(&output));
            assert_eq!(
                said["desktop"],
                json!({"name": desktop, "kind": kind}),
                "{desktop} {command}: {said}"
            );
        }
    }
}

#[test]
fn listing_the_desktops_names_no_single_desktop() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let path = settings(
        directory.path(),
        "atlas.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"),
    );

    let listed = read(&printed(&atlas(&path, &home, &["desktop", "list"], "")));

    assert_eq!(listed["ok"], true, "{listed}");
    assert!(listed.get("desktop").is_none(), "{listed}");
    assert_eq!(listed["desktops"].as_array().unwrap().len(), 2);
}

/// What `atlas --desktop work mcp` answered to a status and a screenshot,
/// by request.
fn asked_over_stdio(path: &Path, home: &Path) -> (Value, Value) {
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "a test", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "atlas_status", "arguments": {}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "take_screenshot", "arguments": {}}}),
    ];
    let input: String = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect();
    let output = atlas(path, home, &["--desktop", "work", "mcp"], &input);
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(read)
        .collect();
    let answer = |id: i64| {
        replies
            .iter()
            .find(|reply| reply["id"] == id)
            .unwrap_or_else(|| panic!("no answer to {id}: {replies:?}"))["result"]
            .clone()
    };
    (answer(2), answer(3))
}

#[test]
fn an_mcp_status_and_screenshot_over_stdio_name_their_desktop_only_beside_another() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let two = settings(
        directory.path(),
        "two.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"),
    );
    let alone = settings(
        directory.path(),
        "alone.toml",
        &format!("default = \"work\"\noutput_dir = \"out\"\n{VM}"),
    );
    let work = json!({"name": "work", "kind": "vm"});

    for (path, words, named) in [
        (&two, "Atlas on work: ", work.clone()),
        (&alone, "Atlas: ", Value::Null),
    ] {
        let (status, screenshot) = asked_over_stdio(path, &home);
        assert_eq!(status["structuredContent"]["desktop"], named, "{status}");
        let text = status["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with(words), "{text}");
        assert!(!text.contains("helper"), "{text}");

        // The screenshot is the command's own, refused in its words.
        assert_eq!(screenshot["structuredContent"]["desktop"], named);
        assert_eq!(screenshot["isError"], true, "{screenshot}");
        let refused = screenshot["structuredContent"]["error"]["error"]
            .as_str()
            .unwrap();
        assert_eq!(
            refused, "work is a VM desktop, which this build of Atlas cannot reach yet.",
            "{screenshot}"
        );
    }
}

#[test]
fn an_mcp_screenshot_after_a_rename_names_the_vm_it_was_taken_on_in_its_words_and_facts() {
    use std::io::{BufRead, BufReader};

    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let other =
        "\n[[desktops]]\nname = \"work\"\nkind = \"vm\"\nid = \"89abcdef0123456701234567\"\n";
    let path = settings(
        directory.path(),
        "atlas.toml",
        &format!("default = \"main\"\noutput_dir = \"out\"\n{USER}{VM}"),
    );
    let mut server = atlas_core::spawn::spawn(
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("HOME", &home)
            .env(atlas_client::host_session::desktops::TEST_HOME, &home)
            .env("ATLAS_JSON", "1")
            .arg("--config")
            .arg(&path)
            .args(["--desktop", "work", "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let mut ask = |id: i64, method: &str, params: Value| {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(input, "{request}").unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        read(&line)["result"].clone()
    };
    let screenshot = json!({"name": "take_screenshot", "arguments": {}});
    ask(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "a test", "version": "1"}}),
    );

    let before = ask(2, "tools/call", screenshot.clone());
    // The VM is renamed archive, and work names another VM.
    std::fs::write(
        &path,
        format!(
            "default = \"main\"\noutput_dir = \"out\"\n{USER}{}{other}",
            VM.replace("\"work\"", "\"archive\"")
        ),
    )
    .unwrap();
    let after = ask(3, "tools/call", screenshot);
    // The closed input ends the server.
    drop(input);
    server.wait().unwrap();

    for (result, name) in [(before, "work"), (after, "archive")] {
        let facts = &result["structuredContent"];
        assert_eq!(
            facts["desktop"],
            json!({"name": name, "kind": "vm"}),
            "{result}"
        );
        let words = format!("{name} is a VM desktop, which this build of Atlas cannot reach yet.");
        assert_eq!(result["content"][0]["text"], words, "{result}");
        assert_eq!(facts["error"]["error"], words);
    }
}
