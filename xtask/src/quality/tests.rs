use super::*;

fn scan(source: &str) -> Scan<'_> {
    let parsed = syn::parse_file(source).unwrap();
    let mut scan = Scan::new("src/fixture.rs".into(), source, Context::default());
    scan.visit_file(&parsed);
    scan
}

fn fixture() -> &'static str {
    include_str!("../../fixtures/quality/source.rs")
}

fn fixture_tree(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for path in [
        "crates/fixture/src",
        "crates/cli/tests",
        "xtask/src",
        "tests",
        "scripts",
    ] {
        fs::create_dir_all(directory.path().join(path)).unwrap();
    }
    fs::write(directory.path().join("crates/fixture/src/lib.rs"), source).unwrap();
    directory
}

fn ledger(sites: &[Value]) -> Value {
    let entries = sites
        .iter()
        .map(|site| {
            let mut entry = site.clone();
            entry["owner"] = "fixture owner".into();
            entry["reason"] = format!(
                "Review {} in {} before closing debt",
                site["kind"], site["symbol"]
            )
            .into();
            entry["status"] = "open_debt".into();
            entry
        })
        .collect::<Vec<_>>();
    json!({"dispositions":entries,"recipe_names":[],"feature_recipes":[]})
}

fn report(scan: &Scan<'_>) -> Value {
    json!({"sites":scan.sites,"functions":scan.functions})
}

#[test]
fn test_items_do_not_count_as_production() {
    let scan = scan(fixture());
    assert_eq!(scan.excluded, [(16, 19), (20, 23)]);
    assert_eq!(
        scan.functions.iter().filter(|f| f["test"] == false).count(),
        4
    );
    assert!(!scan.sites.iter().any(|site| site["kind"] == "unwrap"));
    let changed = fixture().replace("#[cfg(test)]\nimpl", "#[cfg(feature = \"live\")]\nimpl");
    assert!(
        self::scan(&changed)
            .sites
            .iter()
            .any(|site| site["kind"] == "unwrap")
    );
    let unicode = self::scan("fn café() {\n    // é\n}\n");
    assert_eq!(unicode.functions[0]["lines"], 3);
    let directory = fixture_tree(fixture());
    fs::write(directory.path().join("scripts/fixture.sh"), "sleep 1\n").unwrap();
    let inventory = inventory(directory.path()).unwrap();
    assert_eq!(
        inventory["files"][0]["production"],
        fixture().lines().count() - 8
    );
    assert_eq!(inventory["external_timing"].as_array().unwrap().len(), 1);
    assert!(
        inventory["sites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|site| site["symbol"] == "external test timing")
    );
    let long = format!("fn long() {{\n{}}}", "\n".repeat(79));
    let long = self::scan(&long);
    assert_eq!(long.functions[0]["lines"], 81);
    assert!(
        long.sites
            .iter()
            .any(|site| site["kind"] == "function_size")
    );
}

#[test]
fn macro_indices_keep_their_source_owner() {
    let scan = scan(fixture());
    let indices = scan
        .sites
        .iter()
        .filter(|site| site["kind"] == "index")
        .collect::<Vec<_>>();
    assert_eq!(indices.len(), 3);
    assert!(indices.iter().all(|site| site["symbol"] == "decisions"));
    assert_eq!(indices[0]["line"], 7);
    assert_eq!(indices[2]["line"], 8);
    let changed = fixture().replace("inner!(rows[0])", "inner!(rows[4])");
    let changed = self::scan(&changed);
    assert!(
        check(&report(&changed), &ledger(&scan.sites))
            .unwrap_err()
            .contains("stale")
    );
}

#[test]
fn associated_test_items_exclude_attributes_and_documentation() {
    let source = include_str!("../../fixtures/quality/mixed_impl.rs");
    let scan = scan(source);
    assert_eq!(scan.excluded, [(4, 8), (9, 11)]);
    assert_eq!(scan.declarations, 2);
    assert!(!scan.sites.iter().any(|site| site["kind"] == "unwrap"));
    let directory = fixture_tree(source);
    let report = inventory(directory.path()).unwrap();
    assert_eq!(report["files"][0]["production"], source.lines().count() - 8);
    let changed = source.replace("#[cfg(test)]", "#[cfg(feature = \"live\")]");
    fs::write(directory.path().join("crates/fixture/src/lib.rs"), &changed).unwrap();
    let report = inventory(directory.path()).unwrap();
    assert_eq!(report["files"][0]["production"], source.lines().count());
    assert!(
        self::scan(&changed)
            .sites
            .iter()
            .any(|site| site["kind"] == "unwrap")
    );
}

#[test]
fn trait_signatures_are_declarations_without_bodies_or_public_syntax() {
    let source = include_str!("../../fixtures/quality/traits.rs");
    let scan = scan(source);
    assert_eq!(scan.declarations, 6);
    assert_eq!(scan.public_nodes, 1);
    assert_eq!(scan.excluded, [(4, 6)]);
    let bodies = scan
        .functions
        .iter()
        .filter(|function| function["test"] == false)
        .collect::<Vec<_>>();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["symbol"], "Private::body");
    assert_eq!(bodies[1]["symbol"], "Public::body");
    assert_eq!(
        scan.sites
            .iter()
            .filter(|site| site["kind"] == "declaration")
            .count(),
        6
    );
    let changed = source.replace("fn signature(&self);", "fn signature(&self) {}");
    assert_eq!(
        self::scan(&changed)
            .functions
            .iter()
            .filter(|function| function["test"] == false)
            .count(),
        4
    );
}

