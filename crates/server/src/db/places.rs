use anyhow::Result;
use protocol::Place;
use sea_orm::entity::prelude::*;
use sea_orm::{IntoActiveModel, QueryOrder, TransactionTrait};

use super::{Db, added};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "places")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Db {
    pub(crate) async fn places(&self) -> Result<Vec<Place>> {
        let rows = Entity::find()
            .order_by_asc(added())
            .all(&self.connection)
            .await?;
        let places = rows.into_iter().map(|row| Place {
            name: row.name,
            latitude: row.latitude,
            longitude: row.longitude,
        });

        Ok(places.collect())
    }

    pub(crate) async fn save_places(&self, places: &[Place]) -> Result<()> {
        let change = self.connection.begin().await?;

        Entity::delete_many().exec(&change).await?;

        for place in places {
            let row = Model {
                name: place.name.clone(),
                latitude: place.latitude,
                longitude: place.longitude,
            };

            Entity::insert(row.into_active_model())
                .exec_without_returning(&change)
                .await?;
        }

        Ok(change.commit().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gives_back_the_places_that_were_saved() {
        let db = Db::in_memory().await;
        let places = vec![Place {
            name: "Cape Town".to_owned(),
            latitude: -33.9249,
            longitude: 18.4241,
        }];

        db.save_places(&places).await.unwrap();
        assert_eq!(db.places().await.unwrap(), places);
    }
}
