<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# huedeckd and the dashboard on the reference machine (M4)

Run of 2026-10-18: Windows 11 Pro, release build of `huedeckd`, `huedeck-renderer`
(CEF 154) and the dashboard, data and configuration under one `--home` directory.
SignalRGB was closed for the hardware part (`taskkill /PID`, relaunched
afterwards); `config.toml` listed the DualSense under `[devices] ignore` (its
plugin also sets headphone and speaker volumes).

## Without hardware (`--no-hardware`, SignalRGB running)

- `GET /v1/health`, `/v1/state`: running, 13 starter effects, the CEF renderer
  painting the default effect at 30 fps.
- Mutations without the token: 401. With a foreign `Host`: 403 `forbidden_host`.
- `POST /v1/effects/huedeck:plasma/apply` switched the renderer to Plasma; the
  dashboard's preview followed.
- SignalRGB listens on `[::]:16038` and `[::]:16034`. A first build still bound
  `127.0.0.1` on both (Windows allows it because SignalRGB's sockets use address
  reuse) and would have answered SignalRGB's IPv4 clients; the compatible listeners
  now read the TCP listener table and wait (`port 16038 is taken ... waits for it`).
- `POST /v1/shutdown` ended the process with exit code 0.

## On hardware

1. `huedeckd --dry-run`: the MSI board (`1462:7c91`) ran its HueDeck plugin,
   111 frames, 0 HID errors, nothing written.
2. Live, dashboard in Chrome:
   - the MSI board ran with HueDeck's own plugin at 30 fps, 0 errors;
   - **Settings → Use my SignalRGB plugins** rebuilt the plugin catalog and started
     the SteelSeries Arctis 5, Razer DeathAdder V2 Pro and NZXT Kraken X3 sessions
     with SignalRGB's plugins read in place (the MSI board stayed on HueDeck's
     plugin, which wins for its ids); all four rendered with 0 render and 0 HID
     errors; the Kraken's channel showed its strip;
   - **Import SignalRGB layout and channels** stored 30 placements as layout
     `SignalRGB` and 3 channel setups, switched to the layout, and the layout editor
     drew every endpoint where SignalRGB has it (the MSI board with SignalRGB's scale
     38.86);
   - dragging the Razer footprint moved it to (200, 156); the running session
     reported the same placement right away;
   - effects applied from the Effects tab (Plasma, Shader Waves, Aurora, ...),
     global brightness and pause/resume reached every device;
   - **Stop HueDeck**: every session ran its plugin's `Shutdown()` and the process
     exited with code 0.

Found and fixed during the run: the import restarted every known device (now only
those whose channel setup changes); tabs did not follow URL hash changes; the
layout stage did not scale with the window; a stopped daemon showed the browser's
"Failed to fetch" instead of saying HueDeck is not running.

Still to do for M4: the installer (MSI, per-user), autostart, and the
M4 acceptance runs on a fresh machine.
