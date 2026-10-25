// SPDX-License-Identifier: GPL-3.0-or-later
//! The Linux tray: a StatusNotifierItem (`org.kde.StatusNotifierItem`) with
//! a `com.canonical.dbusmenu` menu on the session D-Bus. KDE Plasma, most
//! other panels, and GNOME with the AppIndicator extension show it.
//!
//! The menu is the Windows one ([`crate::menu_with_profiles`]): open the
//! dashboard (also a left click on the icon), pause/resume, brightness,
//! profiles, quit. *Start at sign-in* is left out: on Linux the systemd user
//! unit starts the daemon (docs/linux-porting.md).
//!
//! The pure half (the menu as a dbusmenu tree, label escaping, item ids) is
//! tested on every OS. The D-Bus half (Linux only) serves the two objects;
//! method handlers only queue commands, and the main thread does the HTTP
//! calls, refreshes the menu every 3 s or when a host is about to show it,
//! and emits `LayoutUpdated` / `NewToolTip` when something changed. Without
//! a `org.kde.StatusNotifierWatcher` on the bus there is nowhere to show an
//! icon: the binary says so and exits, as before.

use crate::{Action, Entry};

/// The object path of the item.
pub const ITEM_PATH: &str = "/StatusNotifierItem";
/// The object path of its menu.
pub const MENU_PATH: &str = "/MenuBar";
/// The watcher every SNI host runs.
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
/// One tray per session: whoever owns this name (under the Flatpak app id,
/// which a Flatpak may own without a permission).
pub const TRAY_NAME: &str = "org.lightloom.Lightloom.Tray";
/// How often the daemon's state is polled.
pub const POLL: std::time::Duration = std::time::Duration::from_secs(3);

/// First id of separators and submenus (item ids are [`Action::id`]s, below 1000).
const SYNTHETIC_IDS: i32 = 10_000;

/// What a menu node looks like (the dbusmenu properties Lightloom uses).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The invisible root (id 0).
    Root,
    /// A command.
    Item {
        /// The label, `_` doubled (dbusmenu mnemonics).
        label: String,
        /// Greyed out when `false`.
        enabled: bool,
        /// `Some(state)`: a radio item (brightness presets, profiles).
        radio: Option<bool>,
    },
    /// A line.
    Separator,
    /// A submenu.
    Submenu {
        /// Its label.
        label: String,
        /// Greyed out when `false`.
        enabled: bool,
    },
}

/// One node of the menu tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// dbusmenu id.
    pub id: i32,
    /// What it is.
    pub kind: Kind,
    /// Its children.
    pub children: Vec<Node>,
}

impl Node {
    /// The node with `id` in this tree.
    pub fn find(&self, id: i32) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// The dbusmenu properties of this node, as `(name, value)`.
    pub fn properties(&self) -> Vec<(&'static str, Prop)> {
        match &self.kind {
            Kind::Root => vec![("children-display", Prop::Str("submenu".into()))],
            Kind::Separator => vec![("type", Prop::Str("separator".into()))],
            Kind::Submenu { label, enabled } => vec![
                ("label", Prop::Str(label.clone())),
                ("enabled", Prop::Bool(*enabled)),
                ("children-display", Prop::Str("submenu".into())),
            ],
            Kind::Item {
                label,
                enabled,
                radio,
            } => {
                let mut p = vec![
                    ("label", Prop::Str(label.clone())),
                    ("enabled", Prop::Bool(*enabled)),
                ];
                if let Some(on) = radio {
                    p.push(("toggle-type", Prop::Str("radio".into())));
                    p.push(("toggle-state", Prop::Int(i32::from(*on))));
                }
                p
            }
        }
    }
}

/// A dbusmenu property value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prop {
    /// `s`.
    Str(String),
    /// `b`.
    Bool(bool),
    /// `i`.
    Int(i32),
}

/// A Windows menu label (`&&` is a literal `&`) as a dbusmenu label (`_`
/// marks a mnemonic, so a literal one is doubled).
pub fn label(windows: &str) -> String {
    windows.replace("&&", "&").replace('_', "__")
}

