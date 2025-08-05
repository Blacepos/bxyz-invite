use std::{
    sync::LazyLock,
    time::{Duration, SystemTime},
};

use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, MutexGuard};

use crate::templates::ManagePageJson;

const EVENT_LIFETIME: Duration = Duration::from_days(30 * 3);
const DB_PATH: &str = "events.db";
const PURGE_PERIOD: Duration = Duration::from_days(1);
const PURGE_RETRY_PERIOD: Duration = Duration::from_mins(1);

static DB_GUARD: Mutex<()> = Mutex::const_new(());
static RNG: LazyLock<Mutex<StdRng>> =
    LazyLock::new(|| Mutex::new(StdRng::from_os_rng()));

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct EventDB {
    pub events: Vec<Event>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub id: u64,
    // Option since it starts unset
    pub name: Option<String>,
    pub attendees: Vec<Attendee>,
    pub created: SystemTime,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Attendee {
    pub id: u64,
    pub name: String,
    pub custom_html: String,
    pub has_accepted: bool,
}

/// Attempt to open the database. This function creates a new database if an
/// existing one could not be read or if the data from the existing database
/// could not be parsed.
///
/// To prevent multiple tasks from reading and writing back the file at the same
/// time, a static lock is acquired before opening the file. The lock guard is
/// then returned out for the caller to drop when they are done with the data.
/// The save_db function consumes the lock as an argument, which it drops after
/// writing.
async fn open_db<'a>() -> Result<(EventDB, MutexGuard<'a, ()>), ()> {
    let lock = DB_GUARD.lock().await;

    let data = match tokio::fs::read(DB_PATH).await {
        Ok(d) => d,
        Err(_) => {
            // if failed, it's probably the first run
            log::info!("Unable to open an existing database. Creating new.");
            let def_struct = EventDB::default();
            let def = serde_cbor::to_vec(&def_struct)
                .expect("Default structure is serializable");
            if tokio::fs::write(DB_PATH, &def).await.is_err() {
                log::error!("Could not create database file");
                return Err(());
            }
            return Ok((def_struct, lock));
        }
    };

    match serde_cbor::from_slice::<EventDB>(&data) {
        Ok(db) => Ok((db, lock)),
        Err(_) => {
            log::warn!(
                "Database is corrupted. Assuming database structure has \
                 changed in the source code. Recreating."
            );
            let def_struct = EventDB::default();
            let def = serde_cbor::to_vec(&def_struct)
                .expect("Default structure is serializable");
            if tokio::fs::write(DB_PATH, &def).await.is_err() {
                log::error!("Could not create database file");
                return Err(());
            }
            Ok((def_struct, lock))
        }
    }
}

/// `db` is moved into the function to prevent caller from accidentally writing
/// again. The assumption is that each public function that interacts with the
/// database is an atomic operation
async fn save_db(db: EventDB, _lock: MutexGuard<'_, ()>) -> Result<(), ()> {
    let d = match serde_cbor::to_vec(&db) {
        Ok(d) => d,
        Err(e) => {
            log::error!(
                "Data could not be serialized: \"{e}\". Should not happen."
            );
            return Err(());
        }
    };

    if tokio::fs::write(DB_PATH, &d).await.is_err() {
        log::error!("Failed to write back database. Data is lost!");
        return Err(());
    };
    Ok(())
}

/// Open the event database and delete entries that are older than the
/// configured lifetime
async fn purge_old_events() -> Result<(), ()> {
    let Ok((mut db, lock)) = open_db().await else {
        log::warn!("Purge task could not open the database");
        return Err(());
    };

    db.events.retain(|ev| {
        let diff = match SystemTime::now().duration_since(ev.created) {
            Ok(d) => d,
            Err(_) => {
                let name = ev.name.clone().unwrap_or("<Untitled>".to_string());
                log::warn!(
                    "Purging event \"{name}\" with creation time after \
                     current time"
                );
                return false;
            }
        };

        diff < EVENT_LIFETIME
    });

    if save_db(db, lock).await.is_err() {
        log::warn!("Purge task could not save database");
        return Err(());
    }
    Ok(())
}

pub async fn create_event() -> Result<u64, String> {
    let (mut db, lock) = open_db()
        .await
        .map_err(|_| "Internal database was inaccessible".to_string())?;

    let ev_id = RNG.lock().await.random();
    let new_event = Event {
        id: ev_id,
        name: None,
        attendees: Vec::new(),
        created: SystemTime::now(),
    };
    db.events.push(new_event);

    save_db(db, lock)
        .await
        .map_err(|_| "Internal database was inaccessible".to_string())?;
    Ok(ev_id)
}

pub enum FindEventError {
    Database(String),
    NotFound(String),
}

