//! Bounds retained query values without allocating a second serialized copy.
use serde_json::Value;
use std::io::{self, Write};
const MAX_BYTES: usize = 32 << 20;
const MAX_FIELD: usize = 1 << 20;
#[derive(Default)]
pub struct ResultBudget {
    bytes: usize,
}
impl ResultBudget {
    pub fn accept(&mut self, row: &[Value]) -> bool {
        if row.len() > 4096
            || row
                .iter()
                .any(|v| v.as_str().is_some_and(|s| s.len() > MAX_FIELD))
        {
            return false;
        }
        let mut counter = Counter {
            bytes: 0,
            limit: MAX_BYTES.saturating_sub(self.bytes),
        };
        if serde_json::to_writer(&mut counter, row).is_err() {
            return false;
        }
        self.bytes += counter.bytes;
        true
    }
}
struct Counter {
    bytes: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(io::Error::other("result budget exceeded"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_large_fields_without_retaining_them() {
        let mut budget = ResultBudget::default();
        assert!(!budget.accept(&[Value::String("x".repeat(MAX_FIELD + 1))]));
        assert!(budget.accept(&[Value::from(42)]));
    }
}
