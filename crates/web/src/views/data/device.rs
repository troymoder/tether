use android::data::Store;
use async_trait::async_trait;

use crate::device::unsupported;

#[async_trait(?Send)]
pub(crate) trait Data {
    async fn user_apps(&self) -> Result<Vec<String>, String> {
        Err(unsupported("apps"))
    }

    async fn stores(&self, _app: &str) -> Result<Vec<Store>, String> {
        Err(unsupported("database"))
    }

    /// The first row holds the column names.
    async fn query(
        &self,
        _app: &str,
        _database: &str,
        _sql: &str,
    ) -> Result<Vec<Vec<String>>, String> {
        Err(unsupported("database"))
    }

    /// The first row holds the column names.
    async fn preferences(&self, _app: &str, _file: &str) -> Result<Vec<Vec<String>>, String> {
        Err(unsupported("preferences"))
    }
}
