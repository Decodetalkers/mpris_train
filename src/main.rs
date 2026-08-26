use futures_util::StreamExt;
use once_cell::sync::Lazy;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use zbus::proxy;
use zbus::{Connection, Result};

use serde::{Deserialize, Serialize};
use zbus::zvariant::OwnedObjectPath;
use zbus::zvariant::{
    as_value::{self},
    OwnedValue, Type,
};

#[derive(Deserialize, Serialize, Type, Debug, OwnedValue)]
#[zvariant(signature = "a{sv}")]
pub struct Metadata {
    #[serde(rename = "mpris:trackid", with = "as_value")]
    mpris_trackid: OwnedObjectPath,
    #[serde(rename = "mpris:artUrl", with = "as_value")]
    mpris_arturl: String,
    #[serde(rename = "xesam:title", with = "as_value")]
    xesam_title: String,
    #[serde(rename = "xesam:album", with = "as_value")]
    xesam_album: String,
    #[serde(rename = "xesam:artist", with = "as_value")]
    xesam_artist: Vec<String>,
    #[serde(flatten, with = "as_value")]
    the_rest: HashMap<String, OwnedValue>,
}

static MPIRS_CONNECTIONS: Lazy<Arc<Mutex<Vec<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

async fn get_mpirs_connections() -> Vec<String> {
    let conns = MPIRS_CONNECTIONS.lock().await;
    conns.clone()
}

async fn set_mpirs_connection(list: Vec<String>) {
    let mut conns = MPIRS_CONNECTIONS.lock().await;
    *conns = list;
}

async fn add_mpirs_connection<T: ToString>(conn: T) {
    let mut conns = MPIRS_CONNECTIONS.lock().await;
    conns.push(conn.to_string());
}

async fn remove_mpirs_connection<T: ToString>(conn: T) {
    let mut conns = MPIRS_CONNECTIONS.lock().await;
    conns.retain(|iter| iter != &conn.to_string());
}

#[proxy(
    default_service = "org.freedesktop.DBus",
    interface = "org.freedesktop.DBus",
    default_path = "/org/freedesktop/DBus"
)]
trait FreedestopDBus {
    #[zbus(signal)]
    fn name_owner_changed(
        &self,
        name: String,
        new_owner: String,
        older_owner: String,
    ) -> Result<()>;
    fn list_names(&self) -> Result<Vec<String>>;
}

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer2Dbus {
    #[zbus(property)]
    fn can_pause(&self) -> Result<bool>;

    #[zbus(property)]
    fn metadata(&self) -> Result<Metadata>;
}

#[tokio::main]
async fn main() -> Result<()> {
    let conn = Connection::session().await?;
    let freedesktop = FreedestopDBusProxy::new(&conn).await?;
    let names = freedesktop.list_names().await?;
    let names: Vec<String> = names
        .iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2"))
        .cloned()
        .collect();
    for name in names.iter() {
        let instance = MediaPlayer2DbusProxy::builder(&conn)
            .destination(name.as_str())
            .unwrap()
            .build()
            .await?;

        println!("{name:?}");
        let data = instance.metadata().await?;

        println!("{data:?}");
    }

    set_mpirs_connection(names).await;

    let mut namechangesignal = freedesktop.receive_name_owner_changed().await?;

    while let Some(signal) = namechangesignal.next().await {
        let NameOwnerChangedArgs {
            name: interfacename,
            older_owner: removed,
            new_owner: added,
            ..
        } = signal.args()?;
        if !interfacename.starts_with("org.mpris.MediaPlayer2") {
            continue;
        }
        if removed.is_empty() {
            remove_mpirs_connection(&interfacename).await;
            println!("{interfacename} is removed");
        } else if added.is_empty() {
            add_mpirs_connection(&interfacename).await;
            println!("{interfacename} is added");
            let instance = MediaPlayer2DbusProxy::builder(&conn)
                .destination(interfacename.as_str())
                .unwrap()
                .build()
                .await?;
            println!("{:?}", instance.metadata().await?);
        }
        println!("name: {:?}", get_mpirs_connections().await);
    }
    Ok(())
}
