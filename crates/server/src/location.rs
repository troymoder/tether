//! Emulators of Android before 12 are told their location through the emulator
//! console (`geo fix`), which only this machine reaches. The page sets the
//! location of every other device itself.

use std::sync::{Arc, Mutex};

use anyhow::{Result, ensure};
use protocol::{Device, Place, Topic, Value};

use crate::adb::Adb;
use crate::db::Db;
use crate::topics::Topics;

fn on_earth(latitude: f64, longitude: f64) -> bool {
    (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)
}

/// The console takes the longitude first.
pub(crate) async fn geo_fix(
    adb: &Adb,
    device: &Device,
    latitude: f64,
    longitude: f64,
) -> Result<String> {
    ensure!(
        on_earth(latitude, longitude),
        "invalid coordinates: latitude -90..90, longitude -180..180"
    );

    adb.emu(
        &device.id.serial(),
        &format!("geo fix {longitude:.6} {latitude:.6}"),
    )
    .await?;

    Ok(format!(
        "{} is at {latitude:.5}, {longitude:.5}",
        protocol::mention(&device.id)
    ))
}

pub(crate) struct Locations {
    db: Arc<Db>,
    topics: Arc<Topics>,
    places: Mutex<Vec<Place>>,
}

impl Locations {
    pub(crate) async fn new(db: Arc<Db>, topics: Arc<Topics>) -> Result<Self> {
        Ok(Self {
            places: Mutex::new(db.places().await?),
            db,
            topics,
        })
    }

    async fn save(&self, places: Vec<Place>) -> Result<()> {
        self.db.save_places(&places).await?;
        self.topics.value(&Topic::Places, Value::Places(places));

        Ok(())
    }

    pub(crate) fn places(&self) -> Vec<Place> {
        self.places.lock().unwrap().clone()
    }

    pub(crate) async fn keep(&self, place: Place) -> Result<()> {
        ensure!(!place.name.trim().is_empty(), "name is required");
        ensure!(
            on_earth(place.latitude, place.longitude),
            "invalid coordinates"
        );

        let _writing = self.db.writing().await;
        let places = {
            let mut places = self.places.lock().unwrap();

            places.retain(|kept| kept.name != place.name);
            places.push(place);

            places.clone()
        };

        self.save(places).await
    }

    pub(crate) async fn forget(&self, name: &str) -> Result<()> {
        let _writing = self.db.writing().await;
        let places = {
            let mut places = self.places.lock().unwrap();

            places.retain(|kept| kept.name != name);

            places.clone()
        };

        self.save(places).await
    }
}
