#[cfg(feature = "data")]
use crate::data::apply_menu_diffs;
use crate::data::TrayItemMap;
use crate::dbus::dbus_menu_proxy::{DBusMenuProxy, PropertiesUpdate};
use crate::dbus::notifier_item_proxy::StatusNotifierItemProxy;
use crate::dbus::notifier_watcher_proxy::StatusNotifierWatcherProxy;
use crate::dbus::status_notifier_watcher::StatusNotifierWatcher;
use crate::dbus::{self, OwnedValueExt};
use crate::error::{Error, Result};
use crate::item::{self, IconPixmap, Status, StatusNotifierItem, Tooltip};
use crate::menu::{MenuDiff, TrayMenu};
use crate::names;
use dbus::DBusProps;
use futures_lite::{Stream, StreamExt};
use std::future::Future;
use std::result;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::spawn;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, trace, warn};
use zbus::fdo::{DBusProxy, PropertiesProxy};
use zbus::names::InterfaceName;
use zbus::zvariant::{Array, Structure, Value};
use zbus::{Connection, Message};

/// An event emitted by the client
/// representing a change from either the `StatusNotifierItem`
/// or `DBusMenu` protocols.
#[derive(Debug, Clone)]
pub enum Event {
    /// A new `StatusNotifierItem` was added.
    Add(String, Box<StatusNotifierItem>),
    /// An update was received for an existing `StatusNotifierItem`.
    /// This could be either an update to the item itself,
    /// or an update to the associated menu.
    Update(String, UpdateEvent),
    /// A `StatusNotifierItem` was unregistered.
    Remove(String),
}

/// The specific change associated with an update event.
#[derive(Debug, Clone)]
pub enum UpdateEvent {
    AttentionIcon {
        icon_name: Option<String>,
        icon_pixmap: Option<Vec<IconPixmap>>,
    },
    Icon {
        icon_name: Option<String>,
        icon_pixmap: Option<Vec<IconPixmap>>,
    },
    OverlayIcon(Option<String>),
    Status(Status),
    Title(Option<String>),
    Tooltip(Option<Tooltip>),
    /// A menu layout has changed.
    /// The entire layout is sent.
    Menu(TrayMenu),
    /// One or more menu properties have changed.
    /// Only the updated properties are sent.
    MenuDiff(Vec<MenuDiff>),
    /// A new menu has connected to the item.
    /// Its name on bus is sent.
    MenuConnect(String),
}

/// A request to 'activate' one of the menu items,
/// typically sent when it is clicked.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ActivateRequest {
    /// Submenu ID
    MenuItem {
        address: String,
        menu_path: String,
        submenu_id: i32,
    },
    /// Default activation for the tray.
    /// The parameter(x and y) represents screen coordinates and is to be considered an hint to the item where to show eventual windows (if any).
    Default { address: String, x: i32, y: i32 },
    /// Secondary activation(less important) for the tray.
    /// The parameter(x and y) represents screen coordinates and is to be considered an hint to the item where to show eventual windows (if any).
    Secondary { address: String, x: i32, y: i32 },
}

const PROPERTIES_INTERFACE: &str = "org.kde.StatusNotifierItem";
const ITEM_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(50),
    Duration::from_millis(200),
    Duration::from_millis(750),
];

async fn retry_with_delays<T, E, I, F, Fut, N>(
    delays: I,
    mut operation: F,
    mut on_retry: N,
) -> std::result::Result<T, E>
where
    I: IntoIterator<Item = Duration>,
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, E>>,
    N: FnMut(&E, Duration),
{
    let mut delays = delays.into_iter();

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => match delays.next() {
                Some(delay) => {
                    on_retry(&error, delay);
                    sleep(delay).await;
                }
                None => return Err(error),
            },
        }
    }
}

