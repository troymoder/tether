use anyhow::Result;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{IntoActiveModel, QueryOrder, TransactionTrait};

use protocol::{DeviceId, InstanceId};

use super::{Db, Known, added, devices};
use crate::emulator::Saved;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "emulators")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub name: String,
    pub port: u16,
    /// As JSON.
    #[sea_orm(column_type = "Text")]
    pub settings: String,
    #[sea_orm(column_type = "Text")]
    pub device: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {
    #[sea_orm(
        belongs_to = "super::devices::Entity",
        from = "Column::Device",
        to = "super::devices::Column::Id"
    )]
    Device,
}

impl ActiveModelBehavior for ActiveModel {}

impl Db {
    pub(crate) async fn emulators(&self) -> Result<Vec<Saved>> {
        let rows = Entity::find()
            .order_by_asc(added())
            .all(&self.connection)
            .await?;

        rows.into_iter()
            .map(|row| {
                Ok(Saved {
                    name: row.name,
                    port: row.port,
                    settings: serde_json::from_str(&row.settings)?,
                })
            })
            .collect()
    }

    /// Each emulator owns the row of its device, which is added with it and
    /// deleted with it along with what is kept for that device.
    pub(crate) async fn save_emulators(
        &self,
        emulators: &[Saved],
        instance: &InstanceId,
    ) -> Result<()> {
        let change = self.connection.begin().await?;
        let rows = Entity::find().all(&change).await?;
        let gone = rows
            .into_iter()
            .filter(|row| emulators.iter().all(|emulator| emulator.name != row.name));

        for row in gone {
            Entity::delete_by_id(row.name).exec(&change).await?;
            devices::Entity::delete_by_id(row.device)
                .exec(&change)
                .await?;
        }

        for emulator in emulators {
            let id = DeviceId::Emulator {
                port: emulator.port,
                instance: instance.clone(),
            };
            let known = Known {
                name: emulator.name.clone(),
                given: None,
                last: None,
            };
            let device = id.to_string();

            devices::Entity::insert(devices::row(&id, &known).into_active_model())
                .on_conflict_do_nothing()
                .exec_without_returning(&change)
                .await?;

            let row = Model {
                name: emulator.name.clone(),
                port: emulator.port,
                settings: serde_json::to_string(&emulator.settings)?,
                device,
            };
            let saved = OnConflict::column(Column::Name)
                .update_columns([Column::Port, Column::Settings, Column::Device])
                .to_owned();

            Entity::insert(row.into_active_model())
                .on_conflict(saved)
                .exec_without_returning(&change)
                .await?;
        }

        Ok(change.commit().await?)
    }
}

#[cfg(test)]
mod tests {
    use protocol::App;

    use super::*;
    use crate::db::apps::put;

    fn emulator(name: &str, port: u16) -> Saved {
        let settings = r#"{"profile": "pixel_6", "image": "system-images;android-36;google_apis;x86_64",
            "memory": 2048, "graphics": "Software", "autostart": false, "camera": null}"#;

        Saved {
            name: name.to_owned(),
            port,
            settings: serde_json::from_str(settings).unwrap(),
        }
    }

    async fn names(db: &Db) -> Vec<String> {
        let emulators = db.emulators().await.unwrap();

        emulators
            .into_iter()
            .map(|emulator| emulator.name)
            .collect()
    }

    #[tokio::test]
    async fn an_emulator_has_the_row_of_its_device() {
        let db = Db::in_memory().await;
        let here = InstanceId("SHA256:here".to_owned());
        let id = |port| DeviceId::Emulator {
            port,
            instance: here.clone(),
        };

        db.save_emulators(&[emulator("pixel", 5554), emulator("tablet", 5556)], &here)
            .await
            .unwrap();
        assert_eq!(names(&db).await, ["pixel", "tablet"]);
        let devices: Vec<String> = Entity::find()
            .order_by_asc(added())
            .all(&db.connection)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.device)
            .collect();

        assert_eq!(
            devices,
            ["emulator:5554@SHA256:here", "emulator:5556@SHA256:here"]
        );

        assert_eq!(db.devices().await.unwrap()[&id(5554)].name, "pixel");

        let app = App {
            id: "com.example".to_owned(),
            name: None,
            icon: None,
            builds: 0,
            variants: Vec::new(),
            auto: vec![id(5554)],
        };

        put(&db, &app).await;
        db.save_emulators(&[emulator("pixel", 5554), emulator("tablet", 5556)], &here)
            .await
            .unwrap();
        assert_eq!(db.apps().await.unwrap(), [app]);

        db.save_emulators(&[emulator("tablet", 5556)], &here)
            .await
            .unwrap();
        assert_eq!(names(&db).await, ["tablet"]);
        let ids: Vec<DeviceId> = db.devices().await.unwrap().into_keys().collect();

        assert_eq!(ids, [id(5556)]);
        assert!(db.apps().await.unwrap()[0].auto.is_empty());
    }
}
