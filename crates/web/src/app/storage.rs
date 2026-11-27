//! Every read asks the database, so a view that starts from stored settings
//! reads them in a `LocalResource` and draws once they are there. A database
//! the browser refuses reads as empty and drops writes.

use std::collections::HashMap;

use indexed_db_futures::database::Database;
use indexed_db_futures::prelude::*;
use indexed_db_futures::primitive::TryToJs;
use indexed_db_futures::transaction::TransactionMode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsValue;

pub(crate) const KEYS: &str = "keys";
const PREFERENCES: &str = "preferences";
pub(crate) const CACHE: &str = "cache";
pub(crate) const CACHE_COUNTS: &str = "cache_counts";

/// The version has to go up with each change of tables.
pub(crate) async fn open() -> Option<Database> {
    Database::open("tether")
        .with_version(7u8)
        .with_on_upgrade_needed(|_, db| {
            let made: Vec<String> = db.object_store_names().collect();

            for table in [KEYS, PREFERENCES, CACHE, CACHE_COUNTS] {
                if !made.iter().any(|made| made == table) {
                    db.create_object_store(table).build()?;
                }
            }

            Ok(())
        })
        .await
        .ok()
}

pub(crate) async fn kept(key: &str) -> Option<String> {
    read([key.to_owned()]).await.0.remove(key)
}

pub(crate) async fn read(keys: impl IntoIterator<Item = String>) -> Read {
    let mut found = HashMap::new();
    let Some(db) = open().await else {
        return Read(found);
    };
    let Ok(transaction) = db.transaction(PREFERENCES).build() else {
        return Read(found);
    };
    let Ok(store) = transaction.object_store(PREFERENCES) else {
        return Read(found);
    };

    for key in keys {
        let value: Option<String> = match store.get(&key).primitive() {
            Ok(asked) => asked.await.ok().flatten(),
            Err(_) => None,
        };

        if let Some(value) = value {
            found.insert(key, value);
        }
    }

    Read(found)
}

#[derive(Clone, Default)]
pub(crate) struct Read(HashMap<String, String>);

impl Read {
    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

pub(crate) fn keep(key: &str, value: &str) {
    let key = key.to_owned();
    let value = value.to_owned();

    leptos::task::spawn_local(async move {
        let _ = put(PREFERENCES, key, value).await;
    });
}

pub(crate) fn forget(key: &str) {
    let key = key.to_owned();

    leptos::task::spawn_local(async move {
        let _ = remove(PREFERENCES, key).await;
    });
}

pub(crate) async fn put<K, V>(table: &str, key: K, value: V) -> Option<()>
where
    K: Serialize + DeserializeOwned,
    V: Serialize,
{
    let db = open().await?;
    let transaction = db
        .transaction(table)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .ok()?;

    transaction
        .object_store(table)
        .ok()?
        .put(value)
        .with_key(key)
        .serde()
        .ok()?
        .await
        .ok()?;
    transaction.commit().await.ok()
}

pub(crate) async fn all<V: DeserializeOwned>(table: &str) -> Vec<V> {
    let Some(db) = open().await else {
        return Vec::new();
    };
    let Ok(transaction) = db.transaction(table).build() else {
        return Vec::new();
    };
    let Ok(store) = transaction.object_store(table) else {
        return Vec::new();
    };
    let Ok(asked) = store.get_all::<V>().serde() else {
        return Vec::new();
    };

    match asked.await {
        Ok(rows) => rows.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// The read and the write are one transaction, so counts at the same time
/// all land.
pub(crate) async fn count(table: &str, key: &str) -> Option<()> {
    let db = open().await?;
    let transaction = db
        .transaction(table)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .ok()?;
    let store = transaction.object_store(table).ok()?;
    let now: Option<f64> = store.get(key).primitive().ok()?.await.ok()?;

    store
        .put(now.unwrap_or_default() + 1.0)
        .with_key(key)
        .primitive()
        .ok()?
        .await
        .ok()?;
    transaction.commit().await.ok()
}

pub(crate) async fn counted(table: &str, key: &str) -> f64 {
    let read = async {
        let db = open().await?;
        let transaction = db.transaction(table).build().ok()?;
        let store = transaction.object_store(table).ok()?;
        let value: Option<f64> = store.get(key).primitive().ok()?.await.ok()?;

        value
    };

    read.await.unwrap_or_default()
}

pub(crate) async fn clear(table: &str) -> Option<()> {
    let db = open().await?;
    let transaction = db
        .transaction(table)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .ok()?;

    transaction
        .object_store(table)
        .ok()?
        .clear()
        .ok()?
        .await
        .ok()?;
    transaction.commit().await.ok()
}

pub(crate) async fn remove<K: TryToJs>(table: &str, key: K) -> Option<()> {
    let db = open().await?;
    let transaction = db
        .transaction(table)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .ok()?;

    transaction
        .object_store(table)
        .ok()?
        .delete(key)
        .primitive()
        .ok()?
        .await
        .ok()?;
    transaction.commit().await.ok()
}

/// A `CryptoKeyPair` or bytes.
pub(crate) async fn stored_key(name: &str) -> Option<JsValue> {
    let db = open().await?;
    let transaction = db.transaction(KEYS).build().ok()?;
    let store = transaction.object_store(KEYS).ok()?;

    store.get(name).primitive().ok()?.await.ok()?
}

/// Returns the key that is stored, which is another page's if it made one
/// meanwhile.
pub(crate) async fn store_key(name: &str, made: JsValue) -> Option<JsValue> {
    let db = open().await?;
    let transaction = db
        .transaction(KEYS)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .ok()?;
    let added = transaction
        .object_store(KEYS)
        .ok()?
        .add(made.clone())
        .with_key(name)
        .primitive()
        .ok()?
        .await;

    if added.is_err() {
        return stored_key(name).await;
    }

    transaction.commit().await.ok()?;

    Some(made)
}
