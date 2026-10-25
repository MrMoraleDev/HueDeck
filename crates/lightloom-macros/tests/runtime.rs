// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro runtime end to end on the scripts of `tests/scripts` (written
//! for these tests), with a fake engine, a recording desktop (no input ever
//! reaches the OS) and a fake hook driven by hand.

use lightloom_hal::input::{DesktopCall, HookEvent, HookSink, InputHook, RecordingDesktop, SynthInput};
use lightloom_hal::HalResult;
use lightloom_macros::host::{EngineCall, EngineState, MacroHost, Prompt};
use lightloom_macros::manager::{MacroConfig, MacroManager, ScriptDir};
use lightloom_plugin_host::input::PluginInput;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Engine {
    calls: Mutex<Vec<EngineCall>>,
    lines: Mutex<Vec<String>>,
    prompts: Mutex<Vec<Prompt>>,
    dashboard: Mutex<bool>,
}

impl MacroHost for Engine {
    fn call(&self, call: EngineCall) -> Result<Value, String> {
        self.calls.lock().unwrap().push(call);
        Ok(Value::Null)
    }
    fn state(&self) -> EngineState {
        EngineState {
            effect: Some("Plasma".into()),
            effects: vec!["Plasma".into()],
            brightness: 70,
            paused: false,
            devices: Vec::new(),
        }
    }
    fn show_prompt(&self, prompt: &Prompt) -> bool {
        self.prompts.lock().unwrap().push(prompt.clone());
        *self.dashboard.lock().unwrap()
    }
    fn log(&self, script: &str, line: &str) {
        self.lines.lock().unwrap().push(format!("[{script}] {line}"));
    }
}

#[derive(Clone, Default)]
struct FakeHook {
    state: Arc<Mutex<(bool, bool, Option<HookSink>)>>,
}

impl InputHook for FakeHook {
    fn start(&mut self, keyboard: bool, mouse: bool, sink: HookSink) -> HalResult<()> {
        *self.state.lock().unwrap() = (keyboard, mouse, Some(sink));
        Ok(())
    }
    fn stop(&mut self) {
        *self.state.lock().unwrap() = (false, false, None);
    }
    fn active(&self) -> (bool, bool) {
        let s = self.state.lock().unwrap();
        (s.0, s.1)
    }
}

impl FakeHook {
    fn fire(&self, e: HookEvent) -> bool {
        let sink = self.state.lock().unwrap().2.clone().expect("hook installed");
        sink(&e)
    }
}

