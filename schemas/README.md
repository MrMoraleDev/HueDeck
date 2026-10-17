<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# HueDeck JSON Schemas

Machine-readable contracts for the JSON documents HueDeck reads and writes.
All schemas are [JSON Schema draft 2020-12](https://json-schema.org/draft/2020-12)
and carry an SPDX header in `$comment`. They are the single source of truth for
the Rust `serde` types, the TypeScript SDK typings and the dashboard forms; a
change here is a change to the on-disk/API format and needs a note in the
changelog.

| File | Describes | Produced by | Consumed by |
|---|---|---|---|
| `component.schema.json` | One ARGB component descriptor (fan, strip, AIO ...) in the SignalRGB-compatible component JSON format, plus the persisted instance form (`ComponentId`). | Community/first-party `components/*.json`, user files, the dashboard component builder, the import wizard. | Component library loader, channel persistence, layout sampling. |
| `controllable-parameters.schema.json` | The array returned by a plugin's `ControllableParameters()` export and the object passed to `device.addProperty()`. Macro `Options` descriptors share the shape (plus `mousecode` and `{key,label}` values) but their macro-only types (`effectname`, `layoutname`, `effectpreset`, `device`, `playlist`, `sensor`) are outside this schema's `type` enum. | Device plugins (JS). | Plugin host (globals + `on<prop>Changed`), dashboard device settings UI, SDK lint. |
| `effect-meta.schema.json` | The control model parsed from a lightscript's `<head>`: title/description/publisher, `<meta property>` controls, `<meta meter>` screen meters with `<resolution>` overrides, capability flags. | Effect loader (`huedeck-effects`). | Renderer bootstrap, effect settings UI, presets, REST `/lighting/effects/<id>`. |
| `plugin-manifest-v2.schema.json` | `manifest.json` of a native HueDeck plugin: identity, SPDX license, device claims (USB vid/pids, SMBus addresses, network), permissions, entry module, optional `compat: "signalrgb-2.5"`. | Plugin authors. | Plugin loader (capability gating, udev rule generation, index cache). |
| `layout.schema.json` | A named layout: per-endpoint placement on the 320x200 canvas (position, scale, rotation, flips, enabled, brightness, lighting mode, forced colour). | Dashboard layout editor, import wizard, `.hdlayout` export. | Sampling engine, REST `/scenes/layouts`. |
| `plugin-source.schema.json` | One plugin source entry: git repo or local path, ref, hash pin, license acknowledgement timestamp, enabled flag. | Sources manager (pre-listed community repos + user additions). | Source sync/update flow; nothing from a source loads until its license text is acknowledged. |

## Conventions

* `$id` is `https://huedeck.dev/schemas/<file>`; refer to definitions with
  `#/$defs/<name>`.
* Numeric fields that plugins commonly quote (`"min": "100"`, `"default": "false"`)
  are typed with `anyOf` number|string (`numberish`, `booleanish`) in the
  ControllableParameters and effect-meta schemas; the host coerces at load time.
* Formats that are compatible with SignalRGB 2.5 (component JSON, controllable
  parameters, effect `<meta>` tags) keep the original key spelling and casing so
  community files load unchanged. HueDeck-native formats (manifest, layout,
  plugin source) use camelCase/snake_case as documented in each file.
* Invariants JSON Schema cannot express are written in each schema's
  `description` and enforced by the loaders. For components these are:
  `len(LedMapping) == len(LedCoordinates) == len(LedNames) == LedCount`,
  every coordinate inside `Width x Height`, and `LedMapping` being a permutation
  of `0..LedCount-1`. The real corpus violates the last one in one shipped
  file, so loaders warn and keep going instead of rejecting.

## Testing

`tests/schemas/validate_corpus.py` (Python 3, stdlib only, run with `-I`)
parses every schema here, checks a hard-coded mirror of the component schema
against the schema itself and against built-in fixtures, then validates every
component JSON found in the user's local SignalRGB directories (bundled
`Signal-x64\Components`, CDN cache `plugin_cdn\main\Components`, and `cache\addons`
recursively). It prints pass/fail counts and the distinct keys / `Type` / `Brand`
values seen. Hard schema violations fail the run; the documented invariants are
reported as warnings.

```text
python -I tests/schemas/validate_corpus.py --skip-if-missing-corpus
python -I tests/schemas/validate_corpus.py -v D:\some\other\Components
```

The corpus is user-owned and is never copied into this repository; CI runs the
script with `--skip-if-missing-corpus` and the self-hosted runner runs it against
the real files.

To validate against the schemas with a full draft 2020-12 validator (optional,
not required by CI): `npx ajv-cli validate -s schemas/component.schema.json -d file.json --spec=draft2020`.