/// The menu command of dbusmenu item `id`.
pub fn action_of(id: i32) -> Option<Action> {
    if !(1..SYNTHETIC_IDS).contains(&id) {
        return None;
    }
    Action::from_id(u16::try_from(id).ok()?)
}

/// The Windows menu without what Linux does not have (Start at sign-in) and
/// without the separators that leaves doubled or trailing.
pub fn linux_entries(entries: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in entries {
        match &e {
            Entry::Item {
                action: Action::ToggleAutostart,
                ..
            } => continue,
            Entry::Separator if matches!(out.last(), None | Some(Entry::Separator)) => continue,
            _ => out.push(e),
        }
    }
    while matches!(out.last(), Some(Entry::Separator)) {
        out.pop();
    }
    out
}

/// `entries` as a dbusmenu tree under the root (id 0).
pub fn tree(entries: &[Entry]) -> Node {
    let mut next = SYNTHETIC_IDS;
    Node {
        id: 0,
        kind: Kind::Root,
        children: nodes(entries, &mut next),
    }
}

fn nodes(entries: &[Entry], next: &mut i32) -> Vec<Node> {
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let node = match e {
            Entry::Item {
                action,
                label: text,
                checked,
                enabled,
                ..
            } => Node {
                id: i32::from(action.id()),
                kind: Kind::Item {
                    label: label(text),
                    enabled: *enabled,
                    radio: matches!(action, Action::Brightness(_) | Action::Profile(_)).then_some(*checked),
                },
                children: Vec::new(),
            },
            Entry::Separator => {
                *next += 1;
                Node {
                    id: *next - 1,
                    kind: Kind::Separator,
                    children: Vec::new(),
                }
            }
            Entry::Submenu {
                label: text,
                enabled,
                items,
            } => {
                *next += 1;
                let id = *next - 1;
                Node {
                    id,
                    kind: Kind::Submenu {
                        label: label(text),
                        enabled: *enabled,
                    },
                    children: nodes(items, next),
                }
            }
        };
        out.push(node);
    }
    out
}

/// The icon's pixmaps for the `IconPixmap` property: `(width, height,
/// ARGB32 in network byte order)` from the PNG images of `ico`.
#[cfg(target_os = "linux")]
pub fn pixmaps(ico: &[u8]) -> Vec<(i32, i32, Vec<u8>)> {
    let Some(count) = ico
        .get(4..6)
        .map(|c| usize::from(u16::from_le_bytes([c[0], c[1]])))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..count {
        let Some(dir) = ico.get(6 + 16 * i..22 + 16 * i) else {
            break;
        };
        let size = u32::from_le_bytes([dir[8], dir[9], dir[10], dir[11]]) as usize;
        let offset = u32::from_le_bytes([dir[12], dir[13], dir[14], dir[15]]) as usize;
        let Some(img) = ico.get(offset..offset.saturating_add(size)) else {
            continue;
        };
        let Ok(decoded) = image::load_from_memory_with_format(img, image::ImageFormat::Png) else {
            continue;
        };
        let rgba = decoded.to_rgba8();
        if rgba.width() > 64 {
            continue; // hosts scale down; the big ones only cost bus traffic
        }
        let mut argb = Vec::with_capacity(rgba.as_raw().len());
        for px in rgba.pixels() {
            let [r, g, b, a] = px.0;
            argb.extend_from_slice(&[a, r, g, b]);
        }
        out.push((rgba.width() as i32, rgba.height() as i32, argb));
    }
    out
}

#[cfg(target_os = "linux")]
pub use dbus::run;

#[cfg(target_os = "linux")]
mod dbus {
    use super::{
        action_of, linux_entries, pixmaps, tree, Node, Prop, ITEM_PATH, MENU_PATH, POLL, TRAY_NAME, WATCHER,
    };
    use crate::{
        menu_with_profiles, perform, tooltip, Action, Api, Autostart, Desktop, HttpApi, Outcome, RunKey,
        Status,
    };
    use lightloom_hal_linux::dbus;
    use std::collections::HashMap;
    use std::io;
    use std::path::PathBuf;
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    use zbus::{Connection, OwnedObjectPath, OwnedVariant as OwnedValue, Structure, Variant as Value};

