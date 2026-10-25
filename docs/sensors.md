# Sensors

Lightloom reads hardware sensors (temperatures, loads, fan speeds, clocks) and hands them to effects through `engine.getSensorValue(name)`, to clients through `GET /v1/sensors`, and to the dashboard (the **Sensors** list under *Devices*).

Sensors are **read-only**. Lightloom never writes a PWM, SuperIO, EC or MSR register, and never sets a fan speed. See [Fan control](#fan-control) below.

## Sources

| Source | OS | Sensors | How |
|---|---|---|---|
| `system` | all | `CPU Load`, `RAM Load` (%), `RAM Used` (GB) | `GetSystemTimes` / `GlobalMemoryStatusEx` on Windows, `/proc/stat` / `/proc/meminfo` on Linux (`crates/lightloom-runtime/src/sensors/system.rs`). CPU load is the busy share of the time between two polls. |
| `nvapi` | Windows | `GPU Temperature`, `GPU Load`, `GPU Memory Load`, `GPU Fan`, `GPU Core Clock`, `GPU Memory Clock`; `GPU 2 …` for a second card | Four public NvAPI queries: `NvAPI_GPU_GetThermalSettings`, `NvAPI_GPU_GetDynamicPstatesInfoEx`, `NvAPI_GPU_GetTachReading`, `NvAPI_GPU_GetAllClockFrequencies` (`crates/lightloom-nvapi/src/sensors.rs`). Each one is optional: a card or driver without it just lacks that sensor. They run on the same `lightloom-nvapi` worker thread as GPU I2C (one process-wide hub, `NvapiHub::shared`). |
| `hwmon` | Linux | every `tempN_input` / `fanN_input`, named `"<chip> <label>"` (`k10temp Tctl`, `nct6687 CPU Fan`) | `crates/lightloom-hal-linux/src/hwmon.rs`, sysfs reads only. |
| `lhm` | any (Windows in practice) | everything LibreHardwareMonitor shows, named `"<hardware> <sensor>"` (`AMD Ryzen 7 5800X Core (Tctl/Tdie)`) | LibreHardwareMonitor's `data.json` (`crates/lightloom-runtime/src/sensors/lhm.rs`). Optional, see below. |

The registry (`lightloom_runtime::sensors::SensorRegistry`) merges the sources in that order, makes names unique (` #2`, ` #3`), drops non-finite values, and adds **`CPU Temperature`**: a copy of the best CPU package/die temperature it finds (`Tctl/Tdie`, `CPU Package`, `Package id 0`, `k10temp Tctl`, `zenpower Tdie`, …). A source that fails is listed with its error in `GET /v1/sensors` and logged once.

### CPU temperature on Windows

Windows has no unprivileged API for the CPU temperature. Lightloom reads it from **LibreHardwareMonitor** when you run it:

1. In LibreHardwareMonitor: *Options → Remote Web Server → Run* (port 8085 by default).
2. In Lightloom's `config.toml`:

   ```toml
   [sensors]
   lhm_url = "http://127.0.0.1:8085/data.json"
   ```

Only loopback URLs (`127.0.0.1`, `localhost`, `::1`) are accepted: Lightloom never polls another machine. Each poll is one HTTP/1.0 GET with a 500 ms timeout and a 4 MiB cap. Values with a decimal comma (`"45,3 °C"`) parse.

Without LibreHardwareMonitor there is no `CPU Temperature` on Windows (GPU temperature, CPU load and RAM load still work).

**Not implemented: reading the CPU's thermal MSRs through the I/O broker.** PawnIO's signed `AMDFamily17` and `IntelMSR` modules can read the temperature MSRs (`Tctl` through SMN on AMD family 17h+, `IA32_PACKAGE_THERM_STATUS` / `IA32_TEMPERATURE_TARGET` on Intel). Doing this from Lightloom needs a new read-only broker operation, a pin of each module's hash next to the SMBus module's, and per-family decoding (Tctl offsets, TjMax). It stays future work: the broker today exposes only the chipset SMBus, and widening a LocalSystem service is not worth it while LibreHardwareMonitor covers the case.

## When sensors are read

Nothing is polled unless something reads sensors:

- **An effect**: the active effect's page calls `getSensorValue` or reads `engine.sensors` (its parsed `uses.sensors`). The values are pushed to the renderer after every poll.
- **An LCD face**: a face showing on a device's LCD reads sensors the same way (`docs/host-api/lcd-lightloom.md`). The values go to that face's renderer.
- **A macro**: an enabled macro Input with `onSensorUpdate` (while macros are on) gets every poll's values (`docs/macros.md`).
- **A client**: each `GET /v1/sensors` keeps polling on for 10 s. The dashboard's Sensors list asks every 2 s while it is shown and the page is visible.

The poller (`crates/lightloom-daemon/src/sensors.rs`, thread `lightloom-sensors`) polls at most once a second (`[sensors] interval_ms`, at least 1000) and sleeps otherwise. The sources, NvAPI included, are opened at the first poll, not at daemon start. The first `GET /v1/sensors` waits up to 1.5 s for that first poll.

Without hardware (`--no-hardware`, tests), NvAPI is never opened. The system load and a configured LibreHardwareMonitor URL still work.

## `config.toml`

```toml
[sensors]
enabled = true          # false: nothing is read; effects see null, GET /v1/sensors says enabled: false
interval_ms = 1000      # at least 1000
nvapi = true            # NVIDIA GPU sensors (Windows)
# lhm_url = "http://127.0.0.1:8085/data.json"
```

## Effects: `engine.getSensorValue(name)`

```js
const s = engine.getSensorValue("CPU Temperature");
// { value: 52.3, units: "°C", min: 0, max: 100, type: "Temperature" }, or null when unknown
```

This follows `docs/host-api/effects-engine.md` §1.6. The renderer injects `engine.sensors = {}` and `getSensorValue(name) → engine.sensors[name] ?? null` (`crates/lightloom-renderer/src/inject.rs`). The daemon pushes `{"sensors": {name: {value, units, min, max, type}}}` as an `engine` control command, the same way it pushes audio and screen data, and the renderer merges it into `engine.sensors`. A sensor that disappears keeps its last value.

- `type` is one of `Temperature`, `Fan`, `Voltage`, `Load`, `Clock`, `Power`, `Other`. SignalRGB's exact strings are unknown; these are Lightloom's.
- `min` / `max` give the expected range for scaling, not observed extremes. Examples: 0-100 for a temperature unless the source says otherwise, 0-100 for a load, NvAPI's own range for the GPU temperature.
- An effect property of `type="sensor"` holds a sensor name. It renders as a text field for now, so type the name exactly as `GET /v1/sensors` lists it.
- Values arrive about one second after the effect starts (sooner when sensors were already being polled).

## API

`GET /v1/sensors` takes no token. See `docs/api/openapi.yaml` (`SensorsView`).

```json
{
  "enabled": true, "polling": true, "intervalMs": 1000, "ageMs": 240,
  "sensors": [{ "name": "CPU Load", "value": 12.5, "units": "%", "min": 0, "max": 100, "type": "Load" }],
  "sources": [{ "name": "system", "sensors": 3, "error": null }, { "name": "nvapi", "sensors": 6, "error": null }]
}
```

## Plugins

The plugin contract has no call for reading host sensors. Its fan and temperature members (`createFanControl`, `setRPM`, `getFanlevel`, `createTemperatureSensor`, `SetTemperature`, …; `docs/host-api/components-channels.md` §5) belong to fan control: plugins create them when the host offers fan control. Lightloom keeps them as no-ops and answers `device.fanControlDisabled() === true`. Plugins then skip their fan and probe state machines, and none sends a fan command. Temperatures that plugins report (an AIO's liquid temperature) therefore do not reach the registry.

## Fan control

Fan control is **out of scope** and will be a separate, **opt-in** feature. That covers PWM through SuperIO/LpcIO (NCT6687D and others) through the broker, hwmon `pwm*` on Linux, fan curves bound to a sensor, and the plugin fan API with its hard-lock policy. Until then:

- no source opens a write path; `hwmon` never touches `pwm*`, and NvAPI cooler calls are not resolved;
- `fanControlDisabled()` stays `true` and the plugin fan API stays a no-op.

## Tests

All tests use fakes; none touches a driver.

- NvAPI layouts, versions, decoding, naming, and the worker path through `FakeNvI2c::with_sensors` (`lightloom-nvapi`).
- System load from scripted samples, and `/proc` parsing.
- The LibreHardwareMonitor tree, values, URL policy, chunked replies, and a poll against a local TCP server.
- Registry naming and the `CPU Temperature` alias.
- Poller demand: idle until read, at most 1 Hz, effect pushes, disabled.
- `GET /v1/sensors` and the OpenAPI contract.

Untested on hardware: the four NvAPI queries on a real card, and LibreHardwareMonitor's real server.
