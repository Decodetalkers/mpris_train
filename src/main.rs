use futures_util::StreamExt;
use once_cell::sync::Lazy;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use zbus::proxy;
use zbus::{zvariant::OwnedValue, Connection, Result};

use zbus::zvariant::OwnedObjectPath;

#[allow(unused)]
#[derive(Debug)]
pub struct Metadata {
    mpris_trackid: OwnedObjectPath,
    mpris_arturl: String,
    xesam_title: String,
    xesam_album: String,
    xesam_artist: Vec<String>,
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
    fn metadata(&self) -> Result<HashMap<String, OwnedValue>>;
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
    println!("{names:?}");
    for name in names.iter() {
        let instance = MediaPlayer2DbusProxy::builder(&conn)
            .destination(name.as_str())
            .unwrap()
            .build()
            .await?;

        let mut value = instance.metadata().await?;

        let art_url = value.remove("mpris:artUrl").unwrap();
        let mpris_arturl: String = art_url.try_into().unwrap();

        let trackid = value.remove("mpris:trackid").unwrap();
        let mpris_trackid: OwnedObjectPath = trackid.try_into().unwrap();

        let title = value.remove("xesam:title").unwrap();
        let xesam_title: String = title.try_into().unwrap();

        let artist = value.remove("xesam:artist").unwrap();
        let xesam_artist: Vec<String> = artist.try_into().unwrap();

        let album = value.remove("xesam:album").unwrap();
        let xesam_album: String = album.try_into().unwrap();

        let data = Metadata {
            mpris_trackid,
            xesam_title,
            xesam_artist,
            xesam_album,
            mpris_arturl,
        };
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
