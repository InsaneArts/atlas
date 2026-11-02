use super::{first_line, hex, lower_hex, plain, random_hex};

#[test]
fn the_first_line_of_output_is_bounded() {
    assert_eq!(
        first_line(b"\n\nerror: x\nmore", 300).as_deref(),
        Some("error: x")
    );
    assert_eq!(
        first_line(&[b'x'; 500], 200).map(|line| line.len()),
        Some(200)
    );
    assert_eq!(first_line(b"  \n", 10), None);
}

#[test]
fn hex_is_lowercase_two_digits_a_byte_and_read_back_exactly() {
    assert_eq!(hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    assert!(lower_hex("000fa0ff", 8));
    assert!(!lower_hex("000FA0FF", 8), "uppercase is not Atlas's");
    assert!(!lower_hex("000fa0f", 8), "every digit is there");
    for digits in [1, 6, 12, 48] {
        assert!(lower_hex(&random_hex(digits), digits), "{digits}");
    }
    assert_ne!(random_hex(48), random_hex(48));
}

#[test]
fn a_plain_label_is_short_and_of_safe_characters() {
    assert!(plain("d9031471-5e2a:task_1.b", 64));
    assert!(!plain("", 64));
    assert!(!plain(&"a".repeat(65), 64));
    for refused in ["a b", "a/b", "é", "a\nb"] {
        assert!(!plain(refused, 64), "{refused:?}");
    }
}