#[test]
fn trait_contract_changes_require_fresh_dispositions() {
    let source = "#[doc = \"Contract\"] pub trait Service<T: Clone>: Send + Sync where T: Send {}";
    let original = scan(source);
    let dispositions = ledger(&original.sites);
    assert!(check(&report(&original), &dispositions).is_ok());
    for (from, to) in [
        ("Contract", "Updated contract"),
        ("pub trait", "trait"),
        ("trait Service", "unsafe trait Service"),
        ("T: Clone", "T: Copy"),
        ("Send + Sync", "Send"),
        ("where T: Send", "where T: Sync"),
    ] {
        let changed_source = source.replace(from, to);
        let changed = self::scan(&changed_source);
        assert_eq!(
            check(&report(&changed), &dispositions).unwrap_err(),
            "stale disposition: src/fixture.rs|Service|declaration|1",
            "{from} -> {to}"
        );
    }
}

#[test]
fn associated_type_changes_require_fresh_dispositions() {
    for (source, symbol, from, to) in [
        (
            "trait Service { type Output<'a>: Clone where Self: 'a; }",
            "Service::Output",
            "Clone",
            "Copy",
        ),
        (
            "trait Service { type Output<'a>: Clone where Self: 'a; }",
            "Service::Output",
            "Self: 'a",
            "Self: 'static",
        ),
        (
            "trait Service { type Output = u8; }",
            "Service::Output",
            "u8",
            "u16",
        ),
        (
            "struct Local; impl Service for Local { type Output = u8; }",
            "Local::Output",
            "u8",
            "u16",
        ),
    ] {
        let original = scan(source);
        let site = original
            .sites
            .iter()
            .find(|site| site["symbol"] == symbol && site["kind"] == "declaration")
            .unwrap();
        let dispositions = ledger(std::slice::from_ref(site));
        let changed_source = source.replace(from, to);
        let changed = self::scan(&changed_source);
        let updated = changed
            .sites
            .iter()
            .find(|site| site["symbol"] == symbol && site["kind"] == "declaration")
            .unwrap();
        assert_eq!(
            check(&json!({"sites":[updated]}), &dispositions).unwrap_err(),
            format!("stale disposition: src/fixture.rs|{symbol}|declaration|1")
        );
    }
    let tests = scan(
        "#[cfg(test)] trait Service { type Output; } impl Local { #[cfg(test)] type Output = u8; }",
    );
    assert_eq!(tests.declarations, 0);
    assert!(tests.sites.is_empty());
}

