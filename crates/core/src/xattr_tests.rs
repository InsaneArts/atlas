use super::*;

const ROOMY: Bounds = Bounds {
    count: 128,
    value: 1 << 20,
    total: 4 << 20,
};

fn file() -> (tempfile::TempDir, std::fs::File) {
    let folder = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(folder.path().join("file")).unwrap();
    (folder, file)
}

fn attribute(name: &str, value: &[u8]) -> Attribute {
    Attribute {
        name: name.to_owned(),
        value: value.to_vec(),
    }
}

#[test]
fn attributes_set_are_read_back_sorted_and_whole() {
    let (_folder, file) = file();
    set(&file, &attribute("org.example.b", b"two")).unwrap();
    set(&file, &attribute("com.apple.quarantine", b"0081;one")).unwrap();
    set(&file, &attribute("org.example.empty", b"")).unwrap();

    let read: Vec<_> = of(&file, ROOMY)
        .unwrap()
        .into_iter()
        .filter(|read| read.name != "com.apple.provenance")
        .collect();

    let names: Vec<_> = read.iter().map(|read| read.name.as_str()).collect();
    assert_eq!(
        names,
        ["com.apple.quarantine", "org.example.b", "org.example.empty"]
    );
    assert_eq!(read[0].value, b"0081;one");
    assert!(read[2].value.is_empty());
}

#[test]
fn attributes_past_their_bounds_are_refused_before_they_are_read() {
    let (_folder, file) = file();
    set(&file, &attribute("org.example.big", &[7; 4096])).unwrap();
    set(&file, &attribute("org.example.small", b"x")).unwrap();

    let value = of(
        &file,
        Bounds {
            value: 4095,
            ..ROOMY
        },
    );
    let count = of(&file, Bounds { count: 1, ..ROOMY });
    let total = of(
        &file,
        Bounds {
            total: 4096,
            ..ROOMY
        },
    );

    assert!(value.is_err());
    assert!(count.is_err());
    assert!(total.is_err());
}

#[test]
fn a_name_longer_than_macos_allows_is_refused() {
    let (_folder, file) = file();

    let set = set(&file, &attribute(&"n".repeat(NAME + 1), b"x"));

    assert_eq!(set.unwrap_err().code, Code::BadRequest);
}
