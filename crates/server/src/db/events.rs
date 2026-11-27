use anyhow::Result;
use protocol::{ArchivedEventQuery, DeviceId, Event, EventPage, own};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{ExprTrait, LikeExpr};
use sea_orm::{Order, QueryOrder, QuerySelect, TransactionTrait};

use super::Db;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "events")]
pub(super) struct Model {
    /// Increases in the order events are added.
    #[sea_orm(primary_key)]
    pub id: i64,
    pub at: i64,
    pub failed: bool,
    #[sea_orm(column_type = "Text")]
    pub what: String,
    #[sea_orm(column_type = "Text")]
    pub message: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub device: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub place: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

fn event(row: Model) -> Event {
    Event {
        id: row.id as u64,
        at: row.at as u64,
        failed: row.failed,
        what: row.what,
        message: row.message,
        device: row.device.and_then(|device| device.parse().ok()),
        place: row.place,
    }
}

fn matching(query: &ArchivedEventQuery) -> Select<Entity> {
    let mut found = Entity::find();

    if let Some(search) = query.search.as_ref() {
        // SQLite's LIKE ignores case. The wildcards are escaped so that the
        // search text is matched literally.
        let sought = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let like = LikeExpr::new(format!("%{sought}%")).escape('\\');

        found = found.filter(Expr::col(Column::Message).like(like));
    }

    if let Some(failed) = query.failed.as_ref() {
        found = found.filter(Column::Failed.eq(*failed));
    }

    let narrowed = [(Column::What, &query.what), (Column::Place, &query.place)];

    for (column, value) in narrowed {
        if let Some(value) = value.as_ref() {
            found = found.filter(column.eq(value.as_str()));
        }
    }

    if let Some(device) = query.device.as_ref() {
        let device = own::<DeviceId>(device);

        found = found.filter(Column::Device.eq(device.to_string()));
    }

    found
}

impl Db {
    pub(crate) async fn newest_event(&self) -> Result<Option<Event>> {
        let row = Entity::find()
            .order_by_desc(Column::Id)
            .one(&self.connection)
            .await?;

        Ok(row.map(event))
    }

    pub(crate) async fn events(&self, query: &ArchivedEventQuery) -> Result<EventPage> {
        let order = if query.oldest_first {
            Order::Asc
        } else {
            Order::Desc
        };
        let most = u64::from(Ord::min(query.most.to_native(), 1000));
        let mut rows = matching(query)
            .order_by(Column::Id, order)
            .limit(most + 1)
            .all(&self.connection)
            .await?;
        let more = rows.len() as u64 > most;
        rows.truncate(most as usize);

        Ok(EventPage {
            events: rows.into_iter().map(event).collect(),
            more,
            whats: self.values(Column::What).await?,
            devices: self
                .values(Column::Device)
                .await?
                .iter()
                .filter_map(|device| device.parse().ok())
                .collect(),
            places: self.values(Column::Place).await?,
        })
    }

    async fn values(&self, column: Column) -> Result<Vec<String>> {
        let values: Vec<Option<String>> = Entity::find()
            .select_only()
            .column(column)
            .distinct()
            .order_by_asc(column)
            .into_tuple()
            .all(&self.connection)
            .await?;

        Ok(values
            .into_iter()
            .flatten()
            .filter(|value| !value.is_empty())
            .collect())
    }

    pub(crate) async fn add_event(&self, event: &Event, most: usize) -> Result<u64> {
        let change = self.connection.begin().await?;
        let row = ActiveModel {
            id: NotSet,
            at: Set(event.at as i64),
            failed: Set(event.failed),
            what: Set(event.what.clone()),
            message: Set(event.message.clone()),
            device: Set(event.device.as_ref().map(DeviceId::to_string)),
            place: Set(event.place.clone()),
        };
        let newest = Entity::insert(row).exec(&change).await?.last_insert_id;

        Entity::delete_many()
            .filter(Column::Id.lte(newest - most as i64))
            .exec(&change)
            .await?;
        change.commit().await?;

        Ok(newest as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_the_newest_events() {
        let db = Db::in_memory().await;
        let event = |id, message: &str| Event {
            id,
            at: 1_795_140_353_472,
            failed: true,
            what: "page".to_owned(),
            message: message.to_owned(),
            device: Some(DeviceId::Android("pixel".to_owned())),
            place: "browser".to_owned(),
        };

        for message in ["first", "second", "third"] {
            db.add_event(&event(0, message), 2).await.unwrap();
        }

        let asked = protocol::encode(&protocol::EventQuery {
            most: 10,
            ..Default::default()
        });
        let query = protocol::access::<protocol::EventQuery>(&asked).unwrap();

        assert_eq!(
            db.events(query).await.unwrap().events,
            [event(3, "third"), event(2, "second")]
        );
        assert_eq!(db.newest_event().await.unwrap(), Some(event(3, "third")));
    }
}
