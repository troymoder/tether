use std::collections::BTreeSet;

use anyhow::Result;
use protocol::{App, Variant};
use sea_orm::entity::prelude::*;
use sea_orm::{
    DatabaseTransaction, IntoActiveModel, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};

use super::{Db, added, apps_auto, devices, variants};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "apps")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub id: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub name: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub icon: Option<String>,
    pub builds: u32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Db {
    pub(crate) async fn apps(&self) -> Result<Vec<App>> {
        Self::apps_in(&self.connection, None).await
    }

    pub(crate) async fn app(&self, id: &str) -> Result<Option<App>> {
        Ok(Self::apps_in(&self.connection, Some(id)).await?.pop())
    }

    /// The single connection runs transactions one at a time, so two changes
    /// never overwrite each other. An app that is not registered starts empty
    /// when `new` is set and fails otherwise.
    pub(crate) async fn change_app<T>(
        &self,
        id: &str,
        new: bool,
        edit: impl FnOnce(&mut App) -> Result<T>,
    ) -> Result<T> {
        let change = self.connection.begin().await?;
        let had = Self::apps_in(&change, Some(id)).await?.pop();
        let exists = had.is_some();
        let mut app = match had {
            Some(app) => app,
            None if new => App {
                id: id.to_owned(),
                name: None,
                icon: None,
                builds: 0,
                variants: Vec::new(),
                auto: Vec::new(),
            },
            None => anyhow::bail!("{id}: no build uploaded"),
        };
        let done = edit(&mut app)?;
        let row = Model {
            id: app.id.clone(),
            name: app.name.clone(),
            icon: app.icon.clone(),
            builds: app.builds,
        }
        .into_active_model();

        // Updated in place so that it keeps its position in the order.
        if exists {
            Entity::update(row.reset_all()).exec(&change).await?;
        } else {
            Entity::insert(row).exec_without_returning(&change).await?;
        }

        save_variants(&change, id, &app).await?;
        save_auto(&change, id, &app).await?;
        change.commit().await?;

        Ok(done)
    }

    pub(crate) async fn forget_app(&self, id: &str) -> Result<App> {
        let change = self.connection.begin().await?;
        let app = Self::apps_in(&change, Some(id))
            .await?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("{id}: no build uploaded"))?;

        // Its variants and auto-update rows cascade.
        Entity::delete_by_id(id.to_owned()).exec(&change).await?;
        change.commit().await?;

        Ok(app)
    }

    async fn apps_in(connection: &impl ConnectionTrait, only: Option<&str>) -> Result<Vec<App>> {
        let rows = Entity::find()
            .apply_if(only, |find, id| find.filter(Column::Id.eq(id)))
            .order_by_asc(added())
            .all(connection)
            .await?;
        let mut apps: Vec<App> = rows
            .into_iter()
            .map(|row| {
                Ok(App {
                    id: row.id,
                    name: row.name,
                    icon: row.icon,
                    builds: row.builds,
                    variants: Vec::new(),
                    auto: Vec::new(),
                })
            })
            .collect::<Result<_>>()?;
        let rows = variants::Entity::find()
            .apply_if(only, |find, id| find.filter(variants::Column::App.eq(id)))
            .order_by_asc(added())
            .all(connection)
            .await?;

        for row in rows {
            if let Some(app) = apps.iter_mut().find(|app| app.id == row.app) {
                app.variants.push(Variant {
                    build: row.build,
                    version: row.version,
                    abis: serde_json::from_str(&row.abis)?,
                    handed_over: row.handed_over as u64,
                    size: row.size as u64,
                    hash: row.hash,
                    file: row.file,
                });
            }
        }

        let rows = apps_auto::Entity::find()
            .apply_if(only, |find, id| find.filter(apps_auto::Column::App.eq(id)))
            .order_by_asc(added())
            .all(connection)
            .await?;

        for row in rows {
            if let Some(app) = apps.iter_mut().find(|app| app.id == row.app)
                && let Ok(device) = row.device.parse()
            {
                app.auto.push(device);
            }
        }

        Ok(apps)
    }
}

