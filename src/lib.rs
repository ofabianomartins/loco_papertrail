// use sea_orm::entity::prelude::*;
// use sea_orm::{ActiveModelBehavior, ActiveValue::NotSet, IntoActiveModel};


// use quote::quote;
// use syn::{parse_macro_input, Attribute, DeriveInput};



// ============================================
// MACRO DEFINITION
// ============================================

use sea_orm::{ ConnectionTrait, DbErr };

pub trait ActiveModelAudition {
    fn before_update_audition<C>(
        &self,
        db: &C,
        insert: bool
    ) -> impl std::future::Future<Output = std::result::Result<(), DbErr>> + Send
    where C: ConnectionTrait;

    fn before_delete_audition<C>(
        &self,
        db: &C
    ) -> impl std::future::Future<Output = std::result::Result<(), DbErr>> + Send
    where C: ConnectionTrait;
}

#[macro_export]
macro_rules! define_audition_method {
    (
        $(#[$meta:meta])*
        $model_namespace:ident,
        {
            version_model: $version_namespace:ident,
            attr_id: $attr_id:ident
        }
    ) => {
        ::paste::paste! {
            use sea_orm::{ ConnectionTrait, DbErr };

            impl ActiveModelAudition for ActiveModel {
                async fn before_update_audition<C>(
                    &self,
                    db: &C,
                    insert: bool
                ) -> std::result::Result<(), DbErr>
                where
                    C: ConnectionTrait,
                {
                    let new_data: Json = serde_json::to_value(&$model_namespace::Model::try_from(self.clone())?).unwrap();

                    let entity_id_val = self.$attr_id.clone().unwrap();


                    if insert {
                        // Log INSERT
                        let active_model = $version_namespace::ActiveModel {
                             id: sea_orm::NotSet,
                             entity_id: sea_orm::Set(entity_id_val),
                             entity_name: sea_orm::Set("$model".to_string()),
                             operation: sea_orm::Set("create".to_string()),
                             old_data: sea_orm::Set(None),
                             new_data: sea_orm::Set(Some(new_data)),
                             created_at: sea_orm::Set(chrono::Utc::now().into()),
                        };
                        
                        let _ = active_model.insert(db).await;
                    } else {
                        // Log UPDATE
                        let old_model = $model_namespace::Entity::find_by_id(entity_id_val)
                                            .one(db)
                                            .await?
                                            .ok_or(DbErr::RecordNotFound("Post not found".to_string()))?;
                        
                        let old_data: Json = serde_json::to_value(&old_model).unwrap();

                        let active_model = $version_namespace::ActiveModel {
                            id: sea_orm::NotSet,
                            entity_id: sea_orm::Set(entity_id_val),
                            entity_name: sea_orm::Set(stringify!($model_namespace).to_string()),
                            operation: sea_orm::Set("update".to_string()),
                            old_data: sea_orm::Set(Some(old_data)),
                            new_data: sea_orm::Set(Some(new_data)),
                            created_at: sea_orm::Set(chrono::Utc::now().into()),
                        };

                        let _ = active_model.insert(db).await;
                    };

                    Ok(())
                }

                async fn before_delete_audition<C>(
                    &self,
                    db: &C
                ) -> std::result::Result<(), DbErr>
                where
                    C: ConnectionTrait,
                {
                    let entity_id_val = self.$attr_id.clone().unwrap();
                    
                    // Fetch the existing record to capture the data before deletion
                    let old_model = $model_namespace::Entity::find_by_id(entity_id_val)
                                        .one(db)
                                        .await?
                                        .ok_or(DbErr::RecordNotFound("Post not found".to_string()))?;
                    
                    let old_data: Json = serde_json::to_value(&old_model).unwrap();
                    
                    // Log DELETE

                    let active_model = $version_namespace::ActiveModel {
                        id: sea_orm::NotSet,
                        entity_id: sea_orm::Set(entity_id_val),
                        entity_name: sea_orm::Set(stringify!($model_namespace).to_string()),
                        operation: sea_orm::Set("delete".to_string()),
                        old_data: sea_orm::Set(Some(old_data)),
                        new_data: sea_orm::Set(None),
                        created_at: sea_orm::Set(chrono::Utc::now().into()),
                    };

                    let _ = active_model.insert(db).await;

                    Ok(())
                }
            }
        }
    };
}

// ============================================
// DEFININDO AS FACTORIES
// ============================================

#[cfg(test)]
mod audition_tests {
    use super::*;
    use sea_orm::{
        ActiveModelTrait,
        Database,
        DatabaseConnection,
        Schema,
        ActiveModelBehavior,
        ConnectionTrait,
        entity::prelude::*
    };
    use uuid::Uuid;
    use loco_factory::define_factory;
    use serde::{ Serialize, Deserialize };

    pub mod versions {
        use super::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
        #[sea_orm(table_name = "versions")] // Dummy table name
        pub struct Model {
            pub created_at: DateTimeWithTimeZone,
            #[sea_orm(primary_key)]
            pub id: i32,
            pub entity_name: String,
            pub entity_id: i32,
            pub operation: String,
            pub old_data: Option<Json>,
            pub new_data: Option<Json>,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod specialties {
        use super::*;
        use serde_json::Value as Json;

        #[derive(Clone, Debug, PartialEq, Default, DeriveEntityModel, Eq, Serialize, Deserialize)]
        #[sea_orm(table_name = "specialties")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i32,
            #[sea_orm(unique)]
            pub uuid: Uuid,
            pub name: String,
            pub description: Option<String>,
            pub is_active: bool,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        define_audition_method!(specialties, {version_model: versions, attr_id: id });

        #[async_trait::async_trait]
        impl ActiveModelBehavior for ActiveModel {
            async fn before_save<C>(self, db: &C, insert: bool) -> std::result::Result<Self, DbErr>
            where
                C: ConnectionTrait,
            {
                self.before_update_audition(db, insert).await?;

                Ok(self)
            }

            async fn before_delete<C>(self, db: &C) -> std::result::Result<Self, DbErr>
            where
                C: ConnectionTrait,
            {
                self.before_delete_audition(db).await?;

                Ok(self)
            }
        }
    }


    /// Setup de banco em memória SQLite
    async fn setup_test_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("Failed to connect to test database");

        let schema = Schema::new(sea_orm::DatabaseBackend::Sqlite);

        let stmt = schema.create_table_from_entity(specialties::Entity);
        db.execute(db.get_database_backend().build(&stmt))
            .await
            .expect("Failed to create specialties table");

        let stmt = schema.create_table_from_entity(versions::Entity);
        db.execute(db.get_database_backend().build(&stmt))
            .await
            .expect("Failed to create specialties table");

        db
    }

    define_factory! {
        /// Cria uma specialty de teste
        specialty => specialties::Model {
            active_model: specialties::ActiveModel,
            fields: {
                id: i32 = 1,
                name: String = "Test Specialty".to_string(),
                description: Option<String> = Some("Test Description".to_string()),
                uuid: Uuid = Uuid::new_v4(),
                is_active: bool = true,
            }
        }
    }


    // ============================================
    // TESTES - BUILDER PATTERN
    // ============================================

    mod create_new_log_on_audition_tests {
        use super::*;
        use sea_orm::IntoActiveModel;

        #[tokio::test]
        async fn test_count_on_creation_of_model() {
            let db = setup_test_db().await;
            let _ = create_specialty(&db).await.unwrap();

            let count_all = versions::Entity::find()
                    .count(&db)
                    .await;

            let count = versions::Entity::find()
                    .filter(versions::Column::Operation.eq("create".to_string()))
                    .count(&db)
                    .await;

            assert_eq!(count_all, Ok(1));
            assert_eq!(count, Ok(1));
        }

        #[tokio::test]
        async fn test_count_on_update_of_model() {
            let db = setup_test_db().await;
            let specialty = create_specialty(&db).await.unwrap();

            let mut active_specialty = specialty.into_active_model();

            active_specialty.name = sea_orm::Set("Specialty2".to_string());
            let _ = active_specialty.update(&db).await;

            let count_all = versions::Entity::find()
                    .count(&db)
                    .await;

            let count_create = versions::Entity::find()
                    .filter(versions::Column::Operation.eq("create".to_string()))
                    .count(&db)
                    .await;

            let count_update = versions::Entity::find()
                    .filter(versions::Column::Operation.eq("update".to_string()))
                    .count(&db)
                    .await;

            assert_eq!(count_all, Ok(2));
            assert_eq!(count_create, Ok(1));
            assert_eq!(count_update, Ok(1));
        }
    }

}
