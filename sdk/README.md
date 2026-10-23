<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom plugin SDK

Everything you need to write a Lightloom device plugin without touching hardware:

| Part | What it is |
|---|---|
| [`lightloom.d.ts`](lightloom.d.ts) | TypeScript typings of the plugin host API: `device`, `bus`, channels, subdevices, feature globals, `@SignalRGB/*` and `@lightloom/*` modules, plugin exports, and the `engine` object of effects. Members are tagged `@signalrgb` (SignalRGB-compatible), `@lightloom` (Lightloom extension), `@unimplemented` (throws `HostApiMissing` in Lightloom today) or `@stub`. |
| [`lint/`](lint/) | `lightloom-lint`: checks a plugin directory (manifest, exports, forbidden globals). |
| [`simulator/`](simulator/) | `lightloom-sim`: runs a plugin against mock hardware with a solid colour and prints every HID report or SMBus transfer it sends. |
| [`examples/hello-strip/`](examples/hello-strip/) | The plugin this page builds. |

Requirements: Node 24 or later. No npm packages: the tools use the Node standard library and
the mocks in `plugins/_testkit/`, so run them from a Lightloom checkout.

The reference behind it all is `docs/host-api/*.md` (the SignalRGB-era plugin contract) and the
"Lightloom" sections there for the extensions.

## A plugin in 10 minutes

A plugin is one ES module plus a `manifest.json`, in its own directory. Lightloom plugins use the
same exports and globals as SignalRGB plugins, so most knowledge carries over.

### 1. The manifest (1 minute)

`manifest.json` says which hardware the plugin may touch and what it needs, before any code
loads ([schema](../schemas/plugin-manifest-v2.schema.json)):

```json
{
  "manifestVersion": 2,
  "id": "example.hello-strip",
  "name": "Hello Strip",
  "version": "0.1.0",
  "license": "MIT",
  "publisher": "Lightloom SDK example",
  "entry": "plugin.js",
  "compat": "signalrgb-2.5",
  "deviceType": "lightingcontroller",
  "devices": [{ "vid": "0x1209", "pids": ["0x0001"], "transport": "hid" }],
  "permissions": ["hid"]
}
```

Third-party plugins may use any licence (`LICENSE-EXCEPTION.md`). Keep `tested` out of the
claim until someone has run the plugin on the real device.

### 2. Metadata exports (2 minutes)

Each export may be a value or a function; the host calls functions once when it scans:

```js
/// <reference path="../../lightloom.d.ts" />
const LEDS = 8;

export function Name() { return "Hello Strip"; }
export function Publisher() { return "Lightloom SDK example"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return [0x0001]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [LEDS, 1]; }                     // grid in LED cells
export function DefaultScale() { return 8.0; }                   // canvas pixels per cell
export function LedNames() { return Array.from({ length: LEDS }, (_, i) => `LED ${i + 1}`); }
export function LedPositions() { return Array.from({ length: LEDS }, (_, i) => [i, 0]); }
```

`LedNames()` and `LedPositions()` have the same length, and every position lies inside `Size()`.

### 3. Settings (1 minute)

Each `ControllableParameters()` entry becomes a global of the same name, set before
`Initialize()` and updated live (with an optional `on<property>Changed()` hook):

```js
export function ControllableParameters() {
  return [
    { property: "shutdownColor", group: "lighting", label: "Shutdown Color", type: "color", default: "#000000" },
    { property: "LightingMode", group: "lighting", label: "Lighting Mode", type: "combobox", values: ["Canvas", "Forced"], default: "Canvas" },
    { property: "forcedColor", group: "lighting", label: "Forced Color", type: "color", default: "#009bde" },
  ];
}
```

### 4. Lifecycle (4 minutes)

```js
// Called for every HID collection of the matched device; accept the one you drive.
export function Validate(endpoint) {
  return endpoint.interface === 0 || endpoint.interface === -1;
}

export function Initialize() {
  device.setName("Hello Strip");
}

// Every frame: sample the canvas at each LED and send one report.
export function Render() {
  sendColors();
}

// On stop, unplug and sleep: hand the device a defined state.
export function Shutdown(systemSuspending) {
  sendColors(systemSuspending ? "#000000" : shutdownColor);
}

function sendColors(overrideColor) {
  const packet = [0x00, 0x01, LEDS];            // report id 0, command, count
  for (let i = 0; i < LEDS; i++) {
    const color = overrideColor ? hexToRgb(overrideColor)
      : LightingMode === "Forced" ? hexToRgb(forcedColor)
      : device.color(i, 0);                       // [r, g, b] at LED cell (i, 0)
    packet.push(...color);
  }
  device.write(packet, 65);                       // zero-padded to 65 bytes
}
```

Rules the host enforces or expects:

- Reports start with the report id (`0x00` when the device has none); `write` / `send_report`
  pad with zeros to `length`, return -1 on failure and never throw.
- No DOM, no Node, no timers: use `device.pause(ms)` or count `Render()` calls.
- `Render()` gets 250 ms of script time; time in `device.pause` and I/O is not counted.
- Send only lighting commands. Never write flash or EEPROM, never touch fans, DPI or audio.

The full example, with JSDoc types, is [`examples/hello-strip/plugin.js`](examples/hello-strip/plugin.js).

