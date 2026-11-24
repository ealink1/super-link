//! Shared metrics for the compact SQL object tree. Each child advances one level.
pub(super) const ROW: f32 = 26.;
pub(super) const FONT: f32 = 12.;
pub(super) const ICON: f32 = 14.;
pub(super) const INDENT: f32 = 16.;
pub(super) const INSET: f32 = 8.;
pub(super) const GAP: f32 = 6.;

pub(super) fn object_indent(database: &str) -> f32 {
    INSET + INDENT * if database.is_empty() { 2. } else { 3. }
}
