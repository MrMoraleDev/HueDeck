<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Privacy

Lightloom has no account, no sign-in, no telemetry, no crash reporting, no update check
and no ads. It sends nothing about you or your computer anywhere.

## Network connections

Lightloom makes **one** outbound connection of its own, and only when you ask for it:

| When | What | Where |
|---|---|---|
| You leave **RAM and motherboard lighting** checked in the installer, or press **Install** in Settings → PawnIO / RAM lighting, and the PawnIO driver is missing or older than 2.2.0 | One HTTPS download of the official PawnIO installer, `PawnIO_setup.exe` 2.2.0 (about 3.4 MB) | `https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe` (GitHub, which redirects to its download servers) |

The download is made by `lightloom-setup` (the elevated helper you approve in the Windows
permission prompt), with Windows' own HTTP stack and proxy settings. It sends nothing but
the request for that file (the usual HTTP headers, user agent `lightloom-setup/<version>`).
The file is checked against a pinned size and SHA-256 and its Authenticode signature
(signer `namazso.eu`) before it runs, and deleted afterwards. When PawnIO 2.2.0 or later
is already installed, nothing is downloaded. When the download fails (offline, say),
nothing is changed and RAM lighting stays off until you try again.

Everything else stays on your computer:

- `lightloomd` listens on the loopback interface (`127.0.0.1`) only: the dashboard, the
  local API and the SignalRGB-compatible ports are not reachable from other computers.
- **Plugins** may talk to lighting devices on your local network (for example a network
  LED controller or a smart light), the way their device needs. That traffic goes to the
  devices the plugin drives, on your LAN; Lightloom's first-party plugins never contact the
  internet.
- **Effects** are web pages rendered offline by the bundled Chromium; an effect you add
  yourself could load web content, as any web page can.

## Device reports

The Devices tab has two buttons per device, **Works** and **Report a problem**. They do not
send anything: they open a GitHub issue form (`.github/ISSUE_TEMPLATE/device-report.yml`)
in your browser with the fields filled in, and you decide there whether to submit it.
Before the browser opens, the dashboard shows exactly what the form will contain. What goes
in, all built inside the dashboard page:

- the result ("Works" or "Does not work"), the device's name, its USB VID:PID (or SMBus
  address, or "network device"), the plugin's name and version, the Lightloom version and the
  operating system with its build number;
- for a problem, the text you type;
- diagnostics: the device key with the serial number masked, the plugin's type and file,
  the device state and driver state, frame and error counters, its interfaces (HID
  collections, USB interfaces and their drivers), endpoints, channels, the plugin's messages,
  and the log lines that name the device and arrived since the dashboard page opened.

Before anything goes into the link, the dashboard removes the API token, replaces the user
folder in paths (`C:Users<name>`, `/home/<name>`) with `%USERPROFILE%` or `~`, masks serial
numbers and network ids except their last 4 characters, and replaces IP and MAC addresses
with `<ip>` and `<mac>`. The diagnostics are kept under about 6 KB (the oldest log lines go
first) because the whole form travels in the link. Opening the link is a normal visit to
github.com; you need a GitHub account to submit the issue.

If you tick "Open Works reports without this preview", a later **Works** click opens the
form directly; the choice is kept in the browser's local storage for the dashboard page.

## What is stored

Settings, layouts, presets and the API token stay in your user profile
(`%LOCALAPPDATA%\Lightloom`, `%APPDATA%\Lightloom`; `~/.local/share/lightloom` and
`~/.config/lightloom` on Linux). Logs of the background daemon go to
`%LOCALAPPDATA%\Lightloom\logs`. Nothing is uploaded; a settings export is a file you save
yourself.

## Uninstalling

Uninstalling Lightloom removes its program files, its Start-menu shortcut, its sign-in entry
and, behind one Windows permission prompt, the Lightloom I/O service. It leaves the PawnIO
driver installed, because other programs (fan and monitoring tools) use it too; remove it
from Windows Settings → Apps if you no longer need it. Your settings in the user profile
stay, so a reinstall picks them up; delete the folders above to remove them.
