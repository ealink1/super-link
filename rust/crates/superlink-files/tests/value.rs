//! Cell rendering shared by every text format.

use serde_json::json;
use superlink_files::value::{Cell, base64_encode, format_float};

#[test]
fn text_rendering_matches_the_go_helpers() {
    assert_eq!(Cell::Null.text(), "NULL");
    assert_eq!(Cell::Bool(true).text(), "true");
    assert_eq!(Cell::Int(-5).text(), "-5");
    assert_eq!(Cell::UInt(5).text(), "5");
    assert_eq!(Cell::Float(1.5).text(), "1.5");
    assert_eq!(Cell::Text("中文".to_string()).text(), "中文");
    assert_eq!(Cell::Bytes(b"hello".to_vec()).text(), "aGVsbG8=");
    assert_eq!(Cell::Json(json!({"a": 1})).text(), "{\"a\":1}");
    assert!(Cell::Null.is_null());
    assert_eq!(Cell::Text("中文".to_string()).text_len(), 6);
}

#[test]
fn floats_use_go_shortest_representation() {
    assert_eq!(format_float(1.0), "1");
    assert_eq!(format_float(1.5), "1.5");
    assert_eq!(format_float(1e300), "1e+300");
    assert_eq!(format_float(-0.001), "-0.001");
}

#[test]
fn base64_matches_the_standard_alphabet() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"a"), "YQ==");
    assert_eq!(base64_encode(b"ab"), "YWI=");
    assert_eq!(base64_encode(b"abc"), "YWJj");
}

#[test]
fn json_values_convert_back_to_cells() {
    assert_eq!(Cell::from(json!(null)), Cell::Null);
    assert_eq!(Cell::from(json!(true)), Cell::Bool(true));
    assert_eq!(
        Cell::from(json!(9223372036854775807i64)),
        Cell::Int(i64::MAX)
    );
    assert_eq!(Cell::from(json!([1])), Cell::Json(json!([1])));
    assert_eq!(
        Cell::Bytes(b"hi".to_vec()).to_json().unwrap(),
        json!("aGk=")
    );
    assert!(Cell::Float(f64::INFINITY).to_json().is_err());
    assert_eq!(Cell::Text("x".to_string()).to_json().unwrap(), json!("x"));
}
