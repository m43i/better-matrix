//! Register only when a StatusNotifier host can show the icon. Keep checking the host while
//! closed in case the user disables the extension or restarts their panel.

use std::future::Future;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use futures_lite::future::{block_on, race};
use ksni::TrayMethods;
use ksni::menu::StandardItem;
use zbus::{Connection, Proxy, fdo::DBusProxy, names::OwnedBusName, proxy};

use super::{AppHandle, State, TITLE, icon, t};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const PLASMA_SHELL: &str = "org.kde.plasmashell";
const PLASMA_HAS_TRAY: &str = r#"print(panels().concat(desktops()).some(function (container) {
    return container.screen >= 0 && container.widgets("org.kde.plasma.systemtray").length > 0;
}));"#;
const REGISTER_TIMEOUT: Duration = Duration::from_secs(3);
const RPC_TIMEOUT: Duration = Duration::from_secs(1);
const HOST_CHECK_INTERVAL: Duration = Duration::from_secs(2);
const HOST_CHECK_ATTEMPTS: u8 = 5;

enum Availability {
    MissingHost,
    MissingItem,
    Ready,
}

pub struct Tray {
    // Dropped on reopen to stop the worker. The worker closes the channel if there is no tray.
    service: Sender<()>,
}

impl Tray {
    pub fn show(app: AppHandle<State>) -> std::io::Result<Self> {
        let (stop, cancelled) = async_channel::bounded(1);
        let labels = (t!("tray.open"), t!("tray.quit"));
        std::thread::Builder::new().name("system-tray".into()).spawn(move || {
            if let Ok(pixels) = icon() {
                let item = Item { app: app.clone(), labels, pixels };
                block_on(register(item, &cancelled));
            }
            if cancelled.close() {
                app.wake();
            }
        })?;
        Ok(Self { service: stop })
    }

    pub fn unavailable(&self) -> bool {
        self.service.is_closed()
    }
}

struct Item {
    app: AppHandle<State>,
    labels: (&'static str, &'static str),
    pixels: image::RgbaImage,
}

impl Item {
    fn open(&self) {
        if let Some(token) = ksni::current_xdg_activation_token() {
            self.app.activate(token);
        } else {
            self.app.open();
        }
    }
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }

    fn title(&self) -> String {
        TITLE.into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.open();
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut pixels = self.pixels.as_raw().clone();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.rotate_right(1);
        }
        vec![ksni::Icon { width: 32, height: 32, data: pixels }]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            StandardItem {
                label: self.labels.0.into(),
                activate: Box::new(|item: &mut Self| item.open()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.labels.1.into(),
                activate: Box::new(|item: &mut Self| item.app.quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

async fn register(item: Item, cancelled: &Receiver<()>) {
    let Some(Ok(tray)) = race(
        async {
            let _ = cancelled.recv().await;
            None
        },
        timed(REGISTER_TIMEOUT, item.spawn()),
    )
    .await
    else {
        return;
    };
    // ksni checks the host on registration and watches the watcher's bus name. A watcher can
    // keep its name while its last host disappears, so also check the live host property.
    race(
        async {
            let _ = cancelled.recv().await;
        },
        async {
            let Some(Ok(connection)) = timed(RPC_TIMEOUT, Connection::session()).await else { return };
            let Ok(watcher) = host_watcher(&connection).await else { return };
            let Ok(bus) = DBusProxy::new(&connection).await else { return };
            let item_prefix = format!("org.kde.StatusNotifierItem-{}-", std::process::id());
            let mut failures = 0;
            let mut was_available = false;
            while !tray.is_closed() {
                match timed(RPC_TIMEOUT, availability(&watcher, &bus, &item_prefix)).await {
                    Some(Ok(Availability::Ready)) => {
                        failures = 0;
                        was_available = true;
                    }
                    Some(Ok(Availability::MissingHost)) if !was_available => break,
                    // A working panel may briefly go away during a restart. ksni registers
                    // the existing icon again when a new watcher takes over.
                    _ => {
                        failures += 1;
                        if failures == HOST_CHECK_ATTEMPTS {
                            break;
                        }
                    }
                }
                async_io::Timer::after(HOST_CHECK_INTERVAL).await;
            }
        },
    )
    .await;
    let _ = timed(RPC_TIMEOUT, tray.shutdown()).await;
}

async fn availability(watcher: &Proxy<'_>, bus: &DBusProxy<'_>, item_prefix: &str) -> zbus::Result<Availability> {
    if !watcher.get_property::<bool>("IsStatusNotifierHostRegistered").await? {
        return Ok(Availability::MissingHost);
    }
    let names = bus.list_names().await?;
    if !host_available(watcher.connection(), bus, &names).await? {
        return Ok(Availability::MissingHost);
    }
    let registered: Vec<String> = watcher.get_property("RegisteredStatusNotifierItems").await?;
    // Watchers may publish the well-known or unique name, optionally followed by an object path.
    let registered: Vec<_> = registered.iter().map(|item| item.split('/').next().unwrap_or_default()).collect();
    for name in names.into_iter().filter(|name| name.starts_with(item_prefix)) {
        if registered.contains(&name.as_str())
            || registered.contains(&bus.get_name_owner(name.into_inner()).await?.as_str())
        {
            return Ok(Availability::Ready);
        }
    }
    Ok(Availability::MissingItem)
}

async fn host_available(connection: &Connection, bus: &DBusProxy<'_>, names: &[OwnedBusName]) -> zbus::Result<bool> {
    let plasma = if names.iter().any(|name| name.as_str() == PLASMA_SHELL) {
        Some(bus.get_name_owner(PLASMA_SHELL.try_into()?).await?)
    } else {
        None
    };
    let mut plasma_host = false;
    for host in names.iter().filter(|name| {
        name.starts_with("org.kde.StatusNotifierHost-") || name.starts_with("org.freedesktop.StatusNotifierHost-")
    }) {
        if let Some(plasma) = &plasma
            && bus.get_name_owner(host.clone().into_inner()).await? == *plasma
        {
            plasma_host = true;
        } else {
            return Ok(true);
        }
    }
    if plasma_host {
        // Plasma's singleton can keep its host name after the last tray widget is removed.
        // Ask the shell for the presence of a widget on a screen. The query only reads it.
        let shell = Proxy::new(connection, PLASMA_SHELL, "/PlasmaShell", "org.kde.PlasmaShell").await?;
        let result: String = shell.call("evaluateScript", &(PLASMA_HAS_TRAY,)).await?;
        return Ok(result.trim() == "true");
    }
    // Plasma's watcher in KDED reports true even when the panel's tray is gone.
    // Check who owns the watcher so hosts built into a watcher, such as GNOME's, still work.
    for kded in names.iter().filter(|name| matches!(name.as_str(), "org.kde.kded6" | "org.kde.kded5")) {
        if bus.get_name_owner(kded.clone().into_inner()).await? == bus.get_name_owner(WATCHER.try_into()?).await? {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn host_watcher(connection: &Connection) -> zbus::Result<Proxy<'_>> {
    proxy::Builder::new(connection)
        .destination(WATCHER)
        .path("/StatusNotifierWatcher")
        .interface(WATCHER)
        .cache_properties(proxy::CacheProperties::No)
        .build()
        .await
}

async fn timed<T>(timeout: Duration, operation: impl Future<Output = T>) -> Option<T> {
    race(async { Some(operation.await) }, async {
        async_io::Timer::after(timeout).await;
        None
    })
    .await
}
