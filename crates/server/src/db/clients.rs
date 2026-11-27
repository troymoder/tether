use anyhow::Result;
use protocol::Client;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{IntoActiveModel, QueryOrder};

use super::{Db, added};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "clients")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub print: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub kind: String,
    pub at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Db {
    pub(crate) async fn clients(&self) -> Result<Vec<Client>> {
        let rows = Entity::find()
            .order_by_asc(added())
            .all(&self.connection)
            .await?;
        let clients = rows.into_iter().map(|row| Client {
            print: row.print,
            name: row.name,
            kind: row.kind,
            at: row.at as u64,
        });

        Ok(clients.collect())
    }

    pub(crate) async fn trust(&self, client: &Client) -> Result<()> {
        let row = Model {
            print: client.print.clone(),
            name: client.name.clone(),
            kind: client.kind.clone(),
            at: client.at as i64,
        };
        let again = OnConflict::column(Column::Print)
            .update_columns([Column::Name, Column::At])
            .to_owned();

        Entity::insert(row.into_active_model())
            .on_conflict(again)
            .exec_without_returning(&self.connection)
            .await?;

        Ok(())
    }

    pub(crate) async fn distrust(&self, print: &str) -> Result<()> {
        Entity::delete_by_id(print).exec(&self.connection).await?;

        Ok(())
    }
}
