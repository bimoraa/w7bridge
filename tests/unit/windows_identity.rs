use super::*;

#[test]
fn token_sid_is_owned_and_repeatable( ) {

    let first = current_sid().unwrap();
    assert!(first.starts_with("S-1-"));
    assert!(first.bytes().all(|byte| byte.is_ascii_digit() || byte == b'S' || byte == b'-'));
    assert_eq!(first, current_sid().unwrap());

}
