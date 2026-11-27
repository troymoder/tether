use std::collections::BTreeMap;

use anyhow::Result;
use protocol::{At, ClientId, DeviceId, InstanceId, Last, Means, Way};
use sea_orm::IntoActiveModel;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;

use super::{Db, Known};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "devices")]
pub(super) struct Model {
    /// Its [`DeviceId`] as one line.
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub id: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub given: Option<String>,
    /// `usb`, `wifi` or `emulator`.
    #[sea_orm(column_type = "Text", nullable)]
    pub last_means: Option<String>,
    /// `instance:<print>` or `browser:<print>`.
    #[sea_orm(column_type = "Text", nullable)]
    pub last_place: Option<String>,
    /// Milliseconds since 1970.
    pub last_at: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

fn means_name(means: Means) -> &'static str {
    match means {
        Means::Usb => "usb",
        Means::Wifi => "wifi",
        Means::Emulator => "emulator",
    }
}

fn place_name(at: &At) -> String {
    match at {
        At::Instance(instance) => format!("instance:{}", instance.0),
        At::Browser(client) => format!("browser:{}", client.0),
    }
}

fn last_of(row: &Model) -> Option<Last> {
    let means = match row.last_means.as_deref()? {
        "usb" => Means::Usb,
        "wifi" => Means::Wifi,
        "emulator" => Means::Emulator,
        _ => return None,
    };
    let place = row.last_place.as_deref()?;
    let at = match place.split_once(':')? {
        ("instance", print) => At::Instance(InstanceId(print.to_owned())),
        ("browser", print) => At::Browser(ClientId(print.to_owned())),
        _ => return None,
    };

    Some(Last {
        way: Way { means, at },
        at: row.last_at? as u64,
    })
}

pub(super) fn row(id: &DeviceId, device: &Known) -> Model {
    let last = device.last.as_ref();

    Model {
        id: id.to_string(),
        name: device.name.clone(),
        given: device.given.clone(),
        last_means: last.map(|last| means_name(last.way.means).to_owned()),
        last_place: last.map(|last| place_name(&last.way.at)),
        last_at: last.map(|last| last.at as i64),
    }
}

impl Db {
    pub(crate) async fn devices(&self) -> Result<BTreeMap<DeviceId, Known>> {
        let rows = Entity::find().all(&self.connection).await?;
        let devices = rows.into_iter().filter_map(|row| {
            let id = row.id.parse().ok()?;
            let known = Known {
                last: last_of(&row),
                name: row.name,
                given: row.given,
            };

            Some((id, known))
        });

        Ok(devices.collect())
    }

    /// Its auto-update rows cascade.
    pub(crate) async fn forget_device(&self, id: &DeviceId) -> Result<()> {
        Entity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;

        Ok(())
    }

    pub(crate) async fn save_device(&self, id: &DeviceId, device: &Known) -> Result<()> {
        let known = OnConflict::column(Column::Id)
            .update_columns([
                Column::Name,
                Column::Given,
                Column::LastMeans,
                Column::LastPlace,
                Column::LastAt,
            ])
            .to_owned();

        Entity::insert(row(id, device).into_active_model())
            .on_conflict(known)
            .exec_without_returning(&self.connection)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_the_last_connection() {
        let db = Db::in_memory().await;
        let id = DeviceId::Android("AAA".to_owned());
        let phone = Known {
            name: "Pixel".to_owned(),
            given: None,
            last: Some(Last {
                way: Way {
                    means: Means::Usb,
                    at: At::Browser(ClientId("SHA256:a:b".to_owned())),
                },
                at: 1_795_140_353_472,
            }),
        };

        db.save_device(&id, &phone).await.unwrap();
        assert_eq!(db.devices().await.unwrap()[&id], phone);
    }
}