#[test]
fn foreign_declarations_track_signatures_attributes_and_unsafe_context() {
    let source = include_str!("../../fixtures/quality/foreign.rs");
    let original = scan(source);
    assert_eq!(original.declarations, 5);
    assert_eq!(original.public_nodes, 1);
    assert!(original.functions.is_empty());
    assert_eq!(original.excluded, [(10, 12)]);
    let block = original
        .sites
        .iter()
        .find(|site| site["kind"] == "unsafe_extern")
        .unwrap();
    assert_eq!(block["detail"]["allowed"], true);
    let foreign = original
        .sites
        .iter()
        .filter(|site| site["kind"] == "declaration" && site["detail"]["type"] == "foreign")
        .collect::<Vec<_>>();
    assert_eq!(foreign.len(), 3);
    assert!(foreign.iter().all(|site| site["detail"]["allowed"] == true));
    let function = foreign
        .iter()
        .find(|site| site["symbol"].as_str().unwrap().ends_with("::call"))
        .unwrap();
    for (from, to) in [
        ("value: u8", "value: u16"),
        ("foreign_call", "renamed_call"),
        ("A foreign function.", "Updated foreign function."),
    ] {
        let changed_source = source.replace(from, to);
        let changed = self::scan(&changed_source);
        let updated = changed
            .sites
            .iter()
            .find(|site| site["symbol"] == function["symbol"] && site["kind"] == "declaration")
            .unwrap();
        assert_ne!(function["fingerprint"], updated["fingerprint"]);
        assert!(
            check(&report(&changed), &ledger(&original.sites))
                .unwrap_err()
                .contains("stale")
        );
    }
    let changed_source = source.replace("extern \"C\"", "extern \"system\"");
    let changed = self::scan(&changed_source);
    let updated = changed
        .sites
        .iter()
        .find(|site| site["kind"] == "unsafe_extern")
        .unwrap();
    assert_ne!(block["fingerprint"], updated["fingerprint"]);
    assert!(check(&report(&changed), &ledger(&original.sites)).is_err());
    let changed_source = source.replace("#[allow(unsafe_code)]", "");
    let changed = self::scan(&changed_source);
    assert!(
        changed
            .sites
            .iter()
            .filter(|site| site["kind"] == "unsafe_extern" || site["detail"]["type"] == "foreign")
            .all(|site| site["detail"]["allowed"] == false)
    );
}

