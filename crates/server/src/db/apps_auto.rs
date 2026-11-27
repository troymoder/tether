use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "apps_auto")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub app: String,
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub device: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub(super) enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::App",
        to = "super::apps::Column::Id",
        on_delete = "Cascade"
    )]
    App,
    #[sea_orm(
        belongs_to = "super::devices::Entity",
        from = "Column::Device",
        to = "super::devices::Column::Id",
        on_delete = "Cascade"
    )]
    Device,
}

impl ActiveModelBehavior for ActiveModel {}
