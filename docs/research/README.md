<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# Research briefs (M5-M7)

Design input for the later milestones, written before implementation so the external
interfaces (drivers, OS APIs, protocols, crates) are pinned down with sources. Each brief
was drafted by one research pass and then re-checked by an independent fact-check pass that
re-fetched the primary sources; the corrected text is what is stored here, followed by the
list of corrections and of the claims that are still unverified.

Provenance tags used inline: **[V: source]** verified against a primary source,
**[V-local]** read-only observation on the reference machine, **[V-sec]** secondary source
only, **[U]** unverified (inference, estimate, untested), **[D]** Lightloom design choice.
`<research-cache>` marks downloads kept outside the repository.

| Milestone | Brief | Scope |
|---|---|---|
| M5 | [m5-privileged-buses.md](m5-privileged-buses.md) | PawnIO, chipset SMBus (PIIX4/I801), NvAPI GPU I2C, NCT6687D SuperIO, ADLX; broker design |
| M6 | [m6-windows-integrations.md](m6-windows-integrations.md) | Dynamic Lighting / LampArray, OpenRGB SDK server, CM_Register_Notification hotplug, device identity |
| M6 | [m6-inputs-network.md](m6-inputs-network.md) | audio FFT, screen capture zones, mDNS/UDP/TCP, XHR, DTLS-PSK, ARP for network plugins |
| M7 | [m7-linux-backend.md](m7-linux-backend.md) | hidraw/hidapi, nusb, i2c-dev SMBus, hwmon/SuperIO fans, CEF on Linux, PipeWire, udev, systemd |

Nothing here is normative: ADRs in `docs/adr/` and the host-API specs in `docs/host-api/`
record decisions. Where a brief contradicts them, the brief's finding is tracked as a
correction of the ADR/spec (see the git history of those files).