fn copy_scripts(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripts");
    let dst = std::env::temp_dir().join(format!("lightloom-macros-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    for kind in ["Actions", "Inputs"] {
        std::fs::create_dir_all(dst.join(kind)).unwrap();
        for e in std::fs::read_dir(src.join(kind)).unwrap().flatten() {
            std::fs::copy(e.path(), dst.join(kind).join(e.file_name())).unwrap();
        }
    }
    dst
}

fn wait(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Rig {
    engine: Arc<Engine>,
    desktop: Arc<RecordingDesktop>,
    hook: FakeHook,
    m: MacroManager,
    root: PathBuf,
}

fn rig(name: &str) -> Rig {
    let root = copy_scripts(name);
    let engine = Arc::new(Engine::default());
    let desktop = RecordingDesktop::new();
    let hook = FakeHook::default();
    let mut m = MacroManager::new(engine.clone(), desktop.clone(), Some(Box::new(hook.clone())), MacroConfig::default());
    m.set_dirs(ScriptDir::pair("user", &root).to_vec());
    Rig {
        engine,
        desktop,
        hook,
        m,
        root,
    }
}

fn lines(e: &Engine) -> Vec<String> {
    e.lines.lock().unwrap().clone()
}

fn opts(v: Value) -> Option<Map<String, Value>> {
    v.as_object().cloned()
}

#[test]
fn scripts_are_listed_with_options_and_errors() {
    let r = rig("list");
    let v = serde_json::to_value(r.m.view()).unwrap();
    assert_eq!(v["enabled"], false);
    assert_eq!(v["emergencyStop"], "Ctrl + Alt + Shift + F12");
    let scripts = v["scripts"].as_array().unwrap();
    assert_eq!(scripts.len(), 6);
    let by = |id: &str| scripts.iter().find(|s| s["id"] == id).unwrap_or_else(|| panic!("{id}")).clone();
    let b = by("user:action:Brightness");
    assert_eq!(b["name"], "Brightness");
    assert_eq!(b["options"][0]["value"], 40);
    assert_eq!(b["options"][1]["value"], false);
    assert_eq!(by("user:action:Broken")["error"], "Missing execute callback.");
    let h = by("user:input:Hotkey");
    assert_eq!(h["handlers"], json!(["onKey"]));
    assert_eq!(h["running"], false);
    assert!(r.m.view().scripts.iter().all(|s| !s.running), "nothing runs while macros are off");
    let _ = std::fs::remove_dir_all(&r.root);
}

#[test]
fn hotkeys_trigger_the_chain_and_are_swallowed() {
    let mut r = rig("hotkey");
    let input = "user:input:Hotkey";
    r.m.update_script(input, Some(true), None, Some(vec!["user:action:Press Combo".into()]))
        .unwrap();
    assert_eq!(r.hook.active(), (false, false), "macros are off");
    r.m.update_settings(Some(true), None, None, None);
    assert!(r.m.view().scripts.iter().any(|s| s.id == input && s.running));
    assert_eq!(r.hook.active(), (false, false), "the keyboard hook is not allowed yet");
    r.m.update_settings(None, Some(true), None, None);
    assert_eq!(r.hook.active(), (true, false));

    let key = |vk: u16, released: bool| HookEvent::Key {
        vk,
        released,
        extended: false,
        injected: false,
    };
    assert!(!r.hook.fire(key(0xA2, false)), "Ctrl alone passes");
    assert!(r.hook.fire(key(0x70, false)), "Ctrl + F1 is swallowed");
    assert!(!r.hook.fire(key(0x70, true)));
    assert!(!r.hook.fire(key(0xA2, true)));
    assert!(!r.hook.fire(key(0x70, false)), "F1 alone passes");
    r.m.tick();
    wait("the chain's key press", || r.desktop.inputs().len() == 4);
    assert_eq!(
        r.desktop.inputs(),
        vec![
            SynthInput::Key { vk: 0x11, down: true, extended: false },
            SynthInput::Key { vk: 0x43, down: true, extended: false },
            SynthInput::Key { vk: 0x43, down: false, extended: false },
            SynthInput::Key { vk: 0x11, down: false, extended: false },
        ]
    );

    // A plugin's macro key reaches onKey too (not swallowed: nothing to swallow).
    let sink = r.m.input_sink();
    sink(
        "1234:5678:X",
        PluginInput::Key {
            key: "G1".into(),
            key_code: 0,
            released: false,
        },
    );
    wait("the plugin key's trigger", || {
        r.m.tick();
        r.desktop.inputs().len() == 8
    });
    // sendHid injects a real key while macros are on.
    sink("1234:5678:X", PluginInput::KeyHid { vk: 0xAF, released: false });
    assert_eq!(
        r.desktop.inputs().last(),
        Some(&SynthInput::Key { vk: 0xAF, down: true, extended: true })
    );

    // Option changes reach the running script.
    r.m.update_script(input, None, opts(json!({"Eat": false})), None).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    r.hook.fire(key(0xA2, false));
    assert!(!r.hook.fire(key(0x70, false)), "no longer swallowed");
    r.hook.fire(key(0xA2, true));

    // The emergency stop turns everything off.
    for vk in [0xA2, 0xA4, 0xA0] {
        r.hook.fire(key(vk, false));
    }
    assert!(!r.hook.fire(key(0x7B, false)));
    r.m.tick();
    assert!(!r.m.config().enabled);
    assert!(r.m.take_changed());
    assert_eq!(r.hook.active(), (false, false));
    assert!(lines(&r.engine).iter().any(|l| l.contains("emergency stop")));
    // No line ever names a key.
    assert!(
        !lines(&r.engine)
            .iter()
            .any(|l| !l.contains("emergency") && (l.contains("F1") || l.contains("G1"))),
        "{:?}",
        lines(&r.engine)
    );
    let _ = std::fs::remove_dir_all(&r.root);
}

#[test]
fn actions_reach_the_engine_and_the_policy_refuses() {
    let mut r = rig("actions");
    assert!(r.m.run("user:action:Brightness").is_err(), "macros are off");
    r.m.update_settings(Some(true), None, None, None);
    r.m.update_script("user:action:Brightness", None, opts(json!({"Level": "55", "Pause": true})), None)
        .unwrap();
    assert!(r
        .m
        .update_script("user:action:Brightness", None, opts(json!({"Level": 500})), None)
        .is_err());
    assert!(r.m.update_script("user:action:Brightness", Some(true), None, None).is_err());
    r.m.run("user:action:Brightness").unwrap();
    wait("the brightness calls", || r.engine.calls.lock().unwrap().len() == 2);
    assert_eq!(
        *r.engine.calls.lock().unwrap(),
        vec![EngineCall::Brightness(55), EngineCall::Paused(true)]
    );
    wait("the console line", || lines(&r.engine).iter().any(|l| l == "[Brightness] was 70 on Plasma"));

    r.engine.calls.lock().unwrap().clear();
    *r.engine.dashboard.lock().unwrap() = true;
    r.m.run("user:action:Policy").unwrap();
    wait("the prompt", || !r.engine.prompts.lock().unwrap().is_empty());
    let l = lines(&r.engine);
    for want in ["launch: MacroRefused", "file: MacroRefused", "web: ok", "badkey: TypeError", "globals: ok"] {
        assert!(l.iter().any(|x| x.ends_with(want)), "{want}: {l:?}");
    }
    assert_eq!(
        *r.engine.calls.lock().unwrap(),
        vec![EngineCall::Action("layout/apply/Desk".into())]
    );
    let prompt = r.engine.prompts.lock().unwrap()[0].clone();
    assert_eq!((prompt.title.as_str(), prompt.script.as_str()), ("Name?", "Policy"));
    r.m.answer(prompt.id, Some("Ada".into())).unwrap();
    assert!(r.m.answer(prompt.id, None).is_err(), "answered once");
    wait("the clipboard", || {
        r.desktop.calls().contains(&DesktopCall::Clipboard("got Ada".into()))
    });
    assert!(r.desktop.calls().contains(&DesktopCall::Open("https://example.com".into())));

    // With launching allowed, a command-line string is split into arguments.
    r.m.update_settings(None, None, None, Some(true));
    *r.engine.dashboard.lock().unwrap() = false;
    r.m.run("user:action:Policy").unwrap();
    wait("the launch", || {
        r.desktop.calls().iter().any(|c| matches!(c, DesktopCall::Launch { args, .. } if args == &["a", "b"]))
    });
    wait("the cancelled prompt", || lines(&r.engine).iter().any(|l| l.ends_with("prompt cancelled")));
    let _ = std::fs::remove_dir_all(&r.root);
}

#[test]
fn application_events_carry_both_casings_and_folders_are_watched() {
    let mut r = rig("events");
    r.m.update_settings(Some(true), None, None, None);
    r.m.update_script("user:input:Effect Watch", Some(true), None, Some(vec!["user:action:Brightness".into()]))
        .unwrap();
    r.m.effect_changed("Plasma");
    r.m.session_lock(true);
    wait("the events", || lines(&r.engine).iter().filter(|l| l.starts_with("[Effect Watch]")).count() >= 2);
    let l = lines(&r.engine);
    assert!(l.contains(&r#"[Effect Watch] Application|Effect Changed|true|{"EffectName":"Plasma"}"#.to_string()), "{l:?}");
    assert!(l.contains(&r#"[Effect Watch] Application|Session Lock|true|{"locked":true}"#.to_string()), "{l:?}");
    wait("the trigger", || {
        r.m.tick();
        r.engine.calls.lock().unwrap().contains(&EngineCall::Brightness(40))
    });

    r.desktop.set_processes(Some(vec!["a.exe".into()]));
    r.m.poll_processes();
    r.desktop.set_processes(Some(vec!["b.exe".into()]));
    r.m.poll_processes();
    wait("process events", || {
        let l = lines(&r.engine);
        l.iter().any(|x| x.contains(r#"Process Started|true|{"exeName":"b.exe"}"#))
            && l.iter().any(|x| x.contains(r#"Process Ended|true|{"exeName":"a.exe"}"#))
    });

    std::fs::write(
        r.root.join("Actions").join("New.js"),
        "export function Action() { this.Name = 'New'; this.Description = 'd'; this.onExecute = () => {}; }",
    )
    .unwrap();
    assert!(r.m.watch());
    assert!(r.m.scripts().iter().any(|s| s.id == "user:action:New" && s.error.is_none()));
    assert!(r.m.view().scripts.iter().any(|s| s.id == "user:input:Effect Watch" && s.running), "restarted");
    assert!(!r.m.watch());
    let _ = std::fs::remove_dir_all(&r.root);
}