#[test]
fn parent_unsafe_allowances_apply_to_child_files() {
    let parent = syn::parse_file(include_str!("../../fixtures/quality/parent.rs")).unwrap();
    let mut contexts = BTreeMap::new();
    inherited(
        &parent.items,
        Path::new("src"),
        Context::default(),
        &mut contexts,
    );
    let context = contexts[Path::new("src/child.rs")];
    assert!(context.allowed);
    let source = include_str!("../../fixtures/quality/child.rs");
    let mut child = Scan::new("src/child.rs".into(), source, context);
    child.visit_file(&syn::parse_file(source).unwrap());
    let site = child
        .sites
        .iter()
        .find(|site| site["kind"] == "unsafe_block")
        .unwrap();
    assert_eq!(site["detail"]["allowed"], true);
    let removed =
        include_str!("../../fixtures/quality/parent.rs").replace("#[allow(unsafe_code)]", "");
    let mut contexts = BTreeMap::new();
    inherited(
        &syn::parse_file(&removed).unwrap().items,
        Path::new("src"),
        Context::default(),
        &mut contexts,
    );
    assert!(!contexts[Path::new("src/child.rs")].allowed);
    let directory = fixture_tree(include_str!("../../fixtures/quality/parent.rs"));
    fs::write(directory.path().join("crates/fixture/src/child.rs"), source).unwrap();
    let report = inventory(directory.path()).unwrap();
    let block = report["sites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|site| site["kind"] == "unsafe_block")
        .unwrap();
    assert_eq!(block["detail"]["allowed"], true);
}

#[test]
fn consecutive_bool_parameters_keep_their_callers() {
    let directory = fixture_tree(fixture());
    let report = inventory(directory.path()).unwrap();
    let pair = report["sites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|site| site["kind"] == "bool_parameters")
        .unwrap();
    assert!(
        pair["detail"]["callers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|caller| caller["symbol"] == "caller")
    );
    let scan = scan(fixture());
    let pairs = scan
        .sites
        .iter()
        .filter(|site| site["kind"] == "bool_parameters")
        .collect::<Vec<_>>();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["symbol"], "decisions");
    assert_eq!(pairs[0]["detail"]["parameters"], json!(["first", "second"]));
    assert!(
        scan.calls
            .iter()
            .any(|call| call["name"] == "decisions" && call["symbol"] == "caller")
    );
    let fields = scan
        .sites
        .iter()
        .filter(|site| site["kind"] == "bool_field")
        .map(|site| &site["symbol"])
        .collect::<Vec<_>>();
    assert_eq!(fields, [&json!("First"), &json!("Second")]);
    let changed = self::scan("fn decisions(first: bool, middle: u8, second: bool) {}");
    assert!(
        !changed
            .sites
            .iter()
            .any(|site| site["kind"] == "bool_parameters")
    );
}

#[test]
fn changed_sites_need_a_fresh_disposition() {
    let original = scan("fn read() { storage().ok(); }");
    let changed = scan("fn read() { other_storage().ok(); }");
    assert!(
        check(&report(&changed), &ledger(&original.sites))
            .unwrap_err()
            .contains("stale")
    );
    let added = scan("fn read() { storage().ok(); storage().ok(); }");
    assert!(check(&report(&added), &ledger(&original.sites)).is_err());
    assert!(check(&report(&original), &ledger(&added.sites)).is_err());
    let mut blanket = ledger(&original.sites);
    blanket["dispositions"][0]["reason"] = "all".into();
    assert!(
        check(&report(&original), &blanket)
            .unwrap_err()
            .contains("blanket")
    );
}

#[test]
fn open_debt_is_reported_without_becoming_permission() {
    let scan = scan(fixture());
    let debt = ledger(&scan.sites);
    assert_eq!(check(&report(&scan), &debt).unwrap()[0], scan.sites.len());
    assert!(scan.sites.iter().any(|site| site["kind"] == "fallback"));
    assert!(scan.sites.iter().any(|site| site["kind"] == "drop_chain"));
    let mut proof = debt;
    proof["dispositions"][0]["status"] = "retained_proof".into();
    assert!(
        check(&report(&scan), &proof)
            .unwrap_err()
            .contains("missing local proof")
    );
}

#[test]
fn feature_recipes_reject_missing_sites() {
    let scan = scan("fn route() { match input { A => 1, B => 2 } } ");
    let mut ledger = ledger(&scan.sites);
    ledger["recipe_names"] = json!(["fixture recipe"]);
    ledger["feature_recipes"] = json!([{"name":"fixture recipe","count":1,"sites":[scan.sites.iter().find(|site| site["kind"] == "declaration").unwrap()]}]);
    assert!(check(&report(&scan), &ledger).is_ok());
    ledger["feature_recipes"][0]["sites"][0]["fingerprint"] = "stale".into();
    assert!(
        check(&report(&scan), &ledger)
            .unwrap_err()
            .contains("feature site")
    );
    ledger["feature_recipes"][0]["sites"] = json!([]);
    assert!(
        check(&report(&scan), &ledger)
            .unwrap_err()
            .contains("missing site")
    );
    ledger["feature_recipes"] = json!([]);
    assert!(
        check(&report(&scan), &ledger)
            .unwrap_err()
            .contains("missing or duplicate")
    );
}
