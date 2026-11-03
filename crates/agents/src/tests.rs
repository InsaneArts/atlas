use super::*;

#[test]
fn the_plugin_is_found_in_each_agents_list() {
    assert_eq!(PLUGIN, format!("{NAME}@{NAME}"));
    let claude = br#"[{"id":"other@x"},{"id":"atlas@atlas","version":"1"}]"#;
    assert_eq!(Agent::Claude.listed(claude), Some(true));
    assert_eq!(Agent::Claude.listed(br#"[{"id":"other@x"}]"#), Some(false));

    let codex = br#"{"installed":[{"pluginId":"atlas@atlas"}]}"#;
    assert_eq!(Agent::Codex.listed(codex), Some(true));
    assert_eq!(Agent::Codex.listed(br#"{"installed":[]}"#), Some(false));
    assert_eq!(Agent::Codex.listed(claude), None, "not Codex's shape");
    assert_eq!(Agent::Claude.listed(b"not json"), None);
}

/// Removing reports success only when the plugin is gone, or when every
/// step worked and the list cannot be read.
#[test]
fn removing_checks_the_plugin_is_gone() {
    let failed = || vec![problem("claude plugin uninstall failed: not found")];
    assert!(removed(Agent::Claude, failed(), Ok(Some(false))).is_ok());
    assert!(removed(Agent::Claude, Vec::new(), Ok(Some(false))).is_ok());
    let error = removed(Agent::Claude, failed(), Ok(Some(true))).unwrap_err();
    assert!(error.error.contains("uninstall failed"));
    let error = removed(Agent::Codex, Vec::new(), Ok(Some(true))).unwrap_err();
    assert_eq!(error.error, "Codex still lists the Atlas plugin");
    assert!(removed(Agent::Claude, Vec::new(), Ok(None)).is_ok());
    assert!(removed(Agent::Claude, failed(), Ok(None)).is_err());
    assert!(removed(Agent::Claude, failed(), Err(problem("silent"))).is_err());
}

#[test]
fn connecting_registers_the_local_marketplace_then_installs() {
    let path = Path::new("/Applications/Atlas.app/Contents/Resources/agents");
    assert_eq!(
        Agent::Claude.connect(path),
        [
            vec![
                "plugin",
                "marketplace",
                "add",
                path.to_str().unwrap(),
                "--scope",
                "user"
            ],
            vec!["plugin", "install", "atlas@atlas", "--scope", "user"],
        ]
    );
    assert_eq!(
        Agent::Codex.connect(path),
        [
            vec!["plugin", "marketplace", "add", path.to_str().unwrap()],
            vec!["plugin", "add", "atlas@atlas"],
        ]
    );
    assert_eq!(
        Agent::Codex.disconnect()[0],
        ["plugin", "remove", "atlas@atlas"]
    );
    assert_eq!(
        Agent::Claude.disconnect()[1],
        ["plugin", "marketplace", "remove", "atlas"]
    );
}

#[test]
fn the_marketplace_source_is_read_from_each_agents_list() {
    let claude = br#"[{"name":"other","source":"github","repo":"o/r"},
            {"name":"atlas","source":"directory","path":"/a/agents","installLocation":"/a/agents"}]"#;
    assert_eq!(
        Agent::Claude.source_in(claude),
        Some(Source::Folder("/a/agents".into()))
    );
    let repository = br#"[{"name":"atlas","source":"github","repo":"someone/atlas"}]"#;
    assert_eq!(
        Agent::Claude.source_in(repository),
        Some(Source::Elsewhere("someone/atlas".into()))
    );
    assert_eq!(Agent::Claude.source_in(b"[]"), Some(Source::Absent));

    let codex = br#"{"marketplaces":[{"name":"atlas","root":"/a/agents",
            "marketplaceSource":{"sourceType":"local","source":"/a/agents"}}]}"#;
    assert_eq!(
        Agent::Codex.source_in(codex),
        Some(Source::Folder("/a/agents".into()))
    );
    let git = br#"{"marketplaces":[{"name":"atlas","root":"/cache",
            "marketplaceSource":{"sourceType":"git","source":"https://example.com/atlas.git"}}]}"#;
    assert_eq!(
        Agent::Codex.source_in(git),
        Some(Source::Elsewhere("https://example.com/atlas.git".into()))
    );
    assert_eq!(
        Agent::Codex.source_in(br#"{"marketplaces":[]}"#),
        Some(Source::Absent)
    );
    assert_eq!(Agent::Codex.source_in(claude), None, "not Codex's shape");
    assert_eq!(Agent::Claude.source_in(b"not json"), None);
}

