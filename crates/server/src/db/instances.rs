use anyhow::Result;
use sea_orm::IntoActiveModel;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;

use super::Db;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "instances")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub id: String,
    #[sea_orm(column_type = "Text")]
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Db {
    pub(crate) async fn instance_name(&self, id: &str) -> Result<Option<String>> {
        let row = Entity::find_by_id(id.to_owned())
            .one(&self.connection)
            .await?;

        Ok(row.map(|row| row.name))
    }

    pub(crate) async fn name_instance(&self, id: &str, name: &str) -> Result<()> {
        let row = Model {
            id: id.to_owned(),
            name: name.to_owned(),
        };
        let again = OnConflict::column(Column::Id)
            .update_column(Column::Name)
            .to_owned();

        Entity::insert(row.into_active_model())
            .on_conflict(again)
            .exec_without_returning(&self.connection)
            .await?;

        Ok(())
    }
}
