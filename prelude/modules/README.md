<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# `@SignalRGB/*` module resolution

Every plugin context created by `huedeck-plugin-host` (one rquickjs runtime per
plugin OS thread) gets a module resolver that accepts the bare specifiers
SignalRGB plugins import, verbatim, plus `@huedeck/*` aliases for the same
names. Nothing in this directory is copied from SignalRGB; the member lists
were derived from how the user's local plugin corpus calls them (counts are in
each file header, survey of 2026-10-17 over 1006 plugin `.js` files).

## Resolution table

| Specifier (both `@SignalRGB/` and `@huedeck/` prefixes) | Resolves to | Backing |
|---|---|---|
| `Errors.js`, `Errors` | `prelude/Errors.js` | pure JS (`Assert`, `ContextError`, `globalContext`) |
| `base64` | `prelude/base64.js` | pure JS (`encode`, `decode`, plus `Encode`/`Decode`) |
| `BinaryUtils` (`@huedeck/` only) | `prelude/BinaryUtils.js` | pure JS (HueDeck extension; imported plugins inline their own) |
| `createColorArray` (`@huedeck/` only) | `prelude/createColorArray.js` | pure JS (also bound as `device.createColorArray`) |
| `hexToRgb` (`@huedeck/` only) | `prelude/hexToRgb.js` | pure JS (HueDeck extension) |
| `udp` | native module if registered, else `prelude/modules/udp.js` | Rust (tokio sockets, M6) |
| `tcp` | native module if registered, else `prelude/modules/tcp.js` | Rust (M6) |
| `serial` | native module if registered, else `prelude/modules/serial.js` | Rust (`serialport`, M2) |
| `lcd` | native module if registered, else `prelude/modules/lcd.js` | Rust (renderer LCD frame sink, M6) |
| `permissions` | device sessions: `huedeck:native/permissions.js` (host.js `nativeModules`), else `prelude/modules/permissions.js` | host (every partner permission granted; no partner gate) |
| `systeminfo` | device sessions: `huedeck:native/systeminfo.js` (host.js `nativeModules`), else `prelude/modules/systeminfo.js` | Rust (SMBIOS table: Windows `RSMB`; Linux DMI, M7) |
| `DeviceDiscovery` | native module if registered, else `prelude/modules/DeviceDiscovery.js` | Rust (registry, M2) |
| `performance` | native module if registered, else `prelude/modules/performance.js` | Rust |
| `appInfo` | native module if registered, else `prelude/modules/appInfo.js` | Rust |

Resolution rules the host implements:

1. Strip the prefix (`@SignalRGB/` or `@huedeck/`), then strip an optional
   `.js` suffix. Match the remaining name **case-sensitively** against the table
   (SignalRGB plugins write `DeviceDiscovery`, `Errors.js`, `lcd`, `systeminfo`
   exactly as shown). Rows marked `@huedeck/` only are HueDeck extensions with
   no SignalRGB counterpart: they must not resolve under `@SignalRGB/` (that
   would let a plugin written against HueDeck load here but fail on the
   original host); under `@SignalRGB/` they fall through to rule 4.
2. Pure-JS entries are loaded from `prelude/` as real ES modules. Each of those
   files is dependency-free (no imports), so the loader may evaluate them in any
   order and cache one instance per runtime.
3. For Rust-backed entries the host first asks its native module registry. When
   the native module exists it is returned and the facade here is never
   touched. Otherwise the facade file is loaded; it imports
   `../HostApiMissing.js` (the only relative import inside `prelude/`), so the
   loader must resolve relative specifiers among prelude files.
4. Any other `@SignalRGB/<x>` specifier is an API gap: the loader logs
   `API_GAP:<plugin>:import:@SignalRGB/<x>` once and throws `HostApiMissing`
   from the import, which fails that plugin's module evaluation with a clear
   message instead of a generic "module not found".
5. Relative specifiers inside a plugin (`./Foo.js`) resolve inside the plugin's
   own directory and never into `prelude/`.

## Facade behaviour

Each facade default-exports a frozen-shape object produced by
`createFacade(moduleName, members, options)` from `prelude/HostApiMissing.js`:

- every listed member is a function that throws `HostApiMissing` (fields
  `name: "HostApiMissing"`, `code: "API_GAP"`, `module`, `member`, `plugin`,
  `hint`) and logs `API_GAP: <module>.<member> ...` once per
  plugin/module/member through `__huedeck.log`, `device.log`, `service.log`
  or `console.warn`, whichever exists first;
- any unlisted string property also yields such a throwing function, so a
  member we did not know about still fails loudly on call; `appInfo` uses
  `accessThrows: true` because its members are fields, not methods;
- `then`, `toJSON`, `constructor` and a few other probe keys stay `undefined`
  so Promise resolution, `JSON.stringify` and console printing keep working;
- assignments are allowed (plugins occasionally monkey-patch module objects).

Importing a facade never throws; only using it does. That mirrors the
original host, where a plugin that imports `@SignalRGB/lcd` but only calls it
inside an `if (hasLCD)` branch still loads.

## Feature globals (not modules)

`device.addFeature("udp" | "base64" | "dtls" | "keyboard" | "mouse" | "battery")`
installs a *global* of the same name in device context. `prelude/base64.js`
exports `createFeatureGlobal()` for the `base64` feature (`Encode`/`Decode`
capitalised). The other feature globals are Rust-backed and follow the same
API_GAP policy through the host's `device` Proxy, not through this directory.

## Host globals expected by the prelude

- `globalThis.__huedeck = { plugin: "<plugin name or file>", log(level, msg) }`
  (optional; set by the host before evaluating the plugin so errors and
  log-once keys name the plugin);
- `globalThis.device` / `globalThis.service` with a `log()` method, when the
  context is a device or service context (used as fallback log sinks);
- `globalThis.console` (always present in the host).

## Tests

`node --test "prelude/tests/*.test.mjs"` (Node 24; positional arguments are
glob patterns, a bare directory is not expanded) covers every file in
`prelude/` without a host. `prelude/package.json` only sets
`"type": "module"` and the same `test` script for that purpose.