#[test]
fn only_atlas_own_folders_count_as_its_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("checkout/agents");
    let manifest = checkout.join("atlas/.claude-plugin");
    fs::create_dir_all(&manifest).unwrap();
    fs::write(
        manifest.join("plugin.json"),
        format!(r#"{{"name":"atlas","homepage":"{HOMEPAGE}"}}"#),
    )
    .unwrap();
    assert!(atlas_folder(&checkout), "a checkout's, by its homepage");

    let someone = dir.path().join("someone/agents");
    let manifest = someone.join("atlas/.claude-plugin");
    fs::create_dir_all(&manifest).unwrap();
    fs::write(
        manifest.join("plugin.json"),
        r#"{"name":"atlas","homepage":"https://example.com"}"#,
    )
    .unwrap();
    assert!(!atlas_folder(&someone));
    assert!(!atlas_folder(&dir.path().join("missing")));
    assert!(
        !atlas_folder(Path::new(
            "/Volumes/Old/Atlas.app/Contents/Resources/agents"
        )),
        "a removed app's has no manifest to read"
    );

    // Another Atlas.app's, by its plugin's manifest; an app without one
    // is not Atlas's.
    let other = dir.path().join("Other.app/Contents/Resources/agents");
    let manifest = other.join("atlas/.codex-plugin");
    fs::create_dir_all(&manifest).unwrap();
    assert!(!atlas_folder(&other));
    fs::write(
        manifest.join("plugin.json"),
        format!(r#"{{"name":"atlas","homepage":"{HOMEPAGE}"}}"#),
    )
    .unwrap();
    assert!(atlas_folder(&other));
}

#[test]
fn the_manifests_give_the_homepage_that_marks_atlas_own() {
    let root = atlas_core::app_bundle::checkout().join("agents");
    assert!(atlas_folder(&root));

    for manifest in [
        ".claude-plugin/plugin.json",
        ".codex-plugin/plugin.json",
        ".cursor-plugin/plugin.json",
    ] {
        let text = std::fs::read_to_string(root.join("atlas").join(manifest)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["homepage"], HOMEPAGE, "{manifest}");
    }
}

/// Codex refuses hooks/hooks.json for its Claude Code `modules`, so its
/// manifest names hooks.json, which holds the same hooks without them.
#[test]
fn codex_reads_the_same_hooks_without_claude_code_modules() {
    let plugin = atlas_core::app_bundle::checkout().join("agents/atlas");
    let read = |name: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(plugin.join(name)).unwrap()).unwrap()
    };

    let mut claude = read("hooks/hooks.json");
    assert!(claude["modules"].is_array());
    claude.as_object_mut().unwrap().remove("modules");
    assert_eq!(read("hooks.json"), claude);
    assert_eq!(read(".codex-plugin/plugin.json")["hooks"], "./hooks.json");
}

/// Codex will not add over a marketplace of the same name, so another
/// of Atlas's own goes first; anyone else's stops the connection.
#[test]
fn connecting_replaces_only_a_marketplace_of_atlas_own() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Atlas.app/Contents/Resources/agents");
    fs::create_dir_all(&path).unwrap();
    let older = dir.path().join("Old/Atlas.app/Contents/Resources/agents");
    fs::create_dir_all(older.join("atlas/.claude-plugin")).unwrap();
    fs::write(
        older.join("atlas/.claude-plugin/plugin.json"),
        format!(r#"{{"name":"atlas","homepage":"{HOMEPAGE}"}}"#),
    )
    .unwrap();
    let old = Source::Folder(older);
    let remove = argv(&["plugin", "marketplace", "remove", "atlas"]);

    for agent in [Agent::Claude, Agent::Codex] {
        for source in [Some(&Source::Absent), Some(&Source::Folder(path.clone()))] {
            assert_eq!(
                agent.connecting(&path, source).unwrap(),
                agent.connect(&path),
                "{agent:?} {source:?}"
            );
        }

        fs::create_dir_all(dir.path().join("someone")).unwrap();
        let someone = Source::Folder(dir.path().join("someone"));
        let error = agent.connecting(&path, Some(&someone)).unwrap_err();
        assert!(error.error.contains("another marketplace named atlas"));
        let elsewhere = Source::Elsewhere("someone/atlas".into());
        assert!(agent.connecting(&path, Some(&elsewhere)).is_err());
    }

    let mut codex = vec![remove];
    codex.extend(Agent::Codex.connect(&path));
    assert_eq!(Agent::Codex.connecting(&path, Some(&old)).unwrap(), codex);
    assert_eq!(
        Agent::Claude.connecting(&path, Some(&old)).unwrap(),
        Agent::Claude.connect(&path),
        "Claude Code replaces it itself"
    );
}

/// Claude Code replaces a marketplace named atlas without asking, so an
/// unknown one stops it; Codex refuses to add over another itself.
#[test]
fn claude_code_is_not_connected_over_an_unknown_marketplace() {
    let path = Path::new("/Applications/Atlas.app/Contents/Resources/agents");
    let error = Agent::Claude.connecting(path, None).unwrap_err();
    assert_eq!(error.error, "Claude Code's marketplaces could not be read");
    assert_eq!(
        Agent::Claude
            .connecting(path, Some(&Source::Absent))
            .unwrap(),
        Agent::Claude.connect(path),
        "none at all is no reason to stop"
    );
    assert_eq!(
        Agent::Codex.connecting(path, None).unwrap(),
        Agent::Codex.connect(path)
    );
}

