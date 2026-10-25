# Sleep, resume, lock and display off

The daemon follows the system's power events (`lightloom_hal::power`). On Windows they come from a
hidden window (`lightloom_hal_windows::power`). The window gets:
- `WM_POWERBROADCAST`: `PBT_APMSUSPEND`, `PBT_APMRESUMEAUTOMATIC`, `PBT_APMRESUMESUSPEND`, and
  `PBT_POWERSETTINGCHANGE` for `GUID_CONSOLE_DISPLAY_STATE`;
- `WM_WTSSESSION_CHANGE`: lock and unlock.

Linux has no source yet: logind's `PrepareForSleep` over D-Bus is still to do. Mock runs (`--no-hardware`, fixed device
lists) subscribe no source. Tests drive the engine through `ManualPower` and `Shared::power_sink`.

## Sleep

Windows waits for the window procedure before it sleeps, for about two seconds. The engine therefore handles the event
before the handler returns, and waits at most 2.5 s for it. The engine does the following, in order:
1. Lifecycle state `Suspending`.
2. Every session (HID, SMBus, LampArray, network) gets `Shutdown(true, mode)`, all at once, within 1.5 s.
   - `true` is SignalRGB's `SystemSuspending`: most plugins leave their device dark.
   - `mode` is the exit mode (`restore`, `keep`, `off`), with per-device overrides, as when Lightloom closes.
   - A session that does not finish in time keeps its device until it ends: no new session starts for that device.
3. The network discovery services stop.
4. The SMBus buses are released: the I/O broker pipe and its lease, the NvAPI hub, and the one-process SMBus claim.
5. The renderer and the audio and screen captures stop.

While suspended, the engine scans nothing and starts nothing, and hotplug is ignored. Requests are still answered: an
effect chosen meanwhile is stored and shown on resume.

## Resume

1. Lifecycle state `Resuming`.
2. The renderer starts again, on the stored effect.
3. Devices are enumerated again and the SMBus buses are scanned again. Every session starts over with `Initialize()`.
4. The network services start again.
5. One more rescan follows shortly after, for devices that come back late. Hotplug catches the rest.
6. Lifecycle state `Running`.

Every event is idempotent. A second sleep while suspended does nothing, and so does a resume while running: Windows sends
both resume messages on a user wake.

## Lock and display off

The setting **Lights off when the PC locks or the screen turns off** (`lightsOffWhenIdle`) is off by default. With it
on, the canvas turns black while the session is locked or the display is off. The sessions keep running, so unlocking
brings the effect back at once. A dimmed display counts as on.