async fn save_variants(change: &DatabaseTransaction, id: &str, app: &App) -> Result<()> {
    variants::Entity::delete_many()
        .filter(variants::Column::App.eq(id))
        .exec(change)
        .await?;

    for variant in &app.variants {
        let row = variants::Model {
            app: app.id.clone(),
            build: variant.build,
            version: variant.version.clone(),
            abis: serde_json::to_string(&variant.abis)?,
            handed_over: variant.handed_over as i64,
            size: variant.size as i64,
            hash: variant.hash.clone(),
            file: variant.file.clone(),
        };

        variants::Entity::insert(row.into_active_model())
            .exec_without_returning(change)
            .await?;
    }

    Ok(())
}

/// A device without a row never connected, and auto-update on it is dropped.
async fn save_auto(change: &DatabaseTransaction, id: &str, app: &App) -> Result<()> {
    apps_auto::Entity::delete_many()
        .filter(apps_auto::Column::App.eq(id))
        .exec(change)
        .await?;

    let devices: BTreeSet<String> = devices::Entity::find()
        .select_only()
        .column(devices::Column::Id)
        .into_tuple()
        .all(change)
        .await?
        .into_iter()
        .collect();

    for device in app
        .auto
        .iter()
        .map(ToString::to_string)
        .filter(|device| devices.contains(device))
    {
        let row = apps_auto::Model {
            app: app.id.clone(),
            device,
        };

        apps_auto::Entity::insert(row.into_active_model())
            .on_conflict_do_nothing()
            .exec_without_returning(change)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
pub(super) async fn put(db: &Db, app: &App) {
    db.change_app(&app.id, true, |kept| {
        *kept = app.clone();

        Ok(())
    })
    .await
    .unwrap();
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use protocol::DeviceId;

    use super::*;
    use crate::db::Known;

    #[tokio::test]
    async fn gives_back_what_was_saved() {
        let db = Db::in_memory().await;
        let variant = |build: u32, abis: &[&str]| Variant {
            abis: abis.iter().map(|abi| (*abi).to_owned()).collect(),
            build,
            version: (build != 4).then(|| format!("2.{build}.0")),
            handed_over: 1_795_140_353_472,
            size: 20_454_325,
            hash: "a020".to_owned(),
            file: format!("com.example@{build}.apk"),
        };
        let app = |id: &str, variants: Vec<Variant>| App {
            id: id.to_owned(),
            name: Some("Example".to_owned()),
            icon: Some(format!("{id}.svg")),
            builds: 4,
            auto: vec![DeviceId::Android("39181FDJG005SD".to_owned())],
            variants,
        };
        let id = DeviceId::Android("39181FDJG005SD".to_owned());
        let apps = vec![
            app(
                "org.second",
                vec![variant(3, &["arm64-v8a", "x86_64"]), variant(4, &[])],
            ),
            app("com.first", Vec::new()),
        ];
        let mut phone = Known {
            name: "Phone".to_owned(),
            given: Some("Desk phone".to_owned()),
            last: None,
        };

        db.save_device(&id, &phone).await.unwrap();

        for app in &apps {
            put(&db, app).await;
        }

        assert_eq!(db.apps().await.unwrap(), apps);
        assert_eq!(db.app("com.first").await.unwrap().as_ref(), Some(&apps[1]));

        let mut renamed = apps[0].clone();

        renamed.name = Some("Renamed".to_owned());
        put(&db, &renamed).await;
        assert_eq!(db.apps().await.unwrap(), [renamed.clone(), apps[1].clone()]);
        assert_eq!(db.forget_app("org.second").await.unwrap(), renamed);
        assert_eq!(db.apps().await.unwrap(), apps[1..]);
        assert!(db.change_app("org.gone", false, |_| Ok(())).await.is_err());

        phone.name = "Pixel".to_owned();
        db.save_device(&id, &phone).await.unwrap();
        assert_eq!(
            db.devices().await.unwrap(),
            BTreeMap::from([(id.clone(), phone)])
        );
        assert_eq!(db.apps().await.unwrap(), apps[1..]);

        let mut other = apps[1].clone();

        other.auto = vec![DeviceId::Android("NEVER-SEEN".to_owned())];
        put(&db, &other).await;
        assert!(db.apps().await.unwrap()[0].auto.is_empty());
    }
}
