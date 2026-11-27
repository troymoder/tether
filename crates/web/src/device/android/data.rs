use android::data::Store;
use async_trait::async_trait;

use super::Android;
use crate::views::data::Data;

#[async_trait(?Send)]
impl Data for Android {
    async fn user_apps(&self) -> Result<Vec<String>, String> {
        let listed = android::apps::list(&self.adb().await)
            .await
            .map_err(|error| format!("{error:#}"))?;

        Ok(listed
            .into_iter()
            .filter(|package| !package.system)
            .map(|package| package.id)
            .collect())
    }

    async fn stores(&self, app: &str) -> Result<Vec<Store>, String> {
        android::data::stores(&self.adb().await, self.rooted().await, app)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn query(
        &self,
        app: &str,
        database: &str,
        sql: &str,
    ) -> Result<Vec<Vec<String>>, String> {
        android::data::query(&self.adb().await, self.rooted().await, app, database, sql)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn preferences(&self, app: &str, file: &str) -> Result<Vec<Vec<String>>, String> {
        let found = android::data::preferences(&self.adb().await, self.rooted().await, app, file)
            .await
            .map_err(|error| format!("{error:#}"))?;
        let head = ["Name", "Type", "Value"].map(str::to_owned).to_vec();
        let rows = found
            .into_iter()
            .map(|found| vec![found.name, found.kind, found.value]);

        Ok(std::iter::once(head).chain(rows).collect())
    }
}
