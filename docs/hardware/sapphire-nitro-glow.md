<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Sapphire Radeon cards: Nitro Glow V3 at I2C 0x28

The plugin is `plugins/sapphire-nitro-glow/`. It drives the "Nitro Glow V3" RGB controller
Sapphire puts on its Radeon Nitro+, Pure and Toxic cards (RX 5500 XT to RX 9070 XT), at
7-bit address `0x28` on the GPU's own I2C bus. HueDeck reaches that bus through the legacy
AMD Display Library (`crates/huedeck-adl`, `atiadlxx.dll` from the AMD driver), line 1, from
its own (unelevated) process. It is HueDeck's first AMD GPU plugin.

**Status: untested.** The reference PC has an NVIDIA card; the plugin and the ADL bus are
tested against mocks only (`plugins/sapphire-nitro-glow/tests/packets.test.mjs`,
`crates/huedeck-adl` fake-backed tests, `crates/huedeck-runtime` scan test). No card is in
the manifest's `tested` list.

Why this controller first: among OpenRGB's AMD GPU controllers it is the simplest that writes
**lighting registers only** and has **no save-to-flash command** (OpenRGB: `@save :x:`), and it
covers the most cards (28 PCI tuples, RDNA1 to RDNA4).

## Sources

All OpenRGB files are at commit `d4798c12` (https://gitlab.com/CalcProgrammer1/OpenRGB),
each "part of the OpenRGB project" with `SPDX-License-Identifier: GPL-2.0-or-later`.

| # | Source | Used for |
|---|--------|----------|
| S1 | `Controllers/SapphireGPUController/SapphireNitroGlowV3Controller/SapphireNitroGlowV3Controller.h` (K900) | Register map and mode values |
| S2 | `.../SapphireNitroGlowV3Controller/SapphireNitroGlowV3Controller.cpp` | One SMBus Write Byte Data / Read Byte Data per register |
| S3 | `.../SapphireNitroGlowV3Controller/RGBController_SapphireNitroGlowV3.cpp` | What is read at start, the "Static" mode sequence, the per-frame sequence |
| S4 | `Controllers/SapphireGPUController/SapphireGPUControllerDetect.cpp` | Address 0x28 (V3) / 0x55 (V1), the card list, the presence read |
| S5 | `pci_ids/pci_ids.h` | Device ids (`AMD_NAVI*_DEV`) and Sapphire subsystem ids (`SAPPHIRE_SUB_VEN = 0x1DA2`) |
| S6 | `i2c_smbus/Windows/i2c_smbus_amdadl.cpp` | How OpenRGB's ADL bus sends each transfer (see `docs/host-api/smbus.md` §10) |

The plugin's code is adapted from S1-S4 (GPL-2.0-or-later, so it can be GPL-3.0-or-later
here); no SignalRGB code is used.

## Registers (S1)

| Register | Name | HueDeck |
|---|---|---|
| `0x0F` | external control (1 = the LED follows an external source) | read at start; `0x00` on Initialize; restored last on Shutdown |
| `0x10` | mode (`0x06` custom colour, `0x00` rainbow, `0x07` off, ...) | read at start; `0x06` on Initialize; restored on Shutdown |
| `0x1A`, `0x1B`, `0x1C` | red, green, blue | the frame colour; read at start and restored on Shutdown |
| `0x11`-`0x16`, `0x29`, `0x3E` | animation speeds, music volume, brightness | never touched |

Every transfer is an SMBus Write Byte Data or Read Byte Data to `0x28`. On ADL a write is
`iOffset = 0` with data `[register, value]`; a read is `iOffset = register`, 1 byte.

## Sequences