async fn process_stream<S, T, E, F, Fut, N>(mut stream: S, mut operation: F, mut on_error: N)
where
    S: Stream<Item = T> + Unpin,
    F: FnMut(T) -> Fut,
    Fut: Future<Output = std::result::Result<(), E>>,
    N: FnMut(E),
{
    while let Some(item) = stream.next().await {
        if let Err(error) = operation(item).await {
            on_error(error);
        }
    }
}

/// Client for watching the tray.
#[derive(Debug)]
pub struct Client {
    tx: broadcast::Sender<Event>,
    _rx: broadcast::Receiver<Event>,
    connection: Connection,

    #[cfg(feature = "data")]
    items: TrayItemMap,
}

impl Client {
    /// Creates and initializes the client.
    ///
    /// The client will begin listening to items and menus and sending events immediately.
    /// It is recommended that consumers immediately follow the call to `new` with a `subscribe` call,
    /// then immediately follow that with a call to `items` to get the state to not miss any events.
    ///
    /// The value of `service_name` must be unique on the session bus.
    /// It is recommended to use something similar to the format of `appid-numid`,
    /// where `numid` is a short-ish random integer.
    ///
    /// # Errors
    ///
    /// If the initialization fails for any reason,
    /// for example if unable to connect to the bus,
    /// this method will return an error.
    ///
    /// # Panics
    ///
    /// If the generated well-known name is invalid, the library will panic
    /// as this indicates a major bug.
    ///
    /// Likewise, the spawned tasks may panic if they cannot get a `Mutex` lock.
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        let (tx, rx) = broadcast::channel(32);

        // first start server...
        StatusNotifierWatcher::new().attach_to(&connection).await?;

        // ...then connect to it
        let watcher_proxy = StatusNotifierWatcherProxy::new(&connection).await?;

        // register a host on the watcher to declare we want to watch items
        // get a well-known name
        let pid = std::process::id();
        let mut i = 0;
        let wellknown = loop {
            use zbus::fdo::RequestNameReply::{AlreadyOwner, Exists, InQueue, PrimaryOwner};

            i += 1;
            let wellknown = format!("org.freedesktop.StatusNotifierHost-{pid}-{i}");
            let wellknown: zbus::names::WellKnownName = wellknown
                .try_into()
                .expect("generated well-known name is invalid");

            let flags = [zbus::fdo::RequestNameFlags::DoNotQueue];
            match connection
                .request_name_with_flags(&wellknown, flags.into_iter().collect())
                .await?
            {
                PrimaryOwner => break wellknown,
                Exists | AlreadyOwner => {}
                InQueue => unreachable!(
                    "request_name_with_flags returned InQueue even though we specified DoNotQueue"
                ),
            };
        };

        debug!("wellknown: {wellknown}");
        watcher_proxy
            .register_status_notifier_host(&wellknown)
            .await?;
        let items = TrayItemMap::new();

        // handle new items
        {
            let connection = connection.clone();
            let tx = tx.clone();
            let items = items.clone();

            let stream = watcher_proxy
                .receive_status_notifier_item_registered()
                .await?;

            spawn(async move {
                process_stream(
                    stream,
                    |item| {
                        let connection = connection.clone();
                        let tx = tx.clone();
                        let items = items.clone();

                        async move {
                            let args = item.args().map_err(|error| (None, Error::from(error)))?;
                            let address = args.service;

                            debug!("received new item: {address}");
                            Self::handle_item(address, connection, tx, items)
                                .await
                                .map_err(|error| (Some(address.to_string()), error))
                        }
                    },
                    |(address, error)| match address {
                        Some(address) => {
                            error!("failed to initialize tray item {address}: {error}");
                        }
                        None => error!("failed to parse tray item registration: {error}"),
                    },
                )
                .await;

                Ok::<(), Error>(())
            });
        }

        // then lastly get all items
        // it can take so long to fetch all items that we have to do this last,
        // otherwise some incoming items get missed
        {
            let connection = connection.clone();
            let tx = tx.clone();
            let items = items.clone();

            spawn(async move {
                let initial_items = watcher_proxy.registered_status_notifier_items().await?;
                debug!("initial items: {initial_items:?}");

                for item in initial_items {
                    if let Err(err) =
                        Self::handle_item(&item, connection.clone(), tx.clone(), items.clone())
                            .await
                    {
                        error!("failed to initialize tray item {item}: {err}");
                    }
                }

                Ok::<(), Error>(())
            });
        }

