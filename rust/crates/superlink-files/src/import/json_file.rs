//! Streaming JSON import of an array of objects.
//!
//! Columns are the union of every object key, added in sorted order as new keys
//! appear, so rows never need a second pass over the document.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io::Read;

use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use crate::cancel::Cancellation;
use crate::error::{FileError, Result};
use crate::import::{Dataset, validate_columns};
use crate::limits::{MAX_IMPORT_CELLS, MAX_IMPORT_COLUMNS};
use crate::value::Cell;

const CANCELLED: &str = "operation cancelled";

/// Reads the whole array while keeping only the accepted dataset in memory.
pub(crate) fn read(cancel: &dyn Cancellation, reader: impl Read) -> Result<Dataset> {
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let seed = DatasetSeed { cancel };
    let result = seed
        .deserialize(&mut deserializer)
        .and_then(|dataset| deserializer.end().map(|()| dataset));
    match result {
        Ok(dataset) => Ok(dataset),
        Err(error) => {
            if cancel.is_cancelled() {
                return Err(FileError::Cancelled);
            }
            let message = error.to_string();
            if message.contains("trailing characters") {
                return Err(FileError::invalid("unexpected content after JSON array"));
            }
            Err(FileError::invalid(message))
        }
    }
}

struct DatasetSeed<'a> {
    cancel: &'a dyn Cancellation,
}

impl<'de> DeserializeSeed<'de> for DatasetSeed<'_> {
    type Value = Dataset;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Dataset, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(DatasetVisitor {
            cancel: self.cancel,
        })
    }
}

struct DatasetVisitor<'a> {
    cancel: &'a dyn Cancellation,
}

impl<'de> Visitor<'de> for DatasetVisitor<'_> {
    type Value = Dataset;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of objects")
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Dataset, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut data = Dataset::default();
        let mut indices: BTreeMap<String, usize> = BTreeMap::new();
        while let Some(row) = sequence.next_element::<RowObject>()? {
            if self.cancel.is_cancelled() {
                return Err(de::Error::custom(CANCELLED));
            }
            let number = data.rows.len() + 1;
            let mut new_columns: Vec<String> = row
                .0
                .iter()
                .map(|(name, _)| name)
                .filter(|name| !indices.contains_key(*name))
                .cloned()
                .collect();
            new_columns.sort();
            for name in new_columns {
                if (data.columns.len() + 1) * data.rows.len().max(1) > MAX_IMPORT_CELLS {
                    return Err(de::Error::custom("import exceeds 2,000,000 cells"));
                }
                indices.insert(name.clone(), data.columns.len());
                data.columns.push(name);
                for existing in &mut data.rows {
                    existing.push(Cell::Null);
                }
            }
            validate_columns(&data.columns).map_err(de::Error::custom)?;
            let mut values = vec![Cell::Null; data.columns.len()];
            for (name, value) in row.0 {
                if let Some(index) = indices.get(&name) {
                    values[*index] = value;
                }
            }
            data.append_row(values)
                .map_err(|error| de::Error::custom(format!("JSON row {number}: {error}")))?;
        }
        if data.columns.is_empty() {
            return Err(de::Error::custom("JSON array has no fields"));
        }
        Ok(data)
    }
}

struct RowObject(Vec<(String, Cell)>);

impl<'de> Deserialize<'de> for RowObject {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(RowObjectVisitor)
    }
}

struct RowObjectVisitor;

impl<'de> Visitor<'de> for RowObjectVisitor {
    type Value = RowObject;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<RowObject, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries: Vec<(String, Cell)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(name) = map.next_key::<String>()? {
            if !seen.insert(name.clone()) {
                return Err(de::Error::custom("duplicate object field"));
            }
            if entries.len() >= MAX_IMPORT_COLUMNS {
                return Err(de::Error::custom("row exceeds 1024 fields"));
            }
            let value: Value = map.next_value()?;
            entries.push((name, Cell::from(value)));
        }
        Ok(RowObject(entries))
    }
}
