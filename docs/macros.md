<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Macros

Lightloom runs SignalRGB-shaped macro scripts: **Inputs** watch for something (a key
combination, a program starting, the effect changing, the PC locking) and **Actions** do
something (change the effect or the brightness, press keys, open a link). An Input calls
`block.trigger()` to run its chain of Actions. The script contract is in
`docs/host-api/macros-system.md`; the runtime is `crates/lightloom-macros`.

Everything is **off by default**: no script runs until you turn on **Run macros** in the
Macros page of the dashboard.

## Where scripts live

- `<config>/macroscripts/Actions/*.js` and `<config>/macroscripts/Inputs/*.js`
  (`%APPDATA%\Lightloom\macroscripts` on Windows by default; the folders are
  created at start). The page shows the exact paths.
- SignalRGB's bundled `Macroscripts` folder, while **Use the SignalRGB library** is on
  (Lightloom runs those files in place; it ships none of them).

The folders are watched (every 2 s): a new or changed file is listed again and the running
scripts restart. Each script is listed with its options; one that cannot run says why
(`Missing execute callback.`, `Input doesn't have property: Name`, a syntax error, ...).

A script is an ES module exporting `function Action()` or `function Input()`; the
constructor sets `this.Name`, `this.Description`, `this.Options` and the handlers
(`onExecute` for Actions; `onApplication`, `onKey`, `onMouse` for Inputs). Scripts may not
import anything. There is no `window`, `setTimeout` or `Image`.

## Using the page

- **Inputs**: tick one to run it, set its options, and add the Actions it runs ("Then run").
- **Actions**: set their options and press **Run** to try one.
- Option values are checked against their type (a number within its range, a combobox key
  it lists, a key combination like `Ctrl + Shift + F1`) and saved; they travel with the
  settings export.

## What scripts can do

| Global | Lightloom |
|---|---|
| `system.action(path)` | the action router (`docs/integrations/actions.md`); `signalrgb://` paths are accepted |
| `system.openUrl(url)` | `http`, `https`, `mailto` and `discord:`, `slack:`, `spotify:`, `steam:`, `msteams:`, `zoommtg:` links open with their default program; other schemes (`file:`, `ms-*:`, `search-ms:`, ...) throw `MacroRefused`; `lightloom:` links run as actions |
| `system.openApplication(path, args, startIn)` | starts a program directly (no shell; a single string of arguments is split like a command line), **only** while **Let macros start programs** is on; otherwise `MacroRefused` |
| `system.prompt(options, callback)` | a text dialog in the dashboard; the callback gets the text, or `null` when cancelled or when no dashboard is open |
| `system.copyToClipboard(text)` | the clipboard |
| `system.elevateWindow`, `system.cycleWindow` | not supported (`MacroError`) |
| `system.getForegroundExplorerPath()` | `""` |
| `signalrgb.*` | the 18 methods, mapped onto the engine (`crates/lightloom-macros/src/host.rs`) |
| `keys.press/hold/release(combo or code)`, `keys.paste(text)`, `keys.sendUnicode(text)` | synthetic input (`SendInput`) |
| `mouse.click(button)` | `Left Click`, `Right Click`, `Middle Click`, `Mouse Button 4/5`, `Scroll Up/Down/Left/Right` |
| `block.trigger()` | runs the Input's chain of Actions |
| `console.log/info/warn/error` | the dashboard log (`[macro <name>] ...`) |

Device plugins' `keyboard.sendEvent` / `mouse.sendEvent` (macro keys, extra buttons) reach
Inputs' `onKey` / `onMouse` with `injected: true`; `keyboard.sendHid` / `mouse.sendHid`
press a real key or button, and only while macros are on.

## Events

Every handler gets `{EventHandler, EventType, Sender, SenderType, data}`, with `Data` as a
second name for `data` (scripts use both spellings).

| `EventType` | When | payload |
|---|---|---|
| `Effect Changed` | the effect shown changed | `EffectName` |
| `Layout Changed` | the layout changed | `LayoutName` |
| `Session Lock` | the PC was locked or unlocked | `locked` |
| `System Suspend`, `System Resume` | sleep and wake | none |
| `System Idle Time` | every second | `idleTime` (seconds) |
| `Process Started`, `Process Ended` | a program started or ended (checked every 2 s) | `exeName` |
| key (`onKey`) | keyboard hook or device plugin | `key`, `keyCode`, `released`, `shift`, `control`, `alt`, `win`, `keyString`, `injected` |
| mouse (`onMouse`) | mouse hook or device plugin | `button`, `buttonCode`, `released`, `injected` |

Idle time and processes are polled only while an enabled Input has `onApplication`.
Returning `true` from `onKey` / `onMouse` swallows the key or click (the hook waits at most
40 ms for the answer). Not emitted yet: `System Time`, `System Logoff`, `Foreground App
Changed`, `Audio Capture`, `onSensorUpdate`.

## Keyboard and mouse hooks (privacy)

To react to keys and clicks, Lightloom needs a low-level keyboard or mouse hook, which sees
**every** key pressed and every click in every program. So:

- each hook has its own switch (**Let macros watch the keyboard / the mouse buttons**), off
  by default;
- a hook is installed only while its switch is on, macros are on and an enabled Input has
  `onKey` / `onMouse`; it is removed as soon as one of these stops being true;
- events are processed in memory and handed to those Inputs only. Lightloom never logs or
  sends a key; the console output and error messages of `onKey` / `onMouse` are not logged
  either, because they could hold typed text;
- input Lightloom injects itself is not shown to the hooks, so a macro cannot trigger itself.

**Emergency stop:** `Ctrl + Alt + Shift + F12` (while the keyboard hook is installed) turns
all macros off; turn them on again in the page.

## Limits

- One configuration per script: SignalRGB's macro layers and several blocks using the same
  Input with different options are not supported.
- Windows only for input and hooks; elsewhere scripts run but `keys`, `mouse` and the hooks
  are not available.
- A handler call may use 2 s of JavaScript time (15 s with waiting); each script has a
  32 MiB heap.
- Tests never inject input: they use a recording desktop. The hooks and `SendInput` have not
  been tried on hardware yet.