        // Handle other watchers unregistering and this one taking over
        // It is necessary to clear all items as our watcher will then re-send them all
        {
            let tx = tx.clone();
            let items = items.clone();
            let connection = connection.clone();
            let host_name = wellknown.to_string();

            let dbus_proxy = DBusProxy::new(&connection).await?;

            let mut stream = dbus_proxy.receive_name_acquired().await?;

            spawn(async move {
                while let Some(thing) = stream.next().await {
                    let body = thing.args()?;
                    if body.name == names::WATCHER_BUS {
                        for dest in items.clear_items() {
                            tx.send(Event::Remove(dest))?;
                        }
                        let watcher = StatusNotifierWatcherProxy::new(&connection).await?;
                        watcher.register_status_notifier_host(&host_name).await?;
                        for item in watcher.registered_status_notifier_items().await? {
                            if let Err(error) = Client::handle_item(
                                &item,
                                connection.clone(),
                                tx.clone(),
                                items.clone(),
                            )
                            .await
                            {
                                error!("failed to resnapshot tray item {item} after watcher acquisition: {error}");
                            }
                        }
                    }
                }

                Ok::<(), Error>(())
            });
        }

        debug!("tray client initialized");

        Ok(Self {
            connection,
            tx,
            _rx: rx,
            #[cfg(feature = "data")]
            items,
        })
    }

    /// Processes an incoming item to send the initial add event,
    /// then set up listeners for it and its menu.
    async fn handle_item(
        address: &str,
        connection: Connection,
        tx: broadcast::Sender<Event>,
        items: TrayItemMap,
    ) -> Result<()> {
        let (destination, path) = parse_address(address);
        let item_id = format!("{destination}{path}");

        let properties_proxy = PropertiesProxy::builder(&connection)
            .destination(destination.to_string())?
            .path(path.clone())?
            .build()
            .await?;

        let properties = Self::get_item_properties(destination, &path, &properties_proxy).await?;

        if !items.new_item(item_id.clone(), &properties) {
            return Ok(());
        }

        tx.send(Event::Add(item_id.clone(), properties.clone().into()))?;

        {
            let connection = connection.clone();
            let destination = destination.to_string();
            let item_id = item_id.clone();
            let items = items.clone();
            let tx = tx.clone();

            spawn(async move {
                Self::watch_item_properties(
                    &item_id,
                    &destination,
                    &path,
                    &connection,
                    properties_proxy,
                    tx,
                    items,
                )
                .await?;

                debug!("Stopped watching {item_id}");
                Ok::<(), Error>(())
            });
        }

        if let Some(menu) = properties.menu {
            let destination = destination.to_string();

            tx.send(Event::Update(
                item_id.clone(),
                UpdateEvent::MenuConnect(menu.clone()),
            ))?;

            spawn(async move {
                Self::watch_menu(item_id, destination, &menu, &connection, tx, items).await?;
                Ok::<(), Error>(())
            });
        }

        Ok(())
    }

    /// Gets the properties for an SNI item.
    async fn get_item_properties(
        destination: &str,
        path: &str,
        properties_proxy: &PropertiesProxy<'_>,
    ) -> Result<StatusNotifierItem> {
        let properties = retry_with_delays(
            ITEM_RETRY_DELAYS,
            || {
                properties_proxy.get_all(
                    InterfaceName::from_static_str(PROPERTIES_INTERFACE)
                        .expect("to be valid interface name"),
                )
            },
            |error, delay| {
                warn!(
                    "failed to read tray item {destination}{path}: {error}; retrying in {} ms",
                    delay.as_millis()
                );
            },
        )
        .await?;

        StatusNotifierItem::try_from(DBusProps(properties))
    }

    /// Watches an SNI item's properties,
    /// sending an update event whenever they change.
    async fn watch_item_properties(
        item_id: &str,
        destination: &str,
        path: &str,
        connection: &Connection,
        properties_proxy: PropertiesProxy<'_>,
        tx: broadcast::Sender<Event>,
        items: TrayItemMap,
    ) -> Result<()> {
        let notifier_item_proxy = StatusNotifierItemProxy::builder(connection)
            .destination(destination)?
            .path(path)?
            .build()
            .await?;

        let dbus_proxy = DBusProxy::new(connection).await?;

        let mut disconnect_stream = dbus_proxy.receive_name_owner_changed().await?;
        let mut props_changed = notifier_item_proxy.inner().receive_all_signals().await?;
        // Subscribe before checking, so an already-exited owner cannot strand this task.
        if !dbus_proxy
            .name_has_owner(destination.try_into().map_err(zbus::Error::from)?)
            .await?
        {
            items.remove_item(item_id);
            tx.send(Event::Remove(item_id.to_string()))?;
            return Ok(());
        }

        loop {
            tokio::select! {
                Some(change) = props_changed.next() => {
                    match Self::get_update_event(change, &properties_proxy).await {
                        Ok(Some(event)) => {
                            debug!("[{destination}{path}] received property change: {event:?}");

                            cfg_if::cfg_if! {
                                if #[cfg(feature = "data")] {
                                    items.apply_update_event(item_id, &event);
                                }
                            }

                            tx.send(Event::Update(item_id.to_string(), event))?;
                        }
                        Err(e) => {
                            error!("Error parsing update properties from {destination}{path}: {e:?}");
                        }
                        _ => {}
                    }
                }
                Some(signal) = disconnect_stream.next() => {
                    let args = signal.args()?;
                    let old = args.old_owner();
                    let new = args.new_owner();

                    if let (Some(old), None) = (old.as_ref(), new.as_ref()) {
                        if old == destination {
                            debug!("[{destination}{path}] disconnected");

                            let watcher_proxy = StatusNotifierWatcherProxy::new(connection)
                                .await
                                .expect("Failed to open StatusNotifierWatcherProxy");

                            if let Err(error) = watcher_proxy.unregister_status_notifier_item(item_id).await {
                                error!("{error:?}");
                            }


                            items.remove_item(item_id);

                            tx.send(Event::Remove(item_id.to_string()))?;
                            break Ok(());
                        }
                    }
                }
            }
        }
    }

    /// Gets the update event for a `DBus` properties change message.
    async fn get_update_event(
        change: Message,
        properties_proxy: &PropertiesProxy<'_>,
    ) -> Result<Option<UpdateEvent>> {
        use UpdateEvent::{AttentionIcon, Icon, OverlayIcon, Status, Title, Tooltip};

        let header = change.header();
        let member = header
            .member()
            .ok_or(Error::InvalidData("Update message header missing `member`"))?;

        macro_rules! get_property {
            ($name:expr) => {
                match properties_proxy
                    .get(
                        InterfaceName::from_static_str(PROPERTIES_INTERFACE)
                            .expect("to be valid interface name"),
                        $name,
                    )
                    .await
                {
                    Ok(v) => Ok(Some(v)),
                    Err(e) => match e {
                        // Some properties may not be set, and this error will be raised.
                        zbus::fdo::Error::InvalidArgs(_) => {
                            warn!("{e}");
                            Ok(None)
                        }
                        _ => Err(Into::<Error>::into(e)),
                    },
                }
            };
        }

        let property = match member.as_str() {
            "NewAttentionIcon" => {
                let icon_name = get_property!("AttentionIconName")?
                    .as_ref()
                    .map(OwnedValueExt::to_string)
                    .transpose()?;
                let icon_pixmap = get_property!("AttentionIconPixmap")?
                    .as_deref()
                    .map(Value::downcast_ref::<&Array>)
                    .and_then(result::Result::ok)
                    .map(IconPixmap::from_array)
                    .and_then(result::Result::ok);
                Some(AttentionIcon {
                    icon_name,
                    icon_pixmap,
                })
            }
            "NewIcon" => {
                let icon_name = get_property!("IconName")
                    .unwrap_or_else(|e| {
                        warn!("Error getting IconName: {e:?}");
                        None
                    })
                    .as_ref()
                    .map(OwnedValueExt::to_string)
                    .transpose()
                    .ok()
                    .flatten();

                let icon_pixmap = get_property!("IconPixmap")
                    .unwrap_or_else(|e| {
                        warn!("Error getting IconPixmap: {e:?}");
                        None
                    })
                    .as_deref()
                    .map(Value::downcast_ref::<&Array>)
                    .and_then(result::Result::ok)
                    .map(IconPixmap::from_array)
                    .and_then(result::Result::ok);

                Some(Icon {
                    icon_name,
                    icon_pixmap,
                })
            }
            "NewOverlayIcon" => Some(OverlayIcon(
                get_property!("OverlayIconName")?
                    .as_ref()
                    .map(OwnedValueExt::to_string)
                    .transpose()?,
            )),
            "NewStatus" => Some(Status(
                get_property!("Status")?
                    .as_deref()
                    .map(Value::downcast_ref::<&str>)
                    .transpose()?
                    .map(item::Status::from)
                    .unwrap_or_default(), // NOTE: i'm assuming status is always set
            )),
            "NewTitle" => Some(Title(
                get_property!("Title")?
                    .as_ref()
                    .map(OwnedValueExt::to_string)
                    .transpose()?,
            )),
            "NewToolTip" => Some(Tooltip(
                get_property!("ToolTip")?
                    .as_deref()
                    .map(Value::downcast_ref::<&Structure>)
                    .transpose()?
                    .map(crate::item::Tooltip::try_from)
                    .transpose()?,
            )),
            _ => {
                warn!("received unhandled update event: {member}");
                None
            }
        };

        debug!("received tray item update: {member} -> {property:?}");

        Ok(property)
    }

    /// Watches the `DBusMenu` associated with an SNI item.
    ///
    /// This gets the initial menu, sending an update event immediately.
    /// Update events are then sent for any further updates
    /// until the item is removed.
    async fn watch_menu(
        item_id: String,
        destination: String,
        menu_path: &str,
        connection: &Connection,
        tx: broadcast::Sender<Event>,
        items: TrayItemMap,
    ) -> Result<()> {
        let dbus_menu_proxy = DBusMenuProxy::builder(connection)
            .destination(destination.as_str())?
            .path(menu_path)?
            .build()
            .await?;

        // Bind menu lifetime to its owner, just like watch_item_properties.
        // A signal stream does not end when its remote application quits.
        let dbus_proxy = DBusProxy::new(connection).await?;
        let mut disconnected = dbus_proxy.receive_name_owner_changed().await?;
        let mut layout_updated = dbus_menu_proxy.receive_layout_updated().await?;
        let mut properties_updated = dbus_menu_proxy.receive_items_properties_updated().await?;
        if !dbus_proxy
            .name_has_owner(destination.as_str().try_into().map_err(zbus::Error::from)?)
            .await?
        {
            return Ok(());
        }

        debug!("[{destination}{menu_path}] getting initial menu");
        let menu = dbus_menu_proxy.get_layout(0, -1, &[]).await?;
        let menu = TrayMenu::try_from(menu)?;

        items.update_menu(&item_id, &menu);

        tx.send(Event::Update(item_id.clone(), UpdateEvent::Menu(menu)))?;

        loop {
            tokio::select!(
                Some(signal) = disconnected.next() => {
                    let args = signal.args()?;
                    if args.old_owner().as_ref().is_some_and(|old| old.as_str() == destination.as_str())
                        && args.new_owner().is_none() {
                        break;
                    }
                }
                _ = connection.closed() => break,

                Some(ev) = layout_updated.next() => {
                    trace!("received layout update");
                    let _parent = ev.args()?.parent;
                    debug!("[{destination}{menu_path}] layout update");

                    // The retained model is a full menu tree. A nonzero parent
                    // response would replace that tree with a subtree and drop siblings.
                    let get_layout = dbus_menu_proxy.get_layout(0, -1, &[]);

                    let menu = match timeout(Duration::from_secs(1), get_layout).await {
                        Ok(Ok(menu)) => {
                            debug!("got new menu layout");
                            menu
                        }
                        Ok(Err(err)) => {
                            error!("error fetching layout: {err:?}");
                            break;
                        }
                        Err(_) => {
                            error!("Timeout getting layout");
                            break;
                        }
                    };

                    let menu = TrayMenu::try_from(menu)?;

                    items.update_menu(&item_id, &menu);

                    debug!("sending new menu for '{item_id}'");
                    trace!("new menu for '{destination}': {menu:?}");
                    tx.send(Event::Update(
                        item_id.clone(),
                        UpdateEvent::Menu(menu),
                    ))?;
                }
                Some(change) = properties_updated.next() => {
                    let body = change.message().body();
                    let update: PropertiesUpdate= body.deserialize::<PropertiesUpdate>()?;
                    let diffs = Vec::try_from(update)?;

                    #[cfg(feature = "data")]
                    if let Some((_, Some(menu))) = items
                        .get_map()
                        .lock()
                        .expect("mutex lock should succeed")
                        .get_mut(&item_id)
                    {
                        apply_menu_diffs(menu, &diffs);
                    } else {
                        error!("could not find item in state");
                    }

                    tx.send(Event::Update(
                        item_id.clone(),
                        UpdateEvent::MenuDiff(diffs),
                    ))?;

                    // FIXME: Menu cache gonna be out of sync
                }
            );
        }

        Ok(())
    }

    async fn get_notifier_item_proxy(
        &self,
        address: String,
    ) -> Result<StatusNotifierItemProxy<'_>> {
        let (destination, path) = parse_address(&address);
        let proxy = StatusNotifierItemProxy::builder(&self.connection)
            .destination(destination.to_owned())?
            .path(path)?
            .build()
            .await?;
        Ok(proxy)
    }

    async fn get_menu_proxy(
        &self,
        address: String,
        menu_path: String,
    ) -> Result<DBusMenuProxy<'_>> {
        let (destination, _) = parse_address(&address);
        let proxy = DBusMenuProxy::builder(&self.connection)
            .destination(destination.to_owned())?
            .path(menu_path)?
            .build()
            .await?;

        Ok(proxy)
    }

    /// Subscribes to the events broadcast channel,
    /// returning a new receiver.
    ///
    /// Once the client is dropped, the receiver will close.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Gets all current items, including their menus if present.
    #[cfg(feature = "data")]
    #[must_use]
    pub fn items(&self) -> std::sync::Arc<std::sync::Mutex<crate::data::BaseMap>> {
        self.items.get_map()
    }

    /// Owned watcher handles, including completed tasks awaiting reaping.
    pub async fn retained_watcher_tasks(&self) -> Result<usize> {
        let watcher = self
            .connection
            .object_server()
            .interface::<_, StatusNotifierWatcher>(names::WATCHER_OBJECT)
            .await?;
        let count = watcher.get().await.owned_task_count();
        Ok(count)
    }

    /// One should call this method with id=0 when opening the root menu.
    ///
    /// ID refers to the menuitem id.
    /// Returns `needsUpdate`
    ///
    /// # Errors
    ///
    /// Errors if the proxy cannot be created.
    pub async fn about_to_show_menuitem(
        &self,
        address: String,
        menu_path: String,
        id: i32,
    ) -> Result<bool> {
        let proxy = self.get_menu_proxy(address.clone(), menu_path).await?;
        let needs_update = proxy.about_to_show(id).await?;
        if needs_update {
            let layout = proxy.get_layout(0, -1, &[]).await?;
            let menu = TrayMenu::try_from(layout)?;
            self.items.update_menu(&address, &menu);
            self.tx
                .send(Event::Update(address, UpdateEvent::Menu(menu)))?;
        }
        Ok(needs_update)
    }

    /// Sends an activate request for a menu item.
    ///
    /// # Errors
    ///
    /// The method will return an error if the connection to the `DBus` object fails,
    /// or if sending the event fails for any reason.
    ///
    /// # Panics
    ///
    /// If the system time is somehow before the Unix epoch.
    pub async fn activate(&self, req: ActivateRequest) -> Result<()> {
        macro_rules! timeout_event {
            ($event:expr) => {
                if timeout(Duration::from_secs(1), $event).await.is_err() {
                    error!("Timed out sending activate event");
                }
            };
        }
        match req {
            ActivateRequest::MenuItem {
                address,
                menu_path,
                submenu_id,
            } => {
                let proxy = self.get_menu_proxy(address, menu_path).await?;
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time should flow forwards");

                let event = proxy.event(
                    submenu_id,
                    "clicked",
                    &Value::I32(0),
                    timestamp.as_secs() as u32,
                );

                timeout_event!(event);
            }
            ActivateRequest::Default { address, x, y } => {
                let proxy = self.get_notifier_item_proxy(address).await?;
                let event = proxy.activate(x, y);

                timeout_event!(event);
            }
            ActivateRequest::Secondary { address, x, y } => {
                let proxy = self.get_notifier_item_proxy(address).await?;
                let event = proxy.secondary_activate(x, y);

                timeout_event!(event);
            }
        }

        Ok(())
    }
}

