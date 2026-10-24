// SPDX-License-Identifier: GPL-3.0-or-later
//! What a device plugin shows in the dashboard besides its LEDs: its
//! parameters and its messages, as they change while the session runs.
//!
//! A session starts from the plugin's `ControllableParameters()` and
//! `DeviceMessages()` exports; then `device.addProperty(obj)` adds a
//! parameter or replaces the one of the same `property` (the corpus pattern
//! `getProperty` → mutate → `addProperty` is an update), `removeProperty(name)`
//! drops one (exports included), `device.addMessage(id, message, tooltip)` and
//! `removeMessage(id)` do the same for messages, and `device.notify(title,
//! text, priority)` / `denotify(id)` add and remove a message under the alert
//! id `notify` returns (docs/host-api/components-channels.md §2,
//! docs/host-api/hid-usb.md "notify"). Every change that alters the lists bumps
//! [`PluginUi::revision`]; re-adding an identical entry does not, so a plugin
//! that re-declares its properties on every frame costs no dashboard refresh.

use crate::device::{capped, MAX_ID_CHARS};
use crate::exports::parameter;
use crate::meta::{ControllableParameter, DeviceMessage};

/// Most parameters one device may declare (later additions are refused).
pub const MAX_PROPERTIES: usize = 256;
/// Most messages one device may show at once (later additions are refused).
pub const MAX_MESSAGES: usize = 64;
/// Longest message title, text or tooltip kept (characters).
pub const MAX_MESSAGE_CHARS: usize = 1024;

/// A message on the device card.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PluginMessage {
    /// `DeviceMessages()` `property`, `addMessage` id, or the `notify` alert id.
    pub id: String,
    /// `notify` title (`None` for plain messages).
    pub title: Option<String>,
    /// The text.
    pub message: String,
    /// Hover text.
    pub tooltip: Option<String>,
    /// `notify` priority: 0 info, 1 important, 2 and 3 errors (`None` for plain messages).
    pub priority: Option<i32>,
}

/// A device plugin's parameters and messages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginUi {
    properties: Vec<ControllableParameter>,
    messages: Vec<PluginMessage>,
    revision: u64,
}

impl PluginUi {
    /// The lists the plugin's exports declare.
    pub fn new(properties: &[ControllableParameter], messages: &[DeviceMessage]) -> Self {
        let mut ui = Self::default();
        for p in properties.iter().take(MAX_PROPERTIES) {
            ui.upsert_property(p.clone());
        }
        for m in messages.iter().take(MAX_MESSAGES) {
            ui.upsert_message(PluginMessage {
                id: m.property.clone(),
                title: None,
                message: m.message.clone(),
                tooltip: m.tooltip.clone().filter(|t| !t.is_empty()),
                priority: None,
            });
        }
        ui.revision = 0;
        ui
    }

    /// Parameters in declaration order.
    pub fn properties(&self) -> &[ControllableParameter] {
        &self.properties
    }

    /// Messages in the order they were added.
    pub fn messages(&self) -> &[PluginMessage] {
        &self.messages
    }