### 5. Test it with the simulator (1 minute)

```console
$ node sdk/simulator/cli.mjs sdk/examples/hello-strip --color "#FF8000" --frames 1
Hello Strip: Hid 1209:0001, canvas #FF8000
Validate(interface 0, usage 0x0001, usage_page 0xFF00, collection 0) -> true
  (nothing sent)
Initialize()
  setName("Hello Strip")
Render() #1
  write          65 B  00 01 08 FF 80 00 FF 80 00 ... 00
  (8 canvas samples)
Shutdown(false)
  write          65 B  00 01 08 00 00 00 ... 00
ok
```

Options: `--color #RRGGBB`, `--frames N`, `--param name=value` (overrides a parameter),
`--pid 0x...` (pick one of several product ids), `--endpoint iface:usage:usagePage[:collection]`
(the collections `Validate()` is offered; repeatable), `--suspend` (`Shutdown(true)`),
`--module file.js` (another plugin module of the directory), `--json`.
A plugin that adds the `battery` feature gets a `battery:` line with the last level and state
it set.

SMBus plugins get `Scan(bus)` on a free-address mock bus and then a session on the fixed bus
bound to the first address `Scan()` returned. By default every address of `SmbusAddresses()`
answers and unset registers read `0xFF`; describe your device with `--bus bus.json`, which
takes the options of `plugins/_testkit/mock-bus.mjs` `createMockBus()`:

```console
$ echo '{"kind": "intel", "registers": {"57": {"0": 165}}}' > bus.json
$ node sdk/simulator/cli.mjs sdk/lint/tests/fixtures/good-smbus --bus bus.json --color "#102030"
Good SMBus: SMBus (intel bus, address 0x39), canvas #102030
Scan(bus) -> [57]
  ReadByte                 @0x39 reg 0x00
...
Render() #1
  WriteByte                @0x39 reg 0x10 <- 0x10 [locked]
...
```

The exit code is 1 when a lifecycle call threw or nothing was found. For assertions on exact
bytes, write `node:test` tests with `plugins/_testkit/mock-device.mjs` and `mock-bus.mjs`, as
the first-party plugins do (`plugins/*/tests/`).

### 6. Lint it (1 minute)

```console
$ node sdk/lint/cli.mjs sdk/examples/hello-strip
hello-strip (example.hello-strip 0.1.0)
  ok
```

The lint checks:

- **manifest**: the v2 schema; `tested` ids among the claimed ones; the entry file exists;
  SMBus addresses inside 0x03..0x77; a RAM plugin declares `spdManufacturers`; permissions
  that match the claims;
- **exports**: the required exports per plugin type (`Name`, `Initialize`/`Render`/`Shutdown`,
  `VendorId`/`ProductId` or `HidUsages`, `Scan` and `SmbusAddresses` for SMBus); exports that
  throw; `Type()`; `LedNames()`/`LedPositions()` lengths, positions inside `Size()`, shared
  cells; `ControllableParameters()` against its schema, duplicates, parameters that would
  replace a host global, combobox defaults;
- **exports against the manifest**: `VendorId()`/`ProductId()` of every plugin module in the
  directory against the USB claims, `HidUsages()` against `hidUsage` claims,
  `SmbusAddresses()`, `SmbusDeniedRegisters()`, `SpdManufacturers()`, `SmbusBatchWrites()`,
  `BrandGPUList()` and `SmbusBuses()` against the SMBus claim, `deviceType` and `version`;
- **sources**: DOM (`window`, `document`, `fetch`, ...), Node (`require`, `process`, ...) and
  timer (`setTimeout`, ...) globals, outside comments, strings and `typeof` guards.

Errors make the exit code 1; `--strict` counts warnings too; `--json` prints the findings.
A directory of plugins (`node sdk/lint/cli.mjs plugins`) lints each one. The first-party
plugins pass it in CI (`plugins/_testkit/manifests.test.mjs`).

### Type-checking (optional)

`lightloom.d.ts` declares the globals and modules; reference it from the plugin and run
TypeScript in `checkJs` mode. Parameter globals need a declaration of their own (see
[`examples/hello-strip/params.d.ts`](examples/hello-strip/params.d.ts)):

```console
$ (cd ui && npm ci --ignore-scripts)        # provides tsc
$ ui/node_modules/.bin/tsc -p sdk           # sdk/tsconfig.json: the d.ts, the example, types-test/
```

## Then on real hardware

- `lightloom compat dry-run <parent-dir> --initialize` loads the plugins of a directory in the
  real QuickJS host with recording stubs (no hardware) and lists every host member they use
  that Lightloom lacks (`API_GAP`).
- `lightloom light "#FF0000" --plugins <parent-dir> --device 1209:0001 --dry-run` opens the
  device, reads, and prints the packets it would send without writing; drop `--dry-run` to
  drive it. For SMBus plugins `--dry-run` records every bus write instead of sending it.
- For the daemon, put the plugin directory in the user plugin directory
  (`%APPDATA%\Lightloom\plugins` on Windows) or list its parent in `config.toml`
  `[plugins] dirs`.

## Tests of the SDK itself

```console
$ node --test sdk/lint/tests/lint.test.mjs sdk/simulator/tests/simulate.test.mjs
```
