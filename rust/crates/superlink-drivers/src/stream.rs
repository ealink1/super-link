//! Bounded streaming buffer: a large export never retains previous chunks.
use crate::DriverError;
use serde_json::Value;
pub struct Rows<'a, F: FnMut(&[String], &[Vec<Value>]) -> Result<(), String>> {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    bytes: usize,
    count: u64,
    consume: &'a mut F,
}
impl<'a, F: FnMut(&[String], &[Vec<Value>]) -> Result<(), String>> Rows<'a, F> {
    pub fn new(columns: Vec<String>, consume: &'a mut F) -> Result<Self, DriverError> {
        if columns.is_empty() {
            return Err(DriverError::Malformed("export requires a result set"));
        }
        consume(&columns, &[]).map_err(DriverError::Native)?;
        Ok(Self {
            columns,
            rows: Vec::new(),
            bytes: 0,
            count: 0,
            consume,
        })
    }
    pub fn push(&mut self, row: Vec<Value>) -> Result<(), DriverError> {
        // Each row obeys field/row limits; the limit is not cumulative across the export.
        if !crate::budget::ResultBudget::default().accept(&row) {
            return Err(DriverError::Malformed("stream row exceeds budget"));
        }
        let bytes = serde_json::to_vec(&row)
            .map_err(|_| DriverError::Malformed("stream row"))?
            .len();
        if !self.rows.is_empty() && (self.rows.len() >= 256 || self.bytes + bytes > 4 << 20) {
            self.flush()?;
        }
        self.bytes += bytes;
        self.count += 1;
        self.rows.push(row);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), DriverError> {
        if !self.rows.is_empty() {
            (self.consume)(&self.columns, &self.rows).map_err(DriverError::Native)?;
            self.rows.clear();
            self.bytes = 0;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<u64, DriverError> {
        self.flush()?;
        Ok(self.count)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exports_beyond_materialized_budget_in_bounded_chunks() {
        let mut total = 0;
        let mut largest = 0;
        let mut consume = |_: &[String], rows: &[Vec<Value>]| {
            total += rows.len();
            largest = largest.max(rows.len());
            Ok(())
        };
        let mut stream = Rows::new(vec!["payload".into()], &mut consume).unwrap();
        for _ in 0..10000 {
            stream.push(vec![Value::String("x".repeat(4096))]).unwrap();
        }
        assert_eq!(stream.finish().unwrap(), 10000);
        assert_eq!(total, 10000);
        assert!(largest <= 256);
    }
}
