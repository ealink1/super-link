use std::io::{self, Write};
use superlink_domain::ResultSet;

const PREVIEW_BYTES: usize = 128 * 1024;

fn serialize_rows(result: &ResultSet, output: &mut Bounded) -> Option<()> {
    output.write_all(b"[\n").ok()?;
    for (index, row) in result.rows.iter().enumerate() {
        if index > 0 {
            output.write_all(b",\n").ok()?;
        }
        output.write_all(b"  {\n").ok()?;
        for (column_index, column) in result.columns.iter().enumerate() {
            if column_index > 0 {
                output.write_all(b",\n").ok()?;
            }
            output.write_all(b"    ").ok()?;
            serde_json::to_writer(&mut *output, &column.name).ok()?;
            output.write_all(b": ").ok()?;
            serde_json::to_writer(
                &mut *output,
                row.get(column_index).unwrap_or(&serde_json::Value::Null),
            )
            .ok()?;
        }
        output.write_all(b"\n  }").ok()?;
    }
    output.write_all(b"\n]").ok()?;
    Some(())
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let available = PREVIEW_BYTES.saturating_sub(self.0.len());
        if buf.len() > available {
            return Err(io::Error::other("preview limit"));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Borrow cells and stop serialization before allocating or laying out a huge page.
pub fn json_preview(result: &ResultSet) -> String {
    let mut output = Bounded(Vec::new());
    let truncated = serialize_rows(result, &mut output).is_none();
    let mut text = String::from_utf8_lossy(&output.0).into_owned();
    if truncated {
        text.push_str("\n… JSON 预览已截断（128 KiB），请导出查看完整内容");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::Column;
    #[test]
    fn json_preserves_column_names_null_and_unicode() {
        let result = ResultSet {
            columns: vec![
                Column {
                    name: "内容".into(),
                    ..Default::default()
                },
                Column {
                    name: "空值".into(),
                    ..Default::default()
                },
            ],
            rows: vec![vec![serde_json::json!("测试")]],
            ..Default::default()
        };
        let json: serde_json::Value = serde_json::from_str(&json_preview(&result)).unwrap();
        assert_eq!(json[0]["内容"], "测试");
        assert!(json[0]["空值"].is_null());
    }
    #[test]
    fn huge_cell_cannot_expand_preview_without_bound() {
        let result = ResultSet {
            columns: vec![Column {
                name: "payload".into(),
                ..Default::default()
            }],
            rows: vec![vec![serde_json::json!("中".repeat(PREVIEW_BYTES))]],
            ..Default::default()
        };
        let text = json_preview(&result);
        assert!(text.len() < PREVIEW_BYTES + 200);
        assert!(text.contains("预览已截断"));
    }
}