/// Removing leaves an agent alone unless its marketplace named atlas is
/// Atlas's own, and says whose it left (these cases run no tool).
#[test]
fn removing_never_touches_someone_elses_marketplace() {
    let dir = tempfile::tempdir().unwrap();
    let someone = Some(Source::Folder(dir.path().to_owned()));
    assert_eq!(
        disconnect_agent(Agent::Claude, Ok(someone)).unwrap(),
        Removal::Kept(dir.path().display().to_string())
    );
    let elsewhere = Some(Source::Elsewhere("someone/atlas".into()));
    assert_eq!(
        disconnect_agent(Agent::Codex, Ok(elsewhere)).unwrap(),
        Removal::Kept("someone/atlas".into())
    );
    assert_eq!(
        disconnect_agent(Agent::Codex, Ok(Some(Source::Absent))).unwrap(),
        Removal::Nothing
    );
    assert!(disconnect_agent(Agent::Claude, Ok(None)).is_err());
    assert!(disconnect_agent(Agent::Claude, Err(problem("silent"))).is_err());
}

/// Codex records where each marketplace comes from in its settings,
/// which stay readable while any marketplace folder is missing.
#[test]
fn codex_settings_say_where_its_atlas_marketplace_comes_from() {
    let local = "model = \"x\"\n\n[marketplaces.atlas]\nsource_type = \"local\"\n\
                     source = \"/a/agents\"\n\n[marketplaces.other]\nsource_type = \"local\"\n\
                     source = \"/gone\"\n";
    assert_eq!(
        codex_recorded_in(local),
        Some(Source::Folder("/a/agents".into()))
    );
    let git =
        "[marketplaces.atlas]\nsource_type = \"git\"\nsource = \"https://example.com/a.git\"\n";
    assert_eq!(
        codex_recorded_in(git),
        Some(Source::Elsewhere("https://example.com/a.git".into()))
    );
    assert_eq!(
        codex_recorded_in("[marketplaces.other]\nsource = \"/x\"\n"),
        Some(Source::Absent)
    );
    assert_eq!(codex_recorded_in(""), Some(Source::Absent));
    assert_eq!(codex_recorded_in("not = [toml"), None);

    let dir = tempfile::tempdir().unwrap();
    assert_eq!(codex_recorded(dir.path()), Some(Source::Absent), "no file");
    fs::write(dir.path().join("config.toml"), local).unwrap();
    assert_eq!(
        codex_recorded(dir.path()),
        Some(Source::Folder("/a/agents".into()))
    );
}

/// The plugin's manifests describe this version, and its skill and
/// command reference are exactly what this CLI generates.
#[test]
fn the_plugin_matches_this_cli() {
    let root = atlas_core::app_bundle::checkout().join("agents");

    for manifest in [
        ".claude-plugin/plugin.json",
        ".codex-plugin/plugin.json",
        ".cursor-plugin/plugin.json",
    ] {
        let text = std::fs::read_to_string(root.join("atlas").join(manifest)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["name"], NAME, "{manifest}");
        assert_eq!(value["version"], atlas_core::VERSION, "{manifest}");
    }

    for marketplace in [
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
    ] {
        let text = std::fs::read_to_string(root.join(marketplace)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["name"], NAME, "{marketplace}");
    }

    let skill = std::fs::read_to_string(root.join("atlas/skills/atlas/SKILL.md")).unwrap();
    assert_eq!(
        skill,
        crate::guide::skill(),
        "the plugin's skill is what atlas agent-guide --skill prints"
    );
}

/// Every error code the CLI can report is explained to the agent.
#[test]
fn every_error_code_is_in_the_troubleshooting_reference() {
    let root = atlas_core::app_bundle::checkout();
    let reference = std::fs::read_to_string(
        root.join("agents/atlas/skills/atlas/references/troubleshooting.md"),
    )
    .unwrap();

    let mut sources = Vec::new();
    let mut directories: Vec<_> = ["cli", "client", "core", "daemon", "native"]
        .map(|name| root.join("crates").join(name).join("src"))
        .into();
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                sources.push(std::fs::read_to_string(path).unwrap());
            }
        }
    }

    let mut missing = Vec::new();

    for source in &sources {
        for start in ["Error::new(", "failure("] {
            for (index, _) in source.match_indices(start) {
                let rest = source[index + start.len()..].trim_start();
                let Some(code) = rest
                    .strip_prefix('"')
                    .and_then(|rest| rest.split('"').next())
                else {
                    continue;
                };

                let documented = format!("| `{code}` |");
                if !code.is_empty()
                    && code.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                    && !reference.contains(&documented)
                    && !missing.contains(&code.to_owned())
                {
                    missing.push(code.to_owned());
                }
            }
        }
    }

    // Codes reached through each module's own problem() helper.
    for code in ["agents", "init", "setup", "uninstall", "update"] {
        if !reference.contains(&format!("| `{code}` |")) && !missing.contains(&code.to_owned()) {
            missing.push(code.to_owned());
        }
    }

    assert!(missing.is_empty(), "undocumented error codes: {missing:?}");
}
