use super::*;

#[test]
fn a_full_name_is_read_from_directory_services_answer_on_one_line_or_the_next() {
    assert_eq!(
        parse_real_name(b"RealName: Atlas\n").as_deref(),
        Some("Atlas")
    );
    assert_eq!(
        parse_real_name(b"RealName:\n Atlas Helper\n").as_deref(),
        Some("Atlas Helper")
    );
    assert_eq!(parse_real_name(b"RealName:\n"), None);
    assert_eq!(parse_real_name(b"No such key: RealName\n"), None);
}