fn parse_address(address: &str) -> (&str, String) {
    address
        .split_once('/')
        .map_or((address, String::from("/StatusNotifierItem")), |(d, p)| {
            (d, format!("/{p}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn item_initialization_retries_transient_failures() {
        use std::{cell::Cell, future::ready};

        let attempts = Cell::new(0);
        let result = retry_with_delays(
            [Duration::ZERO; 2],
            || {
                let attempt = attempts.get() + 1;
                attempts.set(attempt);
                ready((attempt == 3).then_some(attempt).ok_or(attempt))
            },
            |_, _| {},
        )
        .await;

        assert_eq!(result, Ok(3));
        assert_eq!(attempts.get(), 3);
    }

    #[tokio::test]
    async fn item_initialization_returns_last_failure() {
        use std::{cell::Cell, future::ready};

        let attempts = Cell::new(0);
        let result = retry_with_delays(
            [Duration::ZERO; 2],
            || {
                let attempt = attempts.get() + 1;
                attempts.set(attempt);
                ready(Err::<(), _>(attempt))
            },
            |_, _| {},
        )
        .await;

        assert_eq!(result, Err(3));
        assert_eq!(attempts.get(), 3);
    }

    #[tokio::test]
    async fn registration_stream_continues_after_item_failure() {
        use std::{cell::RefCell, future::ready};

        let handled = RefCell::new(Vec::new());
        let stream = futures_lite::stream::iter([1, 2]);

        process_stream(
            stream,
            |item| {
                handled.borrow_mut().push(item);
                ready(if item == 1 { Err("broken") } else { Ok(()) })
            },
            |_| {},
        )
        .await;

        assert_eq!(*handled.borrow(), [1, 2]);
    }

    #[test]
    fn parse_unnamed() {
        let address = ":1.58/StatusNotifierItem";
        let (destination, path) = parse_address(address);

        assert_eq!(":1.58", destination);
        assert_eq!("/StatusNotifierItem", path);
    }

    #[test]
    fn parse_named() {
        let address = ":1.72/org/ayatana/NotificationItem/dropbox_client_1398";
        let (destination, path) = parse_address(address);

        assert_eq!(":1.72", destination);
        assert_eq!("/org/ayatana/NotificationItem/dropbox_client_1398", path);
    }
}
