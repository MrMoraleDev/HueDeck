# Action URLs (`lightloom://`)

Lightloom's action router runs short commands written as URLs, so that stream
decks, keyboard launchers, shortcuts, scripts and web pages can drive the
lighting without the full REST API. The paths are the ones of SignalRGB's
`signalrgb://` / `system.action()` router (docs/host-api/macros-system.md §2),
under Lightloom's own scheme:

```text
lightloom://<endpoint>/<verb>[/<argument>][?key=value&...]
```

There are three ways to run an action:

* `POST /v1/actions` with `{"url": "lightloom://..."}` on the native API. It needs the
  bearer token, like every request that changes state ([OpenAPI](../api/openapi.yaml)).
* `lightloom action <url>` from the command line. It reads the token from the data
  directory and the port from the `api-port` file, and `--json` prints the daemon's answer.
* Opening a `lightloom://` link, once the URL scheme is registered (see below).

A bare path (`brightness/50`, the form `system.action()` takes) works
everywhere a URL does.

## Grammar

| URL | What it does | Underlying request |
|---|---|---|
| `app/raiseWindow` | Brings the dashboard to the front (an `action` event; the page calls `window.focus()`) | event |
| `app/restart` | Restarts the engine in place: devices, plugins, effects and the database state are reloaded. The process and the API stay up. | the reload behind `POST /v1/import` |
| `app/toast?title=<t>&message=<m>` | Shows a system notification and a dashboard banner. Needs a title or a message; `sender` and `_silent` are ignored. | notifier + event |
| `view/<page>[?uid=<device key>]` | Switches the dashboard to the page's tab; with `uid`, scrolls to that device | event |
| `layout/apply/<name>` | Switches to a saved layout | `ApplyLayout` |
| `layout/applyprevious` | Goes back to the layout shown before the current one (in this run; `409` before any switch) | `ApplyLayout` |
| `effect/apply/<name>?<property>=<value>&...` | Shows an effect. Each property is set (and stored) on the matching effect control. | `ApplyEffect` |
| `effect/install/<x>`, `effect/update/<x>` | Refused with `409 action_refused`: Lightloom has no effect marketplace | none |
| `device/<uid>/enable`, `device/<uid>/disable` | Drives the device or leaves it alone (its key is matched case-insensitively) | `SetDevice` |
| `playlist/<id>/play` | Plays a playlist from its first entry | `PlayPlaylist` |
| `playlist/<id>/stop` | Stops it, if it is the one playing (`stopped: false` otherwise) | `StopPlaylist` |
| `brightness/<0-100>` | Sets the global brightness | `SetLighting` |
| `canvas/pause`, `canvas/resume` | Pauses the canvas (every device black) or resumes it | `SetLighting` |

`view/` pages map to dashboard tabs as follows:

* `dashboard`, `customize`, `effects`, `library` → Effects;
* `layouts`, `layout` → Layout;
* `devices`, `showdevice` (which needs `uid`), `network` → Devices;
* `settings[/<sub>]`, `addons`, `debuginfo`, `logs` → Settings.

`cooling`, `monitoring`, `macros` and `dumps` have no Lightloom page, so they are dropped.

### Effects and their properties

* **Finding the effect:** `effect/apply/<name>` matches the effect's id first
  (`lightloom:rainbow`), then its id without the source (`rainbow`), then its title
  (`Rainbow Rise`, case-insensitive).
* **Converting values:** query values are converted strictly to the control's type.
  * A `number` or `hue` must be a finite number within the control's `min`..`max`
    (`hue`: 0..360).
  * A `boolean` must be `true`, `false`, `1` or `0`.
  * A `color` must be `#rrggbb`.
  * A `combobox` value must be one the control lists.
  * Anything else is `400 invalid_action`.
* **Ignored keys:** properties the effect does not have are ignored, and the reply lists
  them in `result.ignored`. Keys starting with `_` (`_location`, `_silent`,
  `_sessionId`, `_timestamp`) and `sender` / `version` are SignalRGB bookkeeping and are
  always ignored.

Example: `lightloom://effect/apply/Rainbow%20Rise?reverse=false&speed=3`.

### Strict parsing

The URL is rejected with `400 invalid_action` (and `lightloom action` exits with an error
before contacting the daemon) when:

