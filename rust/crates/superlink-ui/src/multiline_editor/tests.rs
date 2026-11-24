use super::{candidates, char_to_byte};
use superlink_sql::CompletionTable;

fn catalog() -> Vec<CompletionTable> {
    vec![CompletionTable {
        schema: "public".into(),
        name: "orders".into(),
        columns: vec![
            "id".into(),
            "amount".into(),
            "customer_id".into(),
            "created_at".into(),
        ],
    }]
}

#[test]
fn keywords_complete_without_metadata() {
    let text = "select * from orders where in";
    let (start, end, items) = candidates(text, text.len(), "postgresql", &[]).unwrap();
    assert_eq!(&text[start..end], "in");
    assert!(items.iter().any(|item| item.text == "INNER"));
    assert!(items.iter().all(|item| item.kind == "关键字"));
}

#[test]
fn tables_complete_after_from() {
    let text = "select * from ord";
    let (start, end, items) = candidates(text, text.len(), "postgresql", &catalog()).unwrap();
    assert_eq!(&text[start..end], "ord");
    let table = items.iter().find(|item| item.text == "orders").unwrap();
    assert_eq!(table.kind, "表");
}

#[test]
fn qualifiers_complete_columns() {
    let text = "select o. from orders o";
    let caret = "select o.".len();
    let (start, end, items) = candidates(text, caret, "postgresql", &catalog()).unwrap();
    assert_eq!(&text[start..end], "");
    assert!(
        items
            .iter()
            .any(|item| item.text == "amount" && item.kind == "字段")
    );
    assert!(items.iter().all(|item| item.text != "orders"));
}

#[test]
fn column_prefixes_filter_the_qualified_list() {
    let text = "select o.cu from orders o";
    let caret = "select o.cu".len();
    let (start, end, items) = candidates(text, caret, "postgresql", &catalog()).unwrap();
    assert_eq!(&text[start..end], "cu");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text, "customer_id");
}

#[test]
fn inside_a_string_nothing_is_offered() {
    let text = "select * from orders where name = 'ord";
    assert!(candidates(text, text.len(), "postgresql", &catalog()).is_none());
}

#[test]
fn char_offsets_convert_to_byte_offsets() {
    assert_eq!(char_to_byte("", 0), 0);
    assert_eq!(char_to_byte("select", 0), 0);
    assert_eq!(char_to_byte("select", 3), 3);
    assert_eq!(char_to_byte("select", 6), 6);
    assert_eq!(char_to_byte("select", 99), 6);
}

#[test]
fn multi_byte_identifiers_stay_on_char_boundaries() {
    let text = "-- 注释\nselect 1";
    assert_eq!(char_to_byte(text, 0), 0);
    assert_eq!(char_to_byte(text, 1), 1);
    assert_eq!(char_to_byte(text, 2), 2);
    assert_eq!(char_to_byte(text, 4), 6);
    assert!(text.is_char_boundary(char_to_byte(text, 5)));
}
