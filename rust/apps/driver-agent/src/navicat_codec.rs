//! Bounds-checked, big-endian Navicat script response decoder.
use serde_json::{Value, json};
use superlink_domain::{Column, ResultSet};
pub struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Parser<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn read(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(count).ok_or("HTTP 隧道长度溢出")?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or("HTTP 隧道响应截断")?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.read(1)?[0])
    }
    fn word(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(
            self.read(4)?.try_into().map_err(|_| "响应头截断")?,
        ))
    }
    pub fn block(&mut self) -> Result<&'a [u8], String> {
        let first = self.byte()?;
        self.block_after(first)
    }
    fn block_after(&mut self, first: u8) -> Result<&'a [u8], String> {
        let length = if first == 254 {
            self.word()? as usize
        } else {
            first as usize
        };
        self.read(length)
    }
    pub fn header(&mut self) -> Result<(), String> {
        if self.word()? != 1111 {
            return Err("响应不是 Navicat MySQL HTTP 隧道协议".into());
        }
        self.read(2)?;
        let code = self.word()?;
        self.read(6)?;
        if code != 0 {
            self.block()?;
            return Err(format!("HTTP 隧道连接错误 {code}"));
        }
        Ok(())
    }
    pub fn result(&mut self, limit: u32) -> Result<ResultSet, String> {
        self.header()?;
        let code = self.word()?;
        let affected = self.word()?;
        self.word()?;
        let columns = self.word()? as usize;
        let rows = self.word()? as usize;
        self.read(12)?;
        if code != 0 {
            self.block()?;
            return Err(format!("HTTP 隧道查询错误 {code}"));
        }
        if columns > 4096 || rows > 1_000_000 || columns.saturating_mul(rows) > 2_000_000 {
            return Err("HTTP 隧道结果超过解码预算".into());
        }
        let mut result = ResultSet {
            rows_affected: if affected == u32::MAX {
                0
            } else {
                affected.into()
            },
            ..Default::default()
        };
        if columns == 0 {
            let info = self.block()?;
            if !info.is_empty() {
                result
                    .messages
                    .push(String::from_utf8_lossy(info).into_owned());
            }
        }
        let mut types = Vec::with_capacity(columns);
        for _ in 0..columns {
            let name = std::str::from_utf8(self.block()?)
                .map_err(|_| "隧道列名编码无效")?
                .to_string();
            self.block()?;
            let kind = self.word()?;
            let flags = self.word()?;
            self.word()?;
            types.push((kind, flags));
            result.columns.push(Column {
                id: name.clone(),
                name,
            });
        }
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        for _ in 0..rows {
            let mut row = Vec::with_capacity(columns);
            for (kind, flags) in &types {
                let first = self.byte()?;
                let value = if first == 255 {
                    Value::Null
                } else {
                    cell(self.block_after(first)?, *kind, *flags)?
                };
                row.push(value);
            }
            if (limit > 0 && result.rows.len() >= limit as usize) || !budget.accept(&row) {
                result.truncated = true;
            } else if !result.truncated {
                result.rows.push(row);
            }
        }
        if self.byte()? != 0 || self.offset != self.bytes.len() {
            return Err("HTTP 隧道结果结束标记无效".into());
        }
        Ok(result)
    }
}
fn cell(bytes: &[u8], kind: u32, flags: u32) -> Result<Value, String> {
    if bytes.len() > 1 << 20 {
        return Err("HTTP 隧道单元格超过 1 MiB".into());
    }
    if kind == 16 || matches!(kind, 249..=255) && flags & 128 != 0 {
        return Ok(superlink_drivers::arguments::bytes(bytes));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "HTTP 隧道文本编码无效")?;
    Ok(match kind {
        1 | 2 | 3 | 8 | 9 | 13 => {
            if flags & 32 != 0 {
                json!(text.parse::<u64>().map_err(|_| "隧道无符号整数无效")?)
            } else {
                json!(text.parse::<i64>().map_err(|_| "隧道整数无效")?)
            }
        }
        4 | 5 => json!(text.parse::<f64>().map_err(|_| "隧道浮点数无效")?),
        _ => json!(text),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_malformed_lengths_before_allocating() {
        assert!(Parser::new(&[0; 16]).result(1).is_err());
        let mut bytes = 1111u32.to_be_bytes().to_vec();
        bytes.extend([0; 12]);
        bytes.extend([0; 12]);
        bytes.extend(u32::MAX.to_be_bytes());
        bytes.extend(1u32.to_be_bytes());
        bytes.extend([0; 12]);
        assert!(Parser::new(&bytes).result(1).is_err());
    }
    #[test]
    fn keeps_numeric_and_binary_values_exact() {
        assert_eq!(cell(b"9223372036854775807", 8, 0).unwrap(), json!(i64::MAX));
        assert_eq!(
            cell(b"12345678901234567890.123456789", 246, 0).unwrap(),
            json!("12345678901234567890.123456789")
        );
        assert_eq!(
            superlink_drivers::arguments::binary(&cell(&[0, 255], 252, 128).unwrap()).unwrap(),
            Some(vec![0, 255])
        );
    }
}
