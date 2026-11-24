//! Rust port of caretdev/go-irisnative list framing (MIT); see UPSTREAM.md.
use serde_json::{Value, json};
pub fn encode(value: &Value) -> Result<Vec<u8>, String> {
    if value.is_null() {
        return Ok(vec![1]);
    }
    let (kind, data) = if let Some(bytes) =
        superlink_drivers::arguments::binary(value).map_err(|_| "IRIS 二进制参数无效")?
    {
        (1, bytes)
    } else {
        match value {
            Value::Bool(v) => (4, vec![u8::from(*v)]),
            Value::Number(n) if n.is_i64() => integer(n.as_i64().unwrap()),
            Value::Number(n) => (1, n.to_string().into_bytes()),
            Value::String(v) if v.chars().any(|c| c as u32 > 127) => {
                (2, v.encode_utf16().flat_map(u16::to_le_bytes).collect())
            }
            Value::String(v) => (1, v.as_bytes().to_vec()),
            _ => return Err("IRIS 参数类型无效".into()),
        }
    };
    item(kind, &data)
}
pub fn item(kind: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() > 65534 {
        return Err("IRIS 单个列表项超过 64 KiB".into());
    }
    let mut output = if data.len() > 253 {
        let length = (data.len() + 1) as u16;
        vec![0, length as u8, (length >> 8) as u8, kind]
    } else {
        vec![(data.len() + 2) as u8, kind]
    };
    output.extend(data);
    Ok(output)
}
fn integer(value: i64) -> (u8, Vec<u8>) {
    let negative = value < 0;
    let mut magnitude = if negative {
        value.unsigned_abs() - 1
    } else {
        value as u64
    };
    let mut bytes = Vec::new();
    while magnitude > 0 {
        bytes.push((magnitude as u8) ^ if negative { 255 } else { 0 });
        magnitude >>= 8;
    }
    (if negative { 5 } else { 4 }, bytes)
}
pub struct Item<'a> {
    pub kind: u8,
    pub data: &'a [u8],
    pub null: bool,
}
pub struct Reader<'a> {
    pub bytes: &'a [u8],
    pub offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub fn next(&mut self) -> Result<Item<'a>, String> {
        let head = *self.bytes.get(self.offset).ok_or("IRIS 列表数据截断")?;
        self.offset += 1;
        if head == 1 {
            return Ok(Item {
                kind: 0,
                data: &[],
                null: true,
            });
        }
        let length = if head == 0 {
            let raw = self
                .bytes
                .get(self.offset..self.offset + 2)
                .ok_or("IRIS 长列表头截断")?;
            self.offset += 2;
            (u16::from_le_bytes([raw[0], raw[1]]) as usize)
                .checked_sub(1)
                .ok_or("IRIS 长度无效")?
        } else {
            (head as usize).checked_sub(2).ok_or("IRIS 长度无效")?
        };
        let kind = *self.bytes.get(self.offset).ok_or("IRIS 类型缺失")?;
        self.offset += 1;
        let data = self
            .bytes
            .get(self.offset..self.offset + length)
            .ok_or("IRIS 列表内容截断")?;
        self.offset += length;
        Ok(Item {
            kind: if (32..64).contains(&kind) {
                kind - 32
            } else {
                kind
            },
            data,
            null: false,
        })
    }
    pub fn number(&mut self) -> Result<i64, String> {
        self.next()?.number()
    }
    pub fn text(&mut self) -> Result<String, String> {
        self.next()?.text()
    }
}
impl Item<'_> {
    pub fn number(&self) -> Result<i64, String> {
        if self.data.len() > 8 {
            return Err("IRIS 整数长度无效".into());
        }
        let mut raw = [if self.kind == 5 { 255 } else { 0 }; 8];
        raw[..self.data.len()].copy_from_slice(self.data);
        Ok(i64::from_le_bytes(raw))
    }
    pub fn text(&self) -> Result<String, String> {
        if self.kind == 2 {
            if !self.data.len().is_multiple_of(2) {
                return Err("IRIS UTF-16 长度无效".into());
            }
            String::from_utf16(
                &self
                    .data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| u16::from_le_bytes([v[0], v[1]]))
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| "IRIS UTF-16 无效".into())
        } else {
            String::from_utf8(self.data.to_vec()).map_err(|_| "IRIS 文本编码无效".into())
        }
    }
    pub fn value(&self, sql_type: i64) -> Result<Value, String> {
        if self.null {
            return Ok(Value::Null);
        }
        if matches!(sql_type, -4..=-2) {
            return Ok(superlink_drivers::arguments::bytes(self.data));
        }
        match self.kind {
            4 | 5 => Ok(json!(self.number()?)),
            6 | 7 => {
                let exponent = *self.data.first().ok_or("IRIS 小数截断")? as i8;
                let mantissa = Item {
                    kind: if self.kind == 7 { 5 } else { 4 },
                    data: &self.data[1..],
                    null: false,
                }
                .number()?;
                let exact = decimal(mantissa, exponent);
                if matches!(sql_type, 2 | 3) {
                    Ok(json!(exact))
                } else {
                    Ok(json!(exact.parse::<f64>().map_err(|_| "IRIS 浮点数无效")?))
                }
            }
            1 | 2 => Ok(json!(if self.data == [0] {
                String::new()
            } else {
                self.text()?
            })),
            _ => Err("IRIS 返回未知列表类型".into()),
        }
    }
}
fn decimal(mantissa: i64, exponent: i8) -> String {
    let sign = if mantissa < 0 { "-" } else { "" };
    let mut digits = mantissa.unsigned_abs().to_string();
    if exponent >= 0 {
        digits.extend(std::iter::repeat_n('0', exponent as usize));
    } else {
        let scale = exponent.unsigned_abs() as usize;
        if scale >= digits.len() {
            digits = format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits);
        }
        digits.insert(digits.len() - scale, '.');
    }
    format!("{sign}{digits}")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_integer_unicode_and_frame_bounds() {
        for v in [
            json!(i64::MIN),
            json!(i64::MAX),
            json!(-1),
            json!(0),
            json!("中文😀"),
            Value::Null,
        ] {
            let bytes = encode(&v).unwrap();
            let value = Reader::new(&bytes).next().unwrap().value(12).unwrap();
            assert_eq!(v, value);
        }
        let bytes = item(1, &vec![42; 1024]).unwrap();
        assert_eq!(Reader::new(&bytes).next().unwrap().data.len(), 1024);
        for raw in [vec![], vec![0, 1], vec![9, 1]] {
            assert!(Reader::new(&raw).next().is_err());
        }
    }
}
