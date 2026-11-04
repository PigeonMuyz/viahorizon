// SPDX-License-Identifier: GPL-3.0-or-later
use viafabricplus_native::vfp_hash as h;
#[test]
fn crc32c_matches_the_standard_check_value_and_viaversion_unknown_hash() {
    assert_eq!(h::crc32c(b"123456789") as u32, 0xe3069283);
    assert_eq!(h::crc32c(&(i32::MIN + 1).to_le_bytes()), 399825415);
}
#[test]
fn compound_and_component_values_match_upstream_golden_hashes() {
    // ViaVersion ItemHashTest.testCompoundTag, testNumberAndKey, testArrays.
    assert_eq!(
        h::map([
            (h::string("id"), h::int(1)),
            (h::string("type"), h::string("minecraft:stick")),
            (h::string("wow"), h::map([])),
            (h::string("list"), h::list([h::float(0.3)])),
            (h::string("emptylist"), h::list([])),
        ]),
        -859008863
    );
    assert_eq!(
        h::map([
            (h::string("seconds"), h::float(0.5)),
            (h::string("cooldown_group"), h::string("minecraft:stick"))
        ]),
        -524579578
    );
    assert_eq!(
        h::map([(h::string("floats"), h::list([h::float(1.0)]))]),
        2007278159
    );
}
#[test]
fn maps_ignore_entry_order_but_lists_and_numeric_types_do_not() {
    assert_eq!(h::map([(1, -2), (-3, 4)]), h::map([(-3, 4), (1, -2)]));
    assert_ne!(h::list([1, 2]), h::list([2, 1]));
    assert_ne!(h::int(1), h::byte(1));
    assert_ne!(h::boolean(true), h::byte(1));
}
#[test]
fn java_nan_canonicalization_and_utf16_are_preserved() {
    assert_eq!(h::float(f32::from_bits(0x7fffffff)), h::float(f32::NAN));
    assert_eq!(
        h::double(f64::from_bits(0x7ff0000000000001)),
        h::double(f64::NAN)
    );
    assert_ne!(h::float(0.0), h::float(-0.0));
    assert_eq!(h::string("\u{1f600}"), h::utf16(&[0xd83d, 0xde00]));
    assert_ne!(h::utf16(&[0xd800]), h::string("\u{fffd}"));
}
