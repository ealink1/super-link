#[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
mod native;
#[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
pub use native::DuckDb;

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
pub struct DuckDb;
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
impl DuckDb {
    pub fn open(_: &superlink_domain::ConnectionConfig) -> Result<Self, String> {
        Err("DuckDB 原生引擎尚不提供 Windows ARM64 构建".into())
    }
    pub fn set_cancellation(&self, _: &superlink_drivers::QueryCancel) {}
    pub fn query(
        &mut self,
        _: &str,
        _: &[serde_json::Value],
        _: u32,
    ) -> Result<superlink_domain::ResultSet, String> {
        Err("此平台不支持 DuckDB".into())
    }
}
