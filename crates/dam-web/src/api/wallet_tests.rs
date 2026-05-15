use super::*;

#[test]
fn kind_from_key_extracts_prefix() {
    assert_eq!(kind_from_key("email:abc123"), "email");
    assert_eq!(kind_from_key("phone:xyz"), "phone");
    assert_eq!(kind_from_key("nokey"), "nokey");
}