pub async fn find_event_by_id(ev_id: u64) -> Result<Event, FindEventError> {
    let (db, _lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    for event in db.events.iter() {
        if event.id == ev_id {
            return Ok(event.clone());
        }
    }

    Err(FindEventError::NotFound(
        "Event with given ID not found in database".to_string(),
    ))
}

pub async fn find_event_by_attendee(
    at_id: u64,
) -> Result<(Event, Attendee), FindEventError> {
    let (db, _lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    for event in db.events.iter() {
        for attendee in event.attendees.iter() {
            if attendee.id == at_id {
                return Ok((event.clone(), attendee.clone()));
            }
        }
    }

    Err(FindEventError::NotFound(
        "Could not find event with the given attendee ID".to_string(),
    ))
}

pub async fn set_accepted(
    at_id: u64,
    accept: bool,
) -> Result<(), FindEventError> {
    let (mut db, lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    for event in db.events.iter_mut() {
        for attendee in event.attendees.iter_mut() {
            if attendee.id == at_id {
                attendee.has_accepted = accept;
            }
        }
    }

    save_db(db, lock).await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;
    Ok(())
}

pub async fn update_event(
    ev_id: u64,
    data: ManagePageJson,
) -> Result<(), FindEventError> {
    let (mut db, lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    for event in db.events.iter_mut() {
        if ev_id == event.id {
            event.name = Some(data.event_name.clone());
            for attendee_db in event.attendees.iter_mut() {
                for (at_id_str, at_update) in data.attendee_data.iter() {
                    let Ok(at_id) = base62::decode(at_id_str) else {
                        continue;
                    };
                    if at_id as u64 == attendee_db.id {
                        attendee_db.custom_html = at_update.custom_html.clone();
                        attendee_db.name = at_update.name.clone();
                    }
                }
            }
        }
    }

    save_db(db, lock).await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;
    Ok(())
}

pub async fn add_attendee(ev_id: u64) -> Result<(), FindEventError> {
    let (mut db, lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    for event in db.events.iter_mut() {
        if ev_id == event.id {
            let at_id = RNG.lock().await.random();
            event.attendees.push(Attendee {
                id: at_id,
                name: "Unnamed".to_string(),
                custom_html: "<html></html>".to_string(),
                has_accepted: false,
            })
        }
    }

    save_db(db, lock).await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;
    Ok(())
}

pub async fn remove_attendee(at_id: u64) -> Result<(), FindEventError> {
    let (mut db, lock) = open_db().await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;

    log::debug!("remove {at_id}");
    for event in db.events.iter_mut() {
        event.attendees.retain(|at| {
            log::debug!("{}", at.id);
            at.id != at_id
        });
    }

    save_db(db, lock).await.map_err(|_| {
        FindEventError::Database(
            "Internal database was inaccessible".to_string(),
        )
    })?;
    Ok(())
}

pub async fn purge_task() {
    loop {
        log::info!("Next purge in {} secs.", PURGE_PERIOD.as_secs());
        tokio::time::sleep(PURGE_PERIOD).await;
        log::info!("Performing scheduled purge of expired events");
        while purge_old_events().await.is_err() {
            log::warn!(
                "Purge failed. Retrying in {} secs.",
                PURGE_RETRY_PERIOD.as_secs()
            );
            tokio::time::sleep(PURGE_RETRY_PERIOD).await;
        }
    }
}

pub async fn setup_test() {
    let (mut db, lock) = open_db().await.unwrap();

    log::info!("Setup");
    let ev_id = base62::decode("test").unwrap() as u64;
    let new_event = Event {
        id: ev_id,
        name: Some("My Event".to_string()),
        attendees: vec![
            Attendee { id: 1234567, name: "Blacepos".to_string(), custom_html: "hi i hope you're doing well. i'm doing alright. hey by the way do you want to hear me ramble a bit? I mean it's not like you have a choice in the matter. I need to write something in order to make this text really long".to_string(), has_accepted: false },
            Attendee { id: 1234568, name: "Blacepos".to_string(), custom_html: "hi i hope you're doing well. i'm doing alright. hey by the way do you want to hear me ramble a bit? I mean it's not like you have a choice in the matter. I need to write something in order to make this text really long".to_string(), has_accepted: false },
            Attendee { id: 1234569, name: "Blacepos".to_string(), custom_html: "hi i hope you're doing well. i'm doing alright. hey by the way do you want to hear me ramble a bit? I mean it's not like you have a choice in the matter. I need to write something in order to make this text really long".to_string(), has_accepted: false },
            Attendee { id: 1234570, name: "Blacepos".to_string(), custom_html: "hi i hope you're doing well. i'm doing alright. hey by the way do you want to hear me ramble a bit? I mean it's not like you have a choice in the matter. I need to write something in order to make this text really long".to_string(), has_accepted: false },
        ],
        created: SystemTime::now(),
    };
    if !db.events.iter().any(|e| e.id == ev_id) {
        db.events.push(new_event);
    }

    save_db(db, lock).await.unwrap();
}