    /// The ICO the Windows tray uses (`packaging/wix/lightloom.ico`).
    const ICO: &[u8] = include_bytes!("../../../packaging/wix/lightloom.ico");

    /// What the D-Bus handlers ask of the main thread.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Command {
        /// Run a menu command.
        Run(Action),
        /// A host is about to show the menu: refresh it now.
        Refresh,
        /// The watcher (re)appeared: register again.
        Register,
    }

    /// What both objects show, kept by the main thread.
    #[derive(Debug)]
    pub struct Shared {
        /// The menu tree.
        pub menu: Node,
        /// Bumped on every change of `menu`.
        pub revision: u32,
        /// The tooltip line.
        pub tooltip: String,
    }

    type Pixmap = (i32, i32, Vec<u8>);
    /// One dbusmenu layout node, `(ia{sv}av)`.
    pub type LayoutNode = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

    fn prop_value(p: &Prop) -> OwnedValue {
        match p {
            Prop::Str(s) => OwnedValue::from(zbus::Str::from(s.clone())),
            Prop::Bool(b) => OwnedValue::from(*b),
            Prop::Int(i) => OwnedValue::from(*i),
        }
    }

    fn props(node: &Node, wanted: &[String]) -> HashMap<String, OwnedValue> {
        node.properties()
            .into_iter()
            .filter(|(k, _)| wanted.is_empty() || wanted.iter().any(|w| w == k))
            .map(|(k, v)| (k.to_string(), prop_value(&v)))
            .collect()
    }

    /// One `(ia{sv}av)` layout node, children down to `depth` levels (-1: all).
    pub fn layout(node: &Node, depth: i32, wanted: &[String]) -> LayoutNode {
        let children = if depth == 0 {
            Vec::new()
        } else {
            node.children
                .iter()
                .filter_map(|c| {
                    let (id, p, kids) = layout(c, depth - 1, wanted);
                    let s = Structure::from((id, p, kids));
                    OwnedValue::try_from(Value::Structure(s)).ok()
                })
                .collect()
        };
        (node.id, props(node, wanted), children)
    }

    /// `org.kde.StatusNotifierItem`.
    pub struct Item {
        shared: Arc<Mutex<Shared>>,
        commands: Mutex<Sender<Command>>,
        icon: Vec<Pixmap>,
    }

    impl Item {
        fn send(&self, c: Command) {
            let _ = self.commands.lock().unwrap_or_else(|e| e.into_inner()).send(c);
        }
    }

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Item {
        #[zbus(property)]
        fn category(&self) -> &str {
            "ApplicationStatus"
        }

        #[zbus(property)]
        fn id(&self) -> &str {
            "lightloom"
        }

        #[zbus(property)]
        fn title(&self) -> &str {
            "Lightloom"
        }

        #[zbus(property)]
        fn status(&self) -> &str {
            "Active"
        }

        #[zbus(property)]
        fn window_id(&self) -> i32 {
            0
        }

        #[zbus(property)]
        fn icon_name(&self) -> &str {
            ""
        }

        #[zbus(property)]
        fn icon_pixmap(&self) -> Vec<Pixmap> {
            self.icon.clone()
        }

        #[zbus(property)]
        fn overlay_icon_name(&self) -> &str {
            ""
        }

        #[zbus(property)]
        fn overlay_icon_pixmap(&self) -> Vec<Pixmap> {
            Vec::new()
        }

        #[zbus(property)]
        fn attention_icon_name(&self) -> &str {
            ""
        }

        #[zbus(property)]
        fn attention_icon_pixmap(&self) -> Vec<Pixmap> {
            Vec::new()
        }

        #[zbus(property)]
        fn attention_movie_name(&self) -> &str {
            ""
        }

        #[zbus(property)]
        fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
            let tip = self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .tooltip
                .clone();
            (String::new(), Vec::new(), "Lightloom".into(), tip)
        }

        #[zbus(property)]
        fn item_is_menu(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn menu(&self) -> OwnedObjectPath {
            OwnedObjectPath::try_from(MENU_PATH).expect("a valid path")
        }

        fn activate(&self, _x: i32, _y: i32) {
            self.send(Command::Run(Action::OpenDashboard));
        }

        fn secondary_activate(&self, _x: i32, _y: i32) {}

        fn context_menu(&self, _x: i32, _y: i32) {
            self.send(Command::Refresh);
        }

        fn scroll(&self, _delta: i32, _orientation: String) {}
    }

    /// `com.canonical.dbusmenu`.
    pub struct Menu {
        shared: Arc<Mutex<Shared>>,
        commands: Mutex<Sender<Command>>,
    }

    impl Menu {
        fn send(&self, c: Command) {
            let _ = self.commands.lock().unwrap_or_else(|e| e.into_inner()).send(c);
        }

        fn event_of(&self, id: i32, event: &str) {
            if event == "clicked" {
                if let Some(a) = action_of(id) {
                    self.send(Command::Run(a));
                }
            }
        }
    }

    #[zbus::interface(name = "com.canonical.dbusmenu")]
    impl Menu {
        #[zbus(property)]
        fn version(&self) -> u32 {
            3
        }

        #[zbus(property)]
        fn text_direction(&self) -> &str {
            "ltr"
        }

        #[zbus(property)]
        fn status(&self) -> &str {
            "normal"
        }

        #[zbus(property)]
        fn icon_theme_path(&self) -> Vec<String> {
            Vec::new()
        }

        fn get_layout(
            &self,
            parent_id: i32,
            recursion_depth: i32,
            property_names: Vec<String>,
        ) -> zbus::fdo::Result<(u32, LayoutNode)> {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let node = s
                .menu
                .find(parent_id)
                .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no menu item {parent_id}")))?;
            Ok((s.revision, layout(node, recursion_depth, &property_names)))
        }

        fn get_group_properties(
            &self,
            ids: Vec<i32>,
            property_names: Vec<String>,
        ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            ids.iter()
                .filter_map(|id| s.menu.find(*id))
                .map(|n| (n.id, props(n, &property_names)))
                .collect()
        }

        fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            s.menu
                .find(id)
                .and_then(|n| n.properties().into_iter().find(|(k, _)| *k == name))
                .map(|(_, v)| prop_value(&v))
                .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name} on {id}")))
        }

        fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
            self.event_of(id, &event_id);
        }

        fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
            for (id, event, _, _) in &events {
                self.event_of(*id, event);
            }
            Vec::new()
        }

        fn about_to_show(&self, id: i32) -> bool {
            if id == 0 {
                self.send(Command::Refresh);
            }
            false
        }

        fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
            if ids.contains(&0) {
                self.send(Command::Refresh);
            }
            (Vec::new(), Vec::new())
        }
    }

    /// The two objects over `shared`, their commands going to `commands`.
    pub fn objects(shared: Arc<Mutex<Shared>>, commands: Sender<Command>) -> (Item, Menu) {
        (
            Item {
                shared: Arc::clone(&shared),
                commands: Mutex::new(commands.clone()),
                icon: pixmaps(ICO),
            },
            Menu {
                shared,
                commands: Mutex::new(commands),
            },
        )
    }

    /// No start at sign-in on Linux (the systemd user unit does that).
    struct NoRunKey;

    impl RunKey for NoRunKey {
        fn get(&self, _name: &str) -> io::Result<Option<String>> {
            Ok(None)
        }

        fn set(&self, _name: &str, _value: &str) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "use the systemd user unit (docs/linux-porting.md)",
            ))
        }

        fn delete(&self, _name: &str) -> io::Result<()> {
            Ok(())
        }
    }

    /// The browser and the daemon next to the tray.
    struct LinuxDesktop {
        dir: Option<PathBuf>,
    }

    impl Desktop for LinuxDesktop {
        fn open_url(&self, url: &str) {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }

        fn start_and_open(&self) -> bool {
            let Some(d) = self
                .dir
                .as_ref()
                .map(|d| d.join("lightloomd"))
                .filter(|p| p.is_file())
            else {
                return false;
            };
            std::process::Command::new(d)
                .arg("--open")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .is_ok()
        }
    }

    /// The menu entries for the daemon's state now.
    fn entries(api: &HttpApi) -> (Status, Vec<crate::Entry>) {
        let status = api.status();
        let (profiles, current) = if status == Status::NotRunning {
            (Vec::new(), None)
        } else {
            api.profiles()
        };
        let all = menu_with_profiles(status, (false, false), &profiles, current.as_deref());
        (status, linux_entries(all))
    }

    /// Main-thread state.
    struct Tray {
        conn: Connection,
        shared: Arc<Mutex<Shared>>,
        api: HttpApi,
        service: String,
    }

    impl Tray {
        /// Poll the daemon; tell the host what changed.
        fn refresh(&self) {
            let (status, list) = entries(&self.api);
            let menu = tree(&list);
            let tip = tooltip(status);
            let (layout_changed, tip_changed, revision) = {
                let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                let layout_changed = s.menu != menu;
                if layout_changed {
                    s.menu = menu;
                    s.revision = s.revision.wrapping_add(1);
                }
                let tip_changed = s.tooltip != tip;
                s.tooltip = tip;
                (layout_changed, tip_changed, s.revision)
            };
            if layout_changed {
                let _ = dbus::emit(
                    &self.conn,
                    MENU_PATH,
                    "com.canonical.dbusmenu",
                    "LayoutUpdated",
                    &(revision, 0i32),
                );
            }
            if tip_changed {
                let _ = dbus::emit(
                    &self.conn,
                    ITEM_PATH,
                    "org.kde.StatusNotifierItem",
                    "NewToolTip",
                    &(),
                );
            }
        }

        fn register(&self) -> zbus::Result<()> {
            dbus::call::<_, ()>(
                &self.conn,
                Some(WATCHER),
                "/StatusNotifierWatcher",
                WATCHER,
                "RegisterStatusNotifierItem",
                &(self.service.as_str(),),
            )
        }
    }

    const BUS: &str = "org.freedesktop.DBus";

    fn has_owner(conn: &Connection, name: &str) -> bool {
        dbus::call(
            conn,
            Some(BUS),
            "/org/freedesktop/DBus",
            BUS,
            "NameHasOwner",
            &(name,),
        )
        .unwrap_or(false)
    }

    /// Re-register when the watcher comes back (the panel restarted).
    fn watch_watcher(conn: &Connection, commands: Sender<Command>) {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(BUS)
            .interface(BUS)
            .member("NameOwnerChanged")
            .arg(0, WATCHER)
            .build();
        let Ok(mut changes) = rule.and_then(|r| dbus::matching(conn, r)) else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("sni-watcher".into())
            .spawn(move || {
                while let Some(msg) = dbus::next(&mut changes) {
                    let back = msg
                        .ok()
                        .and_then(|m| m.body().deserialize::<(String, String, String)>().ok())
                        .is_some_and(|(_, _, new_owner)| !new_owner.is_empty());
                    if back && commands.send(Command::Register).is_err() {
                        return;
                    }
                }
            });
    }

    fn notify_error(message: &str) {
        use lightloom_hal::notify::Notification;
        let _ = lightloom_runtime::notify::system_notifier().notify(&Notification {
            title: "Lightloom".into(),
            body: message.into(),
        });
        eprintln!("lightloom-tray: {message}");
    }

    /// Serve until the commands stop or Quit; the process exit code.
    fn serve(
        tray: &Tray,
        commands: &Receiver<Command>,
        autostart: &Autostart<NoRunKey>,
        desktop: &LinuxDesktop,
    ) -> i32 {
        tray.refresh();
        loop {
            match commands.recv_timeout(POLL) {
                Ok(Command::Run(action)) => match perform(action, &tray.api, autostart, desktop) {
                    Ok(Outcome::Exit) => return 0,
                    Ok(Outcome::Stay) => tray.refresh(),
                    Err(e) => notify_error(&e),
                },
                Ok(Command::Refresh) | Err(RecvTimeoutError::Timeout) => tray.refresh(),
                Ok(Command::Register) => {
                    if let Err(e) = tray.register() {
                        eprintln!("lightloom-tray: registering with the panel again failed: {e}");
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return 0,
            }
        }
    }

    const NO_TRAY: &str = "lightloom-tray: no StatusNotifierItem host on this desktop (KDE Plasma, or GNOME with \
                           the AppIndicator extension, shows one); `lightloom url` prints the dashboard's address";

    /// Run the tray until Quit; the process exit code (2: nowhere to show it).
    pub fn run() -> i32 {
        let conn = match dbus::session() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{NO_TRAY} (no session D-Bus: {e})");
                return 2;
            }
        };
        match dbus::wait(conn.request_name_with_flags(TRAY_NAME, RequestNameFlags::DO_NOT_QUEUE)) {
            Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {}
            Ok(_) => return 0, // one tray per session
            Err(e) => {
                eprintln!("lightloom-tray: {e}");
                return 1;
            }
        }
        if !has_owner(&conn, WATCHER) {
            eprintln!("{NO_TRAY}");
            return 2;
        }
        let (data, _) = lightloom_runtime::dirs::from_env();
        let shared = Arc::new(Mutex::new(Shared {
            menu: tree(&[]),
            revision: 1,
            tooltip: tooltip(Status::NotRunning),
        }));
        let (tx, rx) = channel();
        let (item, menu) = objects(Arc::clone(&shared), tx.clone());
        let served = dbus::wait(conn.object_server().at(ITEM_PATH, item))
            .and_then(|_| dbus::wait(conn.object_server().at(MENU_PATH, menu)));
        if let Err(e) = served {
            eprintln!("lightloom-tray: {e}");
            return 1;
        }
        // The conventional name; hosts accept the unique name too (a Flatpak may not own the other).
        let wanted = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let service = match dbus::wait(conn.request_name(wanted.as_str())) {
            Ok(()) => wanted,
            Err(_) => match conn.unique_name() {
                Some(name) => name.to_string(),
                None => {
                    eprintln!("{NO_TRAY} (no bus name)");
                    return 2;
                }
            },
        };
        let tray = Tray {
            conn: conn.clone(),
            shared,
            api: HttpApi::new(data),
            service,
        };
        tray.refresh();
        if let Err(e) = tray.register() {
            eprintln!("{NO_TRAY} ({e})");
            return 2;
        }
        watch_watcher(&conn, tx);
        let dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(PathBuf::from));
        serve(&tray, &rx, &Autostart::new(NoRunKey, None), &LinuxDesktop { dir })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::net::UnixStream;
        use std::time::Duration;
        use zbus::connection::Builder;

        /// The tray's objects on one end of a socket pair; a host on the other.
        fn peers(menu: Node) -> (Connection, Receiver<Command>, Connection, Arc<Mutex<Shared>>) {
            let (a, b) = UnixStream::pair().unwrap();
            let shared = Arc::new(Mutex::new(Shared {
                menu,
                revision: 4,
                tooltip: "Lightloom \u{2014} 2 devices".into(),
            }));
            let (tx, rx) = channel();
            let (item, menu) = objects(Arc::clone(&shared), tx);
            let guid = zbus::Guid::generate();
            let server = std::thread::spawn(move || {
                dbus::wait(
                    Builder::unix_stream(a)
                        .server(guid)
                        .p2p()
                        .serve_at(ITEM_PATH, item)
                        .serve_at(MENU_PATH, menu)
                        .build(),
                )
                .unwrap()
            });
            let host = dbus::wait(Builder::unix_stream(b).p2p().build()).unwrap();
            (server.join().unwrap(), rx, host, shared)
        }

        fn running_menu() -> Node {
            let status = Status::Running {
                devices: 2,
                enabled: true,
                brightness: 50,
            };
            let profiles = ["Gaming".to_string(), "my_desk".to_string()];
            tree(&linux_entries(menu_with_profiles(
                status,
                (false, false),
                &profiles,
                Some("Gaming"),
            )))
        }

        fn call<R: for<'d> zbus::wire::DynamicDeserialize<'d>>(
            host: &Connection,
            path: &str,
            iface: &str,
            method: &str,
            body: &(impl serde::Serialize + zbus::DynamicType),
        ) -> R {
            dbus::call(host, None, path, iface, method, body).unwrap()
        }

        #[test]
        fn hosts_read_the_layout_and_clicks_become_commands() {
            let (_tray, commands, host, _shared) = peers(running_menu());
            let (revision, (root, _props, children)): (u32, LayoutNode) = call(
                &host,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "GetLayout",
                &(0i32, -1i32, Vec::<String>::new()),
            );
            assert_eq!((revision, root), (4, 0));
            // Open, separator, pause, brightness, profiles, separator, quit.
            assert_eq!(children.len(), 7);
            let first: LayoutNode = children[0].clone().try_into().unwrap();
            assert_eq!(first.0, i32::from(Action::OpenDashboard.id()));
            assert_eq!(
                String::try_from(first.1["label"].clone()).unwrap(),
                "Open dashboard"
            );
            let profiles: LayoutNode = children[4].clone().try_into().unwrap();
            assert_eq!(String::try_from(profiles.1["label"].clone()).unwrap(), "Profiles");
            assert_eq!(profiles.2.len(), 2);
            let mine: LayoutNode = profiles.2[1].clone().try_into().unwrap();
            assert_eq!(String::try_from(mine.1["label"].clone()).unwrap(), "my__desk");
            assert_eq!(i32::try_from(mine.1["toggle-state"].clone()).unwrap(), 0);

            // Depth 0 and a property filter.
            let (_, (_, props, kids)): (u32, LayoutNode) = call(
                &host,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "GetLayout",
                &(profiles.0, 0i32, vec!["label".to_string()]),
            );
            assert!(kids.is_empty());
            assert_eq!(props.keys().collect::<Vec<_>>(), ["label"]);

            let pause = i32::from(Action::TogglePause.id());
            let () = call(
                &host,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "Event",
                &(pause, "clicked", Value::from(0i32), 0u32),
            );
            assert_eq!(
                commands.recv_timeout(Duration::from_secs(5)).unwrap(),
                Command::Run(Action::TogglePause)
            );
            // Hovering is not a click; separators are not commands.
            let () = call(
                &host,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "Event",
                &(pause, "hovered", Value::from(0i32), 0u32),
            );
            let open: bool = call(
                &host,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "AboutToShow",
                &(0i32,),
            );
            assert!(!open);
            assert_eq!(
                commands.recv_timeout(Duration::from_secs(5)).unwrap(),
                Command::Refresh
            );

            // A left click on the icon opens the dashboard.
            let () = call(
                &host,
                ITEM_PATH,
                "org.kde.StatusNotifierItem",
                "Activate",
                &(10i32, 10i32),
            );
            assert_eq!(
                commands.recv_timeout(Duration::from_secs(5)).unwrap(),
                Command::Run(Action::OpenDashboard)
            );
        }

        #[test]
        fn the_item_describes_itself() {
            let (_tray, _commands, host, _shared) = peers(running_menu());
            let get = |name: &str| -> OwnedValue {
                call(
                    &host,
                    ITEM_PATH,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    &("org.kde.StatusNotifierItem", name),
                )
            };
            assert_eq!(String::try_from(get("Id")).unwrap(), "lightloom");
            assert_eq!(String::try_from(get("Status")).unwrap(), "Active");
            assert_eq!(
                OwnedObjectPath::try_from(get("Menu")).unwrap().as_str(),
                MENU_PATH
            );
            let (_, _, title, text): (String, Vec<Pixmap>, String, String) =
                get("ToolTip").try_into().unwrap();
            assert_eq!(title, "Lightloom");
            assert_eq!(text, "Lightloom \u{2014} 2 devices");
            let icons: Vec<Pixmap> = get("IconPixmap").try_into().unwrap();
            assert!(icons
                .iter()
                .any(|(w, h, px)| *w == 32 && *h == 32 && px.len() == 32 * 32 * 4));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{menu_with_profiles, Status};

    fn running() -> Status {
        Status::Running {
            devices: 2,
            enabled: false,
            brightness: 75,
        }
    }

    #[test]
    fn the_linux_menu_is_the_windows_one_without_start_at_sign_in() {
        let all = menu_with_profiles(running(), (true, true), &["A&B".to_string()], Some("A&B"));
        let linux = linux_entries(all);
        assert!(!linux.iter().any(|e| matches!(
            e,
            Entry::Item {
                action: Action::ToggleAutostart,
                ..
            }
        )));
        assert!(!matches!(linux.first(), Some(Entry::Separator)));
        assert!(!matches!(linux.last(), Some(Entry::Separator)));
        assert!(linux
            .windows(2)
            .all(|w| !matches!(w, [Entry::Separator, Entry::Separator])));

        let root = tree(&linux);
        assert_eq!(root.id, 0);
        let labels: Vec<String> = root
            .children
            .iter()
            .filter_map(|n| match &n.kind {
                Kind::Item { label, .. } | Kind::Submenu { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Open dashboard",
                "Resume lighting",
                "Brightness",
                "Profiles",
                "Quit Lightloom"
            ]
        );
        // Windows' `&&` is a plain `&` again; the current profile is the checked radio item.
        let profile = root.find(i32::from(Action::Profile(0).id())).unwrap();
        assert_eq!(
            profile.kind,
            Kind::Item {
                label: "A&B".into(),
                enabled: true,
                radio: Some(true)
            }
        );
        let b75 = root.find(i32::from(Action::Brightness(75).id())).unwrap();
        assert!(b75.properties().contains(&("toggle-state", Prop::Int(1))));
        let b25 = root.find(i32::from(Action::Brightness(25).id())).unwrap();
        assert!(b25.properties().contains(&("toggle-state", Prop::Int(0))));
    }

    #[test]
    fn ids_map_back_to_commands_and_synthetic_ids_do_not() {
        let root = tree(&linux_entries(menu_with_profiles(
            running(),
            (false, false),
            &[],
            None,
        )));
        let mut ids = Vec::new();
        fn walk(n: &Node, ids: &mut Vec<i32>) {
            ids.push(n.id);
            for c in &n.children {
                walk(c, ids);
            }
        }
        walk(&root, &mut ids);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "ids are unique: {ids:?}");
        assert_eq!(action_of(i32::from(Action::Quit.id())), Some(Action::Quit));
        assert_eq!(
            action_of(i32::from(Action::Brightness(50).id())),
            Some(Action::Brightness(50))
        );
        assert_eq!(action_of(0), None);
        assert_eq!(action_of(SYNTHETIC_IDS), None);
        let sep = root.children.iter().find(|n| n.kind == Kind::Separator).unwrap();
        assert_eq!(sep.properties(), [("type", Prop::Str("separator".into()))]);
        assert!(action_of(sep.id).is_none());
    }

    #[test]
    fn labels_escape_mnemonics() {
        assert_eq!(label("Rock && Roll_2"), "Rock & Roll__2");
        assert_eq!(label("Open dashboard"), "Open dashboard");
    }

    #[test]
    fn not_running_menu_still_offers_start() {
        let root = tree(&linux_entries(menu_with_profiles(
            Status::NotRunning,
            (false, false),
            &[],
            None,
        )));
        let first = &root.children[0];
        assert!(matches!(&first.kind, Kind::Item { label, enabled: true, .. } if label == "Start Lightloom"));
        let last = root.children.last().unwrap();
        assert!(matches!(&last.kind, Kind::Item { label, .. } if label == "Close"));
    }
}