    /// Bumped by every change of either list.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// `device.addProperty(obj)` with `obj` as JSON. `Err` names why the
    /// object was refused (the message is logged to the plugin's log).
    pub fn add_property(&mut self, json: &str) -> Result<(), String> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|_| "addProperty(): parameter must be an object!".to_string())?;
        if !value.is_object() {
            return Err("addProperty(): parameter must be an object!".into());
        }
        let named = value
            .get("property")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|p| !p.is_empty());
        if !named {
            return Err("addProperty(): object requires a 'property' parameter!".into());
        }
        let mut warnings = Vec::new();
        let Some(mut p) = parameter(&value, &mut warnings) else {
            return Err(format!("addProperty(): {}", warnings.join("; ")));
        };
        p.property = capped(p.property, MAX_ID_CHARS);
        let known = self.properties.iter().any(|q| q.property == p.property);
        if !known && self.properties.len() >= MAX_PROPERTIES {
            return Err(format!("addProperty(): more than {MAX_PROPERTIES} properties"));
        }
        self.upsert_property(p);
        Ok(())
    }

    /// `device.removeProperty(name)`; `false` when there was none.
    pub fn remove_property(&mut self, name: &str) -> bool {
        let before = self.properties.len();
        self.properties.retain(|p| p.property != name);
        self.changed(self.properties.len() != before)
    }

    /// `device.addMessage(id, message, tooltip)` (an empty tooltip is none).
    pub fn add_message(&mut self, id: String, message: String, tooltip: String) -> Result<(), String> {
        self.add(PluginMessage {
            id,
            title: None,
            message,
            tooltip: Some(tooltip).filter(|t| !t.is_empty()),
            priority: None,
        })
    }

    /// `device.notify(title, text, priority)` under the alert id `id`.
    pub fn notify(&mut self, id: String, title: String, text: String, priority: i32) -> Result<(), String> {
        self.add(PluginMessage {
            id,
            title: Some(title).filter(|t| !t.is_empty()),
            message: text,
            tooltip: None,
            priority: Some(priority),
        })
    }

    /// `device.removeMessage(id)` / `device.denotify(id)`; `false` when there was none.
    pub fn remove_message(&mut self, id: &str) -> bool {
        let before = self.messages.len();
        self.messages.retain(|m| m.id != id);
        self.changed(self.messages.len() != before)
    }

    fn add(&mut self, mut m: PluginMessage) -> Result<(), String> {
        m.id = capped(m.id, MAX_ID_CHARS);
        m.message = capped(m.message, MAX_MESSAGE_CHARS);
        m.title = m.title.map(|t| capped(t, MAX_MESSAGE_CHARS));
        m.tooltip = m.tooltip.map(|t| capped(t, MAX_MESSAGE_CHARS));
        let known = self.messages.iter().any(|q| q.id == m.id);
        if !known && self.messages.len() >= MAX_MESSAGES {
            return Err(format!("addMessage(): more than {MAX_MESSAGES} messages"));
        }
        self.upsert_message(m);
        Ok(())
    }

    fn upsert_property(&mut self, p: ControllableParameter) {
        let changed = match self.properties.iter_mut().find(|q| q.property == p.property) {
            Some(q) if *q == p => false,
            Some(q) => {
                *q = p;
                true
            }
            None => {
                self.properties.push(p);
                true
            }
        };
        self.changed(changed);
    }

    fn upsert_message(&mut self, m: PluginMessage) {
        let changed = match self.messages.iter_mut().find(|q| q.id == m.id) {
            Some(q) if *q == m => false,
            Some(q) => {
                *q = m;
                true
            }
            None => {
                self.messages.push(m);
                true
            }
        };
        self.changed(changed);
    }

    fn changed(&mut self, changed: bool) -> bool {
        if changed {
            self.revision += 1;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{ParamType, ParamValue};

    #[test]
    fn add_update_and_remove_properties() {
        let mut ui = PluginUi::default();
        ui.add_property(
            r#"{"property":"dpi1","group":"dpi","label":"DPI 1","step":"100","type":"number",
                "min":"100","max":"26000","default":"800","live":"false","order":"2"}"#,
        )
        .unwrap();
        let p = &ui.properties()[0];
        assert_eq!(p.param_type, ParamType::Number);
        assert_eq!((p.min, p.max, p.step), (Some(100.0), Some(26000.0), Some(100.0)));
        assert_eq!(p.default, Some(ParamValue::Number(800.0)));
        assert!(!p.live);
        assert_eq!(p.order, Some(2));
        assert_eq!(ui.revision(), 1);

        // Same object again: no change.
        ui.add_property(
            r#"{"property":"dpi1","group":"dpi","label":"DPI 1","step":"100","type":"number",
                "min":"100","max":"26000","default":"800","live":"false","order":"2"}"#,
        )
        .unwrap();
        assert_eq!(ui.revision(), 1);
        // Mutated and re-added: replaced in place.
        ui.add_property(r#"{"property":"key","type":"keycode","label":"Key","default":"F1"}"#)
            .unwrap();
        ui.add_property(r#"{"property":"dpi1","type":"number","label":"DPI 1","max":12000}"#)
            .unwrap();
        assert_eq!(ui.properties().len(), 2);
        assert_eq!(ui.properties()[0].max, Some(12000.0));
        assert_eq!(ui.properties()[1].param_type, ParamType::Keycode);
        assert_eq!(ui.revision(), 3);

        assert!(ui.remove_property("dpi1"));
        assert!(!ui.remove_property("dpi1"));
        assert_eq!(ui.revision(), 4);
    }

    #[test]
    fn malformed_properties_are_refused_with_the_exe_messages() {
        let mut ui = PluginUi::default();
        assert_eq!(
            ui.add_property("3").unwrap_err(),
            "addProperty(): parameter must be an object!"
        );
        assert_eq!(
            ui.add_property(r#"{"label":"x"}"#).unwrap_err(),
            "addProperty(): object requires a 'property' parameter!"
        );
        assert!(ui
            .add_property(r#"{"property":"x","type":"sensor"}"#)
            .unwrap_err()
            .contains("unknown ControllableParameters type"));
        assert_eq!(ui.revision(), 0);
    }

    #[test]
    fn messages_and_notifications() {
        let mut ui = PluginUi::new(
            &[],
            &[DeviceMessage {
                property: "fw".into(),
                message: "Update the firmware".into(),
                tooltip: Some(String::new()),
            }],
        );
        assert_eq!(ui.revision(), 0);
        assert_eq!(ui.messages()[0].tooltip, None);
        ui.add_message("m".into(), "Hello".into(), "tip".into()).unwrap();
        ui.notify("lightloom-alert-1".into(), "Title".into(), "Body".into(), 3)
            .unwrap();
        assert_eq!(ui.messages().len(), 3);
        assert_eq!(ui.messages()[2].priority, Some(3));
        assert!(ui.remove_message("fw"));
        assert!(ui.remove_message("lightloom-alert-1"));
        assert_eq!(
            ui.messages().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["m"]
        );
        for i in 0..MAX_MESSAGES + 1 {
            let r = ui.add_message(format!("x{i}"), String::new(), String::new());
            assert_eq!(r.is_err(), i >= MAX_MESSAGES - 1, "{i}");
        }
    }
}
