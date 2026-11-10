use super::*;

#[test]
fn json_is_for_programs_and_text_for_terminals() {
    assert_eq!(detect(true, None, true), Format::Json);
    assert_eq!(detect(false, None, true), Format::Human);
    assert_eq!(detect(false, None, false), Format::Json);
    assert_eq!(detect(false, Some("1"), true), Format::Json);
    assert_eq!(detect(false, Some("0"), false), Format::Human);
    assert_eq!(detect(true, Some("0"), true), Format::Json, "--json wins");
    assert_eq!(detect(false, Some(""), false), Format::Json);
}