* it is empty, longer than 2048 bytes, or has spaces or control characters (percent-encode them);
* it uses another scheme (`http:`, `signalrgb:`, `javascript:`, ...);
* it has an empty path segment (`app//raiseWindow`);
* it has a bad percent escape, or decodes to invalid UTF-8 or a control character;
* a query key is empty or given twice;
* a known action has a missing or extra argument (`layout/apply`, `app/restart/now`,
  `device/<uid>` without a verb);
* `brightness` is not a whole number from 0 to 100;
* `app/toast` has no title or message;
* `view/showdevice` has no `uid`.

The parser also accepts or ignores a few things:

* endpoint and verb names are case-insensitive (`app/raisewindow`);
* one trailing `/` is accepted, because browsers add it;
* a `#fragment` is ignored;
* in names, `%2F` stands for a `/` inside one argument (`layout/apply/A%2FB`);
* in the query, `+` is a space.

### Unknown actions

A well-formed path the router has no handler for is **dropped with a warning**, the way
SignalRGB ignores paths it does not know. Examples are `addon/install`, `app/update`,
`app/autominiplayer`, another verb of a known endpoint, or a page without a Lightloom
counterpart. The daemon logs the warning and answers `200`:

```json
{ "action": "unknown", "dropped": true, "warning": "no action endpoint \"addon\"" }
```

## Replies

A successful action answers with `{"action": "<canonical name>", "result": <the
underlying request's answer>}`. For `app/raiseWindow`, `view` and `app/toast`, the
`result` is `{"delivered": <dashboards that got the event>}`; a toast also has
`notified` (the system notification was shown). Errors use the usual
`{"error": {status, code, message}}` body:

* `400 invalid_action`;
* `404 not_found` (no such effect, layout, device or playlist);
* `409 action_refused` or `409 conflict`.

## Dashboard events

`app/raiseWindow`, `view/...` and `app/toast` reach dashboards as an `action` event
on `/v1/events`:

```json
{ "kind": "action", "data": { "action": "view", "page": "showdevice", "tab": "devices", "uid": "1234:4d4d:D" } }
{ "kind": "action", "data": { "action": "raiseWindow" } }
{ "kind": "action", "data": { "action": "toast", "title": "Hi", "message": "There" } }
```

The dashboard switches to `tab`, scrolls to the device card when there is a `uid`,
focuses its window, and shows toasts as a banner for a few seconds. A browser may
refuse to bring a background tab to the front, so `raiseWindow` is best effort.

## Registering the URL scheme (opt-in)

Lightloom never registers `lightloom://` by itself. Turn it on in **Settings → lightloom://
links**, or with `PATCH /v1/settings {"urlScheme": true}`. The daemon then writes one
per-user key, which needs no administrator rights:

```text
HKEY_CURRENT_USER\Software\Classes\lightloom
    (Default)    = "URL:Lightloom action"
    URL Protocol = ""
    DefaultIcon\(Default)        = "<dir>\lightloom.exe",0
    shell\open\command\(Default) = "<dir>\lightloom.exe" action "%1"
```

`<dir>` is the folder of `lightloomd.exe`; the setting is unavailable when `lightloom.exe`
is not next to it.

* **Opening a link:** Windows runs `lightloom action "<url>"`, which hands the URL to the
  running daemon. A console window may flash.
* **Moving the install:** while the setting is on, the daemon checks the key at startup
  and writes it again if the client has moved (a portable copy run from a new folder).
* **After an import:** a backup imported with the setting on shows a notice until the
  setting is turned off and on.
* **Turning it off:** the key and everything under it are deleted. Deleting
  `HKCU\Software\Classes\lightloom` by hand does the same.
* **Errors:** if the scheme is unavailable, the answer is `409 conflict`. If the registry
  refuses the write, it is `500 url_scheme_failed`, and the setting stays as it was.
* **Ephemeral runs:** `--ephemeral` runs never write the registry.

On Linux and macOS registration is not available yet. A `.desktop` handler or an
`Info.plist` entry would play the same part. `lightloom action` and `POST /v1/actions`
work everywhere.

Security: an action URL can do what the dashboard's buttons can, except save to device
memory, change settings or stop the daemon. Opening a link from a web page asks
for the user's confirmation in every browser before Windows runs Lightloom.

## Not covered here

Macro scripts (SignalRGB's Macroscripts Actions/Inputs runtime, `keys`/`mouse`,
low-level keyboard and mouse hooks) are WP-MACROS part (b), see docs/status.md.
