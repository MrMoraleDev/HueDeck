# SignalRGB import

The import wizard (Settings > SignalRGB, `GET`/`POST /v1/import/signalrgb`, see
[`api/openapi.yaml`](api/openapi.yaml)) copies a local SignalRGB setup into Lightloom's
database. It reads SignalRGB's settings under `HKCU\Software\WhirlwindFX\SignalRgb`
(Qt `QSettings`), read-only, and never writes them. `GET` is a preview: it reads the
same data, reports it, and changes nothing. SignalRGB's effect and plugin files are
not copied. They stay third-party files in SignalRGB's folders, are used only while the
SignalRGB option is on, and are flagged `thirdParty` in the API (`EffectView`,
`DeviceView`) and in the dashboard.

Code: `crates/lightloom-runtime/src/signalrgb.rs` reads the setup (`read_setup`, through
a `SettingsSource`; tests use a `Fixture`). `crates/lightloom-daemon/src/engine.rs`
(`import_signalrgb`) and `store.rs` (`merge_effects`) write it.

| SignalRGB setting | Lightloom |
| --- | --- |
| `lighting\endpoint\<uid>` (`position`, `scale`, `rotation`, `flipped`, `flippedV`, `enabled`, `brightness`) | the placements of a new layout (default name `SignalRGB`), which becomes the current layout |
| `layouts` `brightness`, `gridSize` | that layout's brightness and grid |
| `lighting\endpoint\<vid:pid:serial>` `enabled`, `brightness` | per-device enable and brightness: the placement, and for an `enabled` value on a device Lightloom has seen, whether Lightloom drives it |
| `lighting` `globalBrightness` | the global brightness |
| `devices\<key>\<channel>` (JSON arrays) | the channel's components |
| `effects\<id>\<property>` | the stored control values of `signalrgb:<id>` (a `.html` suffix is dropped) |
| `effects\<id>\<preset>\<property>` (subkeys) | that effect's presets |
| `effects\selected` `id` | the current effect, if it is in the library |

Value types: QColor `@Variant(...)` streams (Qt 5 type 67 and Qt 6 type `0x1003`, RGB,
HSV, CMYK and HSL specs) become `#rrggbb`; `true`/`false` become booleans; numeric text
becomes numbers. When the effect is in the library, each value is coerced to its
control's type and values without a control are dropped. Otherwise (the SignalRGB
option is off) they are stored as read and apply once the effect is available. Values
whose name starts with `-` are SignalRGB's own bookkeeping and are skipped. Other Qt
variants (`@ByteArray`) are skipped with a note.

Not confirmed on a real installation: where SignalRGB keeps saved presets. They are
read from subkeys of the effect's key, and none exist on the reference machine.
`globalBrightness` is a documented setting that the reference machine has never written.
