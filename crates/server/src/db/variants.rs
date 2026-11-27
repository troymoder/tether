use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "variants")]
pub(super) struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Text")]
    pub app: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub build: u32,
    #[sea_orm(column_type = "Text", nullable)]
    pub version: Option<String>,
    /// A JSON list.
    #[sea_orm(column_type = "Text")]
    pub abis: String,
    // SQLite's numbers are signed.
    pub handed_over: i64,
    pub size: i64,
    #[sea_orm(column_type = "Text")]
    pub hash: String,
    #[sea_orm(column_type = "Text")]
    pub file: String,
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
}

impl ActiveModelBehavior for ActiveModel {}