- **Scan**: an id match on an AMD GPU bus (`IsAMDGpuBus()`), then one Read Byte at register
  `0x00` (OpenRGB's `i2c_smbus_read_byte`, which its ADL bus sends as a read at offset 0); the
  card is kept when the read succeeds.
- **Initialize**: read `0x1A 0x1B 0x1C 0x0F 0x10`; write `0x0F ← 0`, `0x10 ← 0x06` (S3,
  `DeviceUpdateMode()` for "Static").
- **Render**: `0x1A ← R`, `0x1B ← G`, `0x1C ← B`, only when the colour changed.
- **Shutdown**: the saved colour, then the saved mode, then the saved external control. When a
  startup read failed: the `shutdownColor` parameter (default black) in mode `0x06`.

S3's `ReadConfiguration()` reads the colour back as R, B, G; S3's write path and S2 use R, G,
B at `0x1A/0x1B/0x1C`, which the plugin follows. Worth checking on a real card.

## Cards (S4, S5)

All `1002:dddd` with subsystem vendor `1DA2`: RX 5500 XT Nitro+ (`7340:E423`), RX 5700 XT
Nitro+ (`731F:E409/E410/426E`), RX 6600 XT (`73FF:E448`), RX 6650 XT (`73EF:E448`), RX 6700 XT
(`73DF:E445`), RX 6750 XT (`73DF:E465`), RX 6800 (`73BF:E439`), RX 6800 XT SE (`73BF:438E`),
RX 6800 XT/6900 XT (`73BF:E438`), RX 6900 XT SE (`73BF:440E`), RX 6900 XT Toxic
(`73AF:F440`, `73BF:440F`), RX 6900 XT Toxic LE (`73BF:F441`), RX 6950 XT Toxic (`73A5:D440`),
RX 6950 XT Nitro+ (`73A5:441D`), RX 6950 XT Nitro+ Pure (`73A5:440D`), RX 7700 XT (`747E:F475`),
RX 7800 XT (`747E:D475`), RX 7900 GRE (`744C:E475`), RX 7900 XTX (`744C:E471`), RX 9060 XT Pure
(`7590:493E`), RX 9060 XT Nitro+ (`7590:E493`), RX 9070 Pure (`7550:4499`), RX 9070 XT Nitro+
(`7550:E489`, `7550:4892`), RX 9070 XT Pure (`7550:3490`). Whether `ADL2_Display_WriteAndReadI2C`
works on RDNA3/RDNA4 drivers is unverified (research drivers.md §3).

## Hardware test (needs an AMD card)

1. `huedeck plugins` lists one `AMD ADLX Bus - <card>` bus with the card's four ids, and the
   scan outcome of `sapphire-nitro-glow` finds `0x28`.
2. `huedeck light red|green|blue|white`, then `huedeck rainbow`: the card follows; then it
   returns to its startup effect.
3. Record the card, the driver version and the write/error counts here, then add the card to
   `tested` in the manifest.

## Other AMD GPU candidates (not implemented)

OpenRGB controllers that also run on AMD GPU buses, all GPL-2.0-or-later at `d4798c12`:

| Controller | Cards | Why not first |
|---|---|---|
| `SapphireNitroGlowV1Controller` (0x55) | RX 470-590, Vega 56/64 Nitro+ | Older cards; another register map. No save. A good second plugin. |
| `PowerColorGPUController` Red Devil V1/V2 | RX 5700 to RX 9070 XT Red Devil | Larger protocol (per-zone LED blocks); check its RGBController for any persistent write before porting. |
| `ASRockSMBusController/ASRockGPUSMBusController` (0x36) | ASRock Phantom/Taichi Radeon | 12-byte block packets, `WriteQuick` probe (needs the zero-length ADL write to be verified). No save. |
| `MSIGPUController` (0x68) | MSI Radeon Gaming X/Trio | Same as `plugins/msi-gpu`: adding its AMD tuples there is the cheapest step; `0x3F` save is already denied. |
| `AsusAuraGPUController`, `ENESMBusController` | ASUS ROG/TUF Radeon | ENE register file with a save command; needs a deny list review. |
| `GigabyteRGBFusion2GPUController` | Gigabyte Aorus/Gaming Radeon | Has a save path (`@save :white_check_mark:`); needs a review of which commands persist. |
