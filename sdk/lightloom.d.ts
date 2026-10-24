// SPDX-License-Identifier: GPL-3.0-or-later
//
// TypeScript typings of the Lightloom plugin host API, as Lightloom implements it
// (crates/lightloom-plugin-host js/host.js and its natives in src/{device,children,bus}.rs,
// prelude/, crates/lightloom-renderer src/inject.rs). Written from docs/host-api/*.md and the
// Lightloom sources; nothing here comes from SignalRGB's own (unpublished) typings.
//
// Use from a JavaScript plugin (checked with `tsc --allowJs --checkJs --noEmit`):
//
//   /// <reference path="../../sdk/lightloom.d.ts" />
//   /** @type {Lightloom.PluginExports["Render"]} */
//   export function Render() { ... }
//
// JSDoc tags used on members:
//
//   @signalrgb      part of the SignalRGB 2.5-era plugin surface Lightloom is compatible with
//                   (a plugin that uses only these also runs on SignalRGB).
//   @lightloom        a Lightloom extension: a plugin that uses it runs only on Lightloom.
//   @unimplemented  documented, but a Lightloom device session does not implement it yet:
//                   calling it logs `API_GAP` once and throws `HostApiMissing`.
//   @stub           present, answers placeholder data (no hardware or network behind it yet).
//
// Not declared here: members Lightloom does not know. In a device session reading one returns a
// function that throws `HostApiMissing`; `lightloom compat dry-run` reports it as an API gap.

declare namespace Lightloom {
  // ------------------------------------------------------------------ basics

  /** One byte, 0..255. Arrays passed to the host are masked with `& 0xFF`. */
  type Byte = number;
  /** A byte array: any array-like of numbers. */
  type Bytes = ArrayLike<number>;
  /** `[r, g, b]`, each 0..255. */
  type RGB = [number, number, number];
  /** `"#RRGGBB"`. */
  type HexColor = string;
  /** Colour array layout: one flat array, or one array per channel (both spellings accepted). */
  type ColorLayout = "Inline" | "Separate" | "Seperate";
  /** Byte order of each colour in the result of the colour helpers. */
  type ColorOrder = "RGB" | "RBG" | "BGR" | "BRG" | "GBR" | "GRB";
  /** `"Inline"` gives `number[]` (`[r, g, b, r, g, b, ...]`); the separate layouts `[reds, greens, blues]`. */
  type ColorArray = number[] | [number[], number[], number[]];

  /** `Type()` values (case-insensitive; absent = HID). */
  type PluginType = "Hid" | "hid" | "rawusb" | "hybrid" | "serial" | "SMBUS" | "network";
  /** `DeviceType()` values. */
  type DeviceType =
    | "keyboard" | "mouse" | "lightingcontroller" | "other" | "mousepad" | "headphones" | "gpu" | "aio"
    | "lcd" | "motherboard" | "speakers" | "ram" | "microphone" | "dongle" | "case";

  // ------------------------------------------------------------------ HID

  /**
   * One HID top-level collection of the matched device, as `Validate(endpoint)` and
   * `device.getHidEndpoints()` see it. Same tuple on every OS.
   * @signalrgb
   */
  interface HidEndpoint {
    /** USB interface number (`MI_xx`); 0 for a non-composite device, -1 without a USB interface. */
    interface: number;
    /** Top-level collection usage. */
    usage: number;
    /** Top-level collection usage page. */
    usage_page: number;
    /** Collection index within the interface (Windows `ColNN`), 0 for a single collection. */
    collection: number;
  }

  /** @signalrgb */
  interface HidInfo {
    /** Input report length of the active endpoint, report id included (0: none). */
    readLength: number;
    /** Output report length of the active endpoint, report id included (0: none). */
    writeLength: number;
  }

  /** @signalrgb (only `product` is used by SignalRGB plugins; the others are Lightloom's) */
  interface DeviceInfo {
    product: string;
    manufacturer: string;
    serialNumber: string;
    vendorId: number;
    productId: number;
  }

  /** A report of the open collection (`device.getHidReportFields()`). @lightloom */
  interface HidReport {
    kind: "input" | "output" | "feature";
    reportId: number;
    /** Report length in bytes, report id included. */
    bytes: number;
  }

  /** A main item of the open collection's report descriptor. @lightloom */
  interface HidReportField {
    kind: "input" | "output" | "feature";
    reportId: number;
    /** Bit offset after the report-id byte (little-endian bit order). */
    bitOffset: number;
    bitSize: number;
    count: number;
    /** Main-item flags: bit 0 constant, bit 1 variable. */
    flags: number;
    logicalMin: number;
    logicalMax: number;
    /** `[usagePage, usage]` of the innermost collection around the field. */
    collection: [number, number];
    /** `[page, min, max]` usage ranges in declaration order. */
    usages: Array<[number, number, number]>;
  }

  /** @lightloom */
  interface HidReportFields {
    usagePage: number;
    usage: number;
    reports: HidReport[];
    fields: HidReportField[];
  }

  // ------------------------------------------------------------------ channels and components

  /** A component instance assigned to a channel (SignalRGB component JSON). @signalrgb */
  interface ComponentData {
    ProductName?: string;
    DisplayName?: string;
    Brand?: string;
    Type?: string;
    LedCount: number;
    Width?: number;
    Height?: number;
    LedMapping?: number[];
    LedCoordinates?: Array<[number, number]>;
    LedNames?: string[];
    Image?: string;
    ImageUrl?: string;
    ComponentId?: string;
    [key: string]: unknown;
  }

  /** @signalrgb */
  interface ChannelComponent {
    componentID: string;
    componentData: ComponentData;
  }

  /**
   * The value of `device.channel(name)`: the chain of components (fans, strips) the user
   * assigned to one channel of a controller.
   * @signalrgb
   */
  interface ComponentChannel {
    /** Sum of the assigned components' LEDs (capped by the limit). */
    readonly ledCount: number;
    readonly ledLimit: number;
    readonly name: string;
    /** Always false in Lightloom (no colour-override UI yet). */
    overrideColors: boolean;
    overrideCount: number;
    forceColor: boolean;
    identifying: boolean;
    LedCount(): number;
    LedLimit(): number;
    SetLedLimit(limit: number): void;
    /** No-op in Lightloom (the count comes from the components). */
    SetLedCount(count: number): void;
    Name(): string;
    /** No-op in Lightloom device sessions. */
    SetName(name: string): void;
    /** The channel's colours, sampled from the canvas at each component LED, in chain order. */
    getColors(layout?: ColorLayout, order?: ColorOrder | string): ColorArray;
    /** True while the user identifies the channel. Always false in Lightloom (no UI yet). */
    shouldPulseColors(): boolean;
    onlyHasDefaultComponents(): boolean;
    /** Component instance ids in chain order. */
    getComponentNames(): string[];
    getComponents(): ChannelComponent[];
    getComponent(id: string): ChannelComponent | undefined;
    getComponentColors(id: string, layout?: ColorLayout, order?: ColorOrder | string): ColorArray;
    getComponentData(id: string): ComponentData | undefined;
    getChannelColor(): HexColor;
    /** `count` LEDs of the identify colour (`#009EDB`). */
    getPulse(count: number, layout?: ColorLayout, order?: ColorOrder | string): ColorArray;
  }

  // ------------------------------------------------------------------ plugin parameters

  /** One `ControllableParameters()` entry (schemas/controllable-parameters.schema.json). @signalrgb */
  interface ControllableParameter {
    /** JS identifier: the value becomes a module-level global of this name. */
    property: string;
    type: "color" | "combobox" | "number" | "boolean" | "hue" | "textfield" | "keycode" | "mousecode" | "list";
    label?: string;
    group?: string;
    description?: string;
    tooltip?: string;
    /** Numbers may be quoted (`"100"`); the host coerces them. */
    min?: number | string;
    max?: number | string;
    step?: number | string;
    default?: string | number | boolean | null;
    /** Choices of a combobox / list. */
    values?: Array<string | number | { key: string | number; label: string }>;
    live?: boolean | "true" | "false";
    order?: number | string;
    filter?: string;
    [key: string]: unknown;
  }

  /** @signalrgb */
  interface DeviceMessage {
    property: string;
    message: string;
    tooltip: string;
  }

  // ------------------------------------------------------------------ the `device` global

  /**
   * The device the plugin drives (global `device`). In `Scan(bus)` of an SMBus plugin it does
   * not exist.
   */
  interface Device {
    // --- HID I/O (docs/host-api/hid-usb.md §2) ----------------------------------------

    /**
     * Write an output report (report id first). `data` is zero-padded or truncated to
     * `length` (default: its own length). Returns the bytes written, or -1 on failure; never throws.
     * @signalrgb
     */
    write(data: Bytes, length?: number): number;
    /** Set a feature report (report id first). Same conventions as `write`. @signalrgb */
    send_report(data: Bytes, length?: number): number;
    /**
     * Read an input report: `length` bytes, report id first; `[]` on timeout
     * (`getLastReadSize()` 0). Only `reportId[0]` matters. `timeout` ms, default 100, at most 5000.
     * @signalrgb
     */
    read(reportId: Bytes | number, length: number, timeout?: number): number[];
    /** Get a feature report: `length` bytes, report id first. @signalrgb */
    get_report(reportId: Bytes | number, length: number): number[];
    /** GET_REPORT(Input) through the control pipe. @signalrgb */
    input_report(reportId: Bytes | number, length: number): number[];
    /** Bytes the last read / get_report returned (0 on timeout). @signalrgb */
    getLastReadSize(): number;
    /** Discard queued input reports. @signalrgb */
    clearReadBuffer(): void;
    /** Same as `clearReadBuffer()` in Lightloom. @signalrgb */
    flush(): void;
    /** Make another validated collection the active endpoint (3 arguments: any collection). @signalrgb */
    set_endpoint(iface: number, usage: number, usagePage: number, collection?: number): void;
    /** Every HID collection of the physical device, validated or not. @signalrgb */
    getHidEndpoints(): HidEndpoint[];
    /** Report lengths of the active endpoint. @signalrgb */
    getHidInfo(): HidInfo;
    /**
     * The active collection's report descriptor, parsed by the host; `null` when the backend
     * cannot read it. No report I/O.
     * @lightloom
     */
    getHidReportFields(): HidReportFields | null;
    /** @signalrgb */
    getDeviceInfo(): DeviceInfo;
    /** USB product id of the opened device. @signalrgb */
    productId(): number;
    /** libusb bulk transfer. @signalrgb @unimplemented */
    bulk_transfer(endpoint: number, data: Bytes, length: number, timeout?: number): number[] | number;
    /** libusb control transfer. @signalrgb @unimplemented */
    control_transfer(
      requestType: number, request: number, value: number, index: number, data: Bytes, length: number, timeout?: number,
    ): number[];
    /** Switch between `"hid"` and `"hybrid"` at run time. @signalrgb @unimplemented */
    setProtocol(protocol: "hid" | "hybrid"): void;
    /** Sleep the plugin thread (time paused is not charged to the script budget). @signalrgb */
    pause(ms: number): void;
    /** @signalrgb @stub (Lightloom's frame rate comes from the engine) */
    setFrameRateTarget(fps: number): void;
    /** @signalrgb @stub */
    setRefreshRate(hz: number): void;
    /** @signalrgb @stub */
    setFrameDelay(ms: number): void;
    /** @signalrgb @stub */
    resetFrameRateTarget(): void;

    // --- canvas and shape ---------------------------------------------------------------

    /**
     * The canvas colour at LED cell `(x, y)` of the device's own grid (layout placement,
     * scale, rotation and brightness already applied).
     * @signalrgb
     */
    color(x: number, y: number): RGB;
    /** Device brightness 0..100. @signalrgb */
    getBrightness(): number;
    /** @signalrgb */
    setName(name: string): void;
    /** `[width, height]` in cells. @signalrgb */
    setSize(size: [number, number]): void;
    /** Re-declare the LED set at run time (same shape as LedNames/LedPositions). @signalrgb */
    setControllableLeds(names: string[], positions: Array<[number, number]>): void;
    /** @signalrgb @stub */
    setImageFromUrl(url: string): void;
    /** @signalrgb @stub */
    setImageFromBase64(png: string): void;
    /** @signalrgb @stub */
    repollName(): void;
    /** @signalrgb @stub */
    repollLeds(): void;
    /** @signalrgb @stub */
    repollSize(): void;
    /** `count` LEDs of `color` in the given layout and order. @signalrgb */
    createColorArray(color: HexColor, count: number, layout?: ColorLayout, order?: ColorOrder | string): ColorArray;

    // --- channels (docs/host-api/components-channels.md §1) --------------------------

    /** Device-wide LED cap over all channels. @signalrgb */
    SetLedLimit(limit: number): void;
    /** @signalrgb */
    addChannel(name: string, ledLimit: number, defaultLedCount?: number): void;
    /** @signalrgb */
    removeChannel(name: string): void;
    /** @signalrgb */
    getChannelNames(): string[];
    /** The channel, or `undefined` when the plugin did not add it. @signalrgb */
    channel(name: string): ComponentChannel | undefined;
    /** LEDs over all channels. @signalrgb */
    getLedCount(): number;
    /** The identify colour (`#009EDB`). @signalrgb */
    getChannelPulseColor(name?: string, count?: number): HexColor;

    // --- subdevices (components-channels.md §2) ------------------------------------

    /** A separate canvas endpoint owned by this device. @signalrgb */
    createSubdevice(id: string): void;
    /** @signalrgb */
    removeSubdevice(id: string): void;
    /** @signalrgb */
    getCurrentSubdevices(): string[];
    /** @signalrgb */
    setSubdeviceName(id: string, name: string): void;
    /** Two numbers, not an array (unlike `setSize`). @signalrgb */
    setSubdeviceSize(id: string, width: number, height: number): void;
    /** @signalrgb */
    setSubdeviceLeds(id: string, names: string[], positions: Array<[number, number]>): void;
    /** @signalrgb */
    setSubdeviceLedMap(id: string, mapping: number[]): void;
    /** @signalrgb */
    setSubdeviceImageUrl(id: string, url: string): void;
    /** @signalrgb */
    setSubdeviceImage(id: string, png: string): void;
    /** The subdevice's canvas colour at its LED cell `(x, y)`. @signalrgb */
    subdeviceColor(id: string, x: number, y: number): RGB;
    /** Runtime form of the `SubdeviceController()` export. @signalrgb */
    SetIsSubdeviceController(flag: boolean): void;

    // --- properties -----------------------------------------------------------------

    /**
     * Add a parameter at run time, or replace the one with the same `property`; its value
     * becomes a global named `property` (a stored user value wins over `default`). The device
     * card in the dashboard follows at once. Non-objects and objects without `property` are
     * refused and logged. @signalrgb
     */
    addProperty(parameter: ControllableParameter): void;
    /** Remove a parameter (exported ones too) from the device card. @signalrgb */
    removeProperty(name: string): void;
    /**
     * The object given to `addProperty` (or the `ControllableParameters()` row): mutate it and
     * pass it to `addProperty` again to update the control. @signalrgb
     */
    getProperty(name: string): ControllableParameter | undefined;
    /** @signalrgb */
    hasProperty(name: string): boolean;
    /** @signalrgb */
    getPropertyNames(): string[];
    /** @signalrgb */
    setUserProperty(name: string, value: unknown): void;

    // --- features, notifications, logging ------------------------------------------

    /**
     * Install a feature global of the same name. `"udp"` and `"dtls"` throw `HostApiMissing`
     * in Lightloom device sessions (no sockets for device plugins yet).
     * @signalrgb
     */
    addFeature(name: "battery" | "keyboard" | "mouse" | "base64" | "udp" | "dtls"): void;
    /**
     * Show a message on the device card (and in the session log) until `denotify(id)`;
     * returns its id. Priority 0 info, 1 important, 2 and 3 errors. @signalrgb
     */
    notify(title: string, text: string, priority?: number, button?: string): string;
    /** Withdraw a `notify` message. @signalrgb */
    denotify(id: string): void;
    /** Show a message on the device card, or replace the one with the same id. @signalrgb */
    addMessage(id: string, message: string, tooltip?: string): void;
    /** Withdraw a message (`DeviceMessages()` rows too, by their `property`). @signalrgb */
    removeMessage(id: string): void;
    /** Log a line; `{ hex: true }` prints a byte array as hex. @signalrgb */
    log(value: unknown, options?: { hex?: boolean; toFile?: boolean }): void;

    // --- fans and temperature (components-channels.md §5) -------------------------

    /** Always true in Lightloom: it does not drive fans yet. @signalrgb */
    fanControlDisabled(): boolean;
    /** @signalrgb @unimplemented */
    createFanControl(id: string): void;
    /** @signalrgb @unimplemented */
    removeFanControl(id: string): void;
    /** @signalrgb @unimplemented */
    setRPM(id: string, rpm: number): void;
    /** @signalrgb @unimplemented */
    getFanlevel(id: string): number;
    /** @signalrgb @unimplemented */
    getFanLevel(id: string): number;
    /** @signalrgb @unimplemented */
    getNormalizedFanlevel(id: string): number;
    /** @signalrgb @unimplemented */
    getNormalizedFanLevel(id: string): number;
    /** @signalrgb @stub */
    createTemperatureSensor(id: string): void;
    /** @signalrgb @stub */
    removeTemperatureSensor(id: string): void;
    /** @signalrgb @stub */
    SetTemperature(id: string, celsius: number): void;
    /** LCD frame buffer. @signalrgb @unimplemented */
    getImageBuffer(...args: unknown[]): number[];
    /** @signalrgb @unimplemented (use `@SignalRGB/systeminfo`) */
    getMotherboardName(): string;
  }

  // ------------------------------------------------------------------ feature globals

  /** Battery state numbers (`battery.*` constants). 2..5 count as "on power". */
  type BatteryState = 0 | 1 | 2 | 3 | 4 | 5;

  /**
   * Global `battery` after `device.addFeature("battery")` (idempotent). In a Lightloom device
   * session the daemon shows the level, warns at the user's thresholds (a notification, a
   * `battery` event, optionally a red pulse on the device). Poll the device rarely (every
   * 30-60 s) and call the setters when a value changes. docs/host-api/hid-usb.md §7.1.
   * @signalrgb
   */
  interface Battery {
    /** No number in the corpus; 0. */
    readonly disabled: 0;
    readonly unknown: 0;
    readonly draining: 1;
    readonly charging: 2;
    readonly fullCharging: 3;
    readonly full: 4;
    readonly wirelessCharging: 5;
    /** Last level set, 0 before the first `setBatteryLevel`. */
    readonly batteryLevel: number;
    /** Last state set. */
    readonly batteryState: BatteryState;
    readonly stateString: "Unknown" | "Draining" | "Charging" | "Full (charging)" | "Full" | "Wireless charging";
    /** The user's low-battery threshold, percent (Settings, default 15); updates live. */
    readonly lowBatteryWarningLevel: number;
    /** Percent: rounded and clamped to 0..100; a non-finite value is ignored. */
    setBatteryLevel(level: number): void;
    /** One of the state constants; anything outside 0..5 is stored as 0 (unknown). */
    setBatteryState(state: number): void;
  }

  /** Global `keyboard` after `device.addFeature("keyboard")`. @signalrgb @stub (no input injection yet) */
  interface Keyboard {
    sendHid(virtualKey: number, options?: { released?: boolean }): void;
    sendKey(keyName: string, options?: { released?: boolean }): void;
    sendEvent(event: { key: string; keyCode?: number; released?: boolean }, kind: string): void;
  }

  /** Global `mouse` after `device.addFeature("mouse")`. @signalrgb @stub */
  interface Mouse {
    sendHid(flag: number, options?: { XButton?: number; WheelDelta?: number }): void;
    sendEvent(event: { button: string; released?: boolean; [key: string]: unknown }, kind: string): void;
  }

  /** Global `base64` after `device.addFeature("base64")` (also `@SignalRGB/base64`). @signalrgb */
  interface Base64 {
    Encode(bytes: Bytes, urlSafe?: boolean): string;
    Decode(text: string): number[];
    encode(bytes: Bytes, urlSafe?: boolean): string;
    decode(text: string): number[];
  }

  /** A socket of `udp.createSocket()` / `tcp.createSocket()`. @signalrgb @stub */
  interface Socket {
    readonly UnconnectedState: 0;
    readonly HostLookupState: 1;
    readonly ConnectingState: 2;
    readonly ConnectedState: 3;
    readonly BoundState: 4;
    readonly ListeningState: 5;
    readonly ClosingState: 6;
    readonly state: number;
    on(event: string, handler: (...args: unknown[]) => void): void;
    bind(port?: number): void;
    connect(host: string, port: number): void;
    disconnect(): void;
    close(): void;
    send(data: Bytes | string): number;
    write(data: Bytes | string): number;
    address(): { address: string; port: number; protocol: string };
    remoteAddress(): { address: string; port: number; protocol: string };
  }

  /** Global `udp` after `device.addFeature("udp")`. @signalrgb @unimplemented (device sessions) */
  interface UdpFeature {
    send(ip: string, port: number, data: Bytes, bigEndian?: boolean): number;
    createSocket(): Socket;
  }

  /** Global `dtls` after `device.addFeature("dtls")`. @signalrgb @unimplemented (device sessions) */
  interface DtlsFeature {
    createConnection(ip: string, port: number, identity: string, psk: string): void;
    send(data: Bytes): number;
    hasEncryptedConnection(): boolean;
    CloseConnection(): void;
    onConnectionEstablished(handler: () => void): void;
    onConnectionClosed(handler: () => void): void;
    onConnectionError(handler: (error: unknown) => void): void;
    ignoreSslErrors(): void;
  }

  // ------------------------------------------------------------------ SMBus (docs/host-api/smbus.md)

  /** Members both bus objects have. */
  interface BusIdentity {
    /** PCI vendor id of the controller that owns the bus. @signalrgb */
    Vendor(): number;
    /** @signalrgb */
    SubVendor(): number;
    /** PCI device id. @signalrgb */
    Product(): number;
    /** @signalrgb */
    SubDevice(): number;
    /** e.g. `"AMD PIIX4 Bus - 0x0b00"`, the GPU name on NVIDIA buses. @signalrgb */
    Name(): string;
    /** The 7-bit address the fixed bus is bound to (0 on the free bus). @signalrgb */
    GetAddress(): number;
    /** Chipset buses (AMD, Intel). @signalrgb */
    IsSystemBus(): boolean;
    /** @signalrgb */
    IsAMDBus(): boolean;
    /** @signalrgb */
    IsIntelBus(): boolean;
    /** NvAPI GPU I2C. @signalrgb */
    IsNvidiaBus(): boolean;
    /** AMD GPU (ADLX) I2C. @signalrgb */
    IsAMDGpuBus(): boolean;
    /** @signalrgb */
    IsNuvotonBus(): boolean;
    /**
     * Hold the bus across several transfers. Lightloom returns whether the bus is held
     * (`false`: send nothing, try at the next frame); SignalRGB returns nothing, so test `=== false`.
     * @signalrgb (the boolean result is Lightloom's)
     */
    TransactionLock(): boolean;
    /**
     * Release the bus. Lightloom: `0`, or (with `SmbusBatchWrites()`) the status of the first queued
     * write that failed; SignalRGB returns nothing.
     * @signalrgb (the result is Lightloom's)
     */
    TransactionUnlock(): number | undefined;
    /** @signalrgb */
    HasTransactionLock(): boolean;
    /** Deprecated in SignalRGB 2.4.66. @signalrgb @unimplemented */
    FetchRamInfo(): string;
    /** Lightloom `--dry-run`: writes are recorded and answer 0 but are never sent; reads still happen. @lightloom */
    IsDryRun(): boolean;
    /** @signalrgb */
    log(value: unknown, options?: { toFile?: boolean; hex?: boolean }): void;
    /** @signalrgb */
    pause(ms: number): void;
    /** `"I2CBusWrapperFixedAddress(...)"` or `"I2CBusWrapper(...)"`. @signalrgb */
    toString(): string;
  }

  /** A DIMM slot whose SPD names one of the plugin's `SpdManufacturers()`. @lightloom */
  interface SpdSlot {
    /** SPD address 0x50..0x57. */
    address: number;
    memoryType: "DDR4" | "DDR5" | string;
    /** SPD byte 2. */
    typeByte: number;
    /** JEP106 bank, 1-based. */
    jedecBank: number;
    /** JEP106 id byte as stored (parity bit included). */
    jedecId: number;
    partNumber: string;
  }

  /**
   * The `Scan(bus)` argument: every transfer takes the 7-bit address first. Transfers return
   * 0 / the value read, or a negative status; they never throw. Bad arguments throw
   * `RangeError` / `TypeError`. Only the plugin's `SmbusAddresses()` are reachable.
   * @signalrgb
   */
  interface FreeAddressBus extends BusIdentity {
    Type(): "Free";
    /** SMBus Quick write: 0 = something ACKed. Refused on NVIDIA buses (-1). */
    WriteQuick(address: number): number;
    ReadByte(address: number, register: number): number;
    WriteByte(address: number, register: number, value: number): number;
    /** 16-bit little-endian word. */
    ReadWord(address: number, register: number): number;
    WriteWord(address: number, register: number, word: number): number;
    /** SMBus Block Write of `length` (1..32) bytes: longer data clipped, shorter zero-padded. */
    WriteBlock(address: number, register: number, length: number, data: Bytes): number;
    /** `[status, bytes]`. Refused on chipset buses. */
    ReadBlockBytes(address: number, register: number, length: number): [number, number[]];
    /** Raw I2C write, no command byte. Refused on chipset buses. */
    WriteBlockWithoutRegister(address: number, length: number, data: Bytes): number;
    /** Raw I2C read, `[status, bytes]`. Refused on chipset buses. */
    ReadBlockWithoutRegister(address: number, length: number): [number, number[]];
    /** SMBus Receive Byte (Lightloom also allows it on AMD and Intel chipset buses). */
    ReadByteWithoutRegister(address: number): number;
    /** The matching DIMM slots the host found before `Scan(bus)` (copies; no I/O). @lightloom */
    SpdSlots(): SpdSlot[];
  }

  /**
   * The global `bus` of an SMBus device session: bound to the address `Scan()` returned.
   * Same transfers without the address; no `WriteQuick`, no `SpdSlots`.
   * @signalrgb
   */
  interface FixedAddressBus extends BusIdentity {
    Type(): "Fixed";
    ReadByte(register: number): number;
    WriteByte(register: number, value: number): number;
    ReadWord(register: number): number;
    WriteWord(register: number, word: number): number;
    WriteBlock(register: number, length: number, data: Bytes): number;
    ReadBlockBytes(register: number, length: number): [number, number[]];
    WriteBlockWithoutRegister(length: number, data: Bytes): number;
    ReadBlockWithoutRegister(length: number): [number, number[]];
    ReadByteWithoutRegister(): number;
  }

  /** A card of `BrandGPUList()` (all four PCI ids must match the bus). @signalrgb */
  interface GPUIdentifier {
    Vendor: number;
    Device: number;
    SubVendor: number;
    SubDevice: number;
    Address: number;
    Name: string;
    Model?: string;
  }

  /** A DIMM manufacturer of `SpdManufacturers()`. @lightloom */
  interface SpdManufacturer {
    /** JEP106 bank, 1-based. */
    bank: number;
    /** JEP106 id byte (compared on its low 7 bits). */
    id: number;
    name?: string;
  }

  /** Bus kinds of `SmbusBuses()`. @lightloom */
  type SmbusBusKind = "system" | "amd" | "intel" | "nvidia" | "amdgpu" | "nuvoton" | "any";

  // ------------------------------------------------------------------ plugin exports

  /** Something exported either as a value or as a function returning it (call-if-function rule). */
  type Exported<T> = T | (() => T);

  /**
   * What a plugin module exports. Metadata exports may be values or functions; the host calls
   * functions once when it reads the catalogue (and `device.productId()` is bound by then).
   * Every `ControllableParameters()` property `p` may come with an `on<p>Changed()` hook,
   * called before the next `Render()` when the user changes it.
   */
  interface PluginExports {
    // --- metadata (@signalrgb unless noted) ------------------------------------------
    /** @signalrgb */
    Name: Exported<string>;
    /** @signalrgb */
    Publisher?: Exported<string>;
    /** USB vendor id (one per module; a second vendor needs a second module). @signalrgb */
    VendorId?: Exported<number>;
    /** USB product ids (decimal strings accepted, e.g. from `Object.keys`). @signalrgb */
    ProductId?: Exported<number | Array<number | string> | Record<string, unknown>>;
    /** Grid size in LED cells. @signalrgb */
    Size?: Exported<[number, number]>;
    /** @signalrgb */
    DefaultPosition?: Exported<[number, number]>;
    /** Canvas pixels per LED cell. @signalrgb */
    DefaultScale?: Exported<number>;
    /** Same length and order as LedPositions. @signalrgb */
    LedNames?: Exported<string[]>;
    /** `[x, y]` cells inside Size. @signalrgb */
    LedPositions?: Exported<Array<[number, number]>>;
    /** @signalrgb */
    ImageUrl?: Exported<string>;
    /** Base64 PNG. @signalrgb */
    Image?: Exported<string>;
    /** Documentation slug. @signalrgb */
    Documentation?: Exported<string>;
    /** @signalrgb */
    Version?: Exported<string>;
    /** Absent: HID. @signalrgb */
    Type?: Exported<PluginType>;
    /** @signalrgb */
    DeviceType?: Exported<DeviceType>;
    /** @signalrgb */
    ControllableParameters?: Exported<ControllableParameter[]>;
    /** Executables that fight over the device: reported to the user, never killed. @signalrgb */
    ConflictingProcesses?: Exported<string[]>;
    /** @signalrgb */
    SubdeviceController?: Exported<boolean>;
    /** @signalrgb */
    SupportsSubdevices?: Exported<boolean>;
    /** @signalrgb */
    LacksOnBoardLeds?: Exported<boolean>;
    /** @signalrgb */
    DefaultComponentBrand?: Exported<string>;
    /** @signalrgb */
    SupportsFanControl?: Exported<boolean>;
    /** @signalrgb */
    DeviceMessages?: Exported<DeviceMessage[]>;
    /** @signalrgb */
    UsesCorsairMutex?: Exported<boolean>;
    /** @signalrgb */
    UsesUSBSensorMutex?: Exported<boolean>;
    /** @signalrgb */
    AllowConflictBypass?: Exported<boolean>;
    /** GPU SMBus plugins: the cards whose bus `Scan(bus)` is offered. @signalrgb */
    BrandGPUList?: Exported<GPUIdentifier[]>;

    // --- Lightloom extensions ------------------------------------------------------------
    /**
     * Generic HID drivers: `[[usagePage, usage], ...]` of the top-level collections the plugin
     * drives on any device no VID:PID plugin claims (no VendorId/ProductId needed).
     * @lightloom
     */
    HidUsages?: Exported<Array<[number, number]>>;
    /** SMBus plugins (required): the 7-bit addresses (0x03..0x77) Scan may touch. @lightloom */
    SmbusAddresses?: Exported<number[]>;
    /** Bus kinds `Scan(bus)` is offered (absent or "any": any). @lightloom */
    SmbusBuses?: Exported<SmbusBusKind[]>;
    /** Registers the host never lets the plugin write (save-to-flash and the like). @lightloom */
    SmbusDeniedRegisters?: Exported<number[]>;
    /** RAM plugins: DIMM manufacturers; Scan is offered only when one is present. @lightloom */
    SpdManufacturers?: Exported<SpdManufacturer[]>;
    /** Writes inside TransactionLock are queued and sent as one batch. @lightloom */
    SmbusBatchWrites?: Exported<boolean>;
    /**
     * What `SaveToDevice(request)` can store in the device's own memory ("Save to devices";
     * docs/host-api/save-shutdown.md). Repeated as `save` in manifest.json.
     * @lightloom
     */
    SaveCapabilities?: Exported<SaveCapabilities>;
    /**
     * The exit modes `Shutdown(systemSuspending, mode)` carries out itself; the others fall back
     * to `shutdownColor`. Repeated as `shutdownModes` in manifest.json.
     * @lightloom
     */
    ShutdownModes?: Exported<ShutdownMode[]>;

    // --- lifecycle (@signalrgb) ----------------------------------------------------------
    /** Accept one HID collection of the matched device; `device` is already bound. @signalrgb */
    Validate?(endpoint: HidEndpoint): boolean;
    /** SMBus: the addresses (from SmbusAddresses()) where a device of this plugin answers. @signalrgb */
    Scan?(bus: FreeAddressBus): number[];
    /** Once, after the device is open; parameter globals are bound. @signalrgb */
    Initialize?(): void;
    /** Every frame (SMBus sessions: at most every 33 ms). @signalrgb */
    Render?(): void;
    /**
     * On stop, removal and suspend; `systemSuspending` true: show black. Lightloom passes the exit
     * mode as a second argument (`undefined` elsewhere: treat as "restore"). @signalrgb
     */
    Shutdown?(systemSuspending?: boolean, mode?: ShutdownMode): void;
    /**
     * "Save to devices": store the frame's colours (`effect` null) or a built-in effect in the
     * device's own memory. Called only after the user confirmed it, at most once a minute per
     * device; a "preview" must not write (the host runs it dry anyway). Throw on a failed write.
     * @lightloom
     */
    SaveToDevice?(request: SaveRequest): SaveResult;
    /** Network plugins: the discovery service class/object. @signalrgb @unimplemented */
    DiscoveryService?: unknown;
    /** Called when the device brightness changes. @signalrgb */
    onBrightnessChanged?(): void;
  }

  /** What happens to the lights when Lightloom closes. @lightloom */
  type ShutdownMode = "restore" | "keep" | "off";

  /** `SaveCapabilities()`. @lightloom */
  interface SaveCapabilities {
    /** The frame's colours (one per zone or LED, as the device stores them). */
    static: boolean;
    /** Built-in effects the device can store. */
    effects: Array<"spectrum" | "breathing" | "wave" | "rainbow">;
  }

  /** The argument of `SaveToDevice()`. @lightloom */
  interface SaveRequest {
    action: "preview" | "save";
    /** One of `SaveCapabilities().effects`, or null for the frame's colours. */
    effect: string | null;
    /** The device's LEDs on the frame shown last, `LedPositions()` order. */
    colors: Array<[number, number, number]>;
  }

  /** What `SaveToDevice()` returns. @lightloom */
  type SaveResult =
    | { effect: string; zones: Array<{ name: string; color: string }>; note?: string }
    | { unsupported: string };

  // ------------------------------------------------------------------ host modules

  /** `@SignalRGB/Errors.js` / `@lightloom/Errors`. @signalrgb */
  interface ErrorsModule {
    Assert: {
      isOk<T>(value: T, message?: string, context?: unknown): T;
      isEqual(actual: unknown, expected: unknown, message?: string, context?: unknown): true;
      [check: string]: (...args: any[]) => unknown;
    };
    ContextError: new (message?: string, context?: unknown) => Error & { context: Record<string, unknown> };
    globalContext: {
      set(key: string, value: unknown): unknown;
      get(key: string): unknown;
      has(key: string): boolean;
      delete(key: string): boolean;
      clear(): void;
      keys(): string[];
      snapshot(): Record<string, unknown>;
    };
  }

  /** Every partner permission is granted in Lightloom (no partner gate). @signalrgb */
  interface PermissionsModule {
    permissions(): Record<string, { fans: boolean; lighting: boolean; macros: boolean }>;
    setCallback(handler: (permissions: Record<string, Record<string, boolean>>) => void): void;
  }

  /** Machine facts from the SMBIOS table; `undefined` where unknown. @signalrgb */
  interface SystemInfoModule {
    GetMotherboardInfo(): { manufacturer: string; model: string } | undefined;
    GetBiosInfo(): { vendor: string; version: string; releaseDate: string } | undefined;
    GetRamInfo(): Array<{
      manufacturer: string; partNumber: string; serialNumber: string; locator: string; capacity: number; speed: number;
    }>;
  }

  /** @signalrgb @unimplemented (the facade throws HostApiMissing) */
  interface SerialModule {
    connect(options?: {
      portName?: string; baudRate?: number; dataBits?: 5 | 6 | 7 | 8;
      stopBits?: "One" | "OneAndHalf" | "Two"; parity?: "None" | "Even" | "Odd" | "Mark" | "Space";
    }): boolean;
    disconnect(): void;
    isConnected(): boolean;
    write(data: Bytes): boolean | number;
    read(maxBytes?: number, timeout?: number): number[] | null;
    readAll(): number[];
    getDeviceInfo(portName?: string): { deviceId: string; vendorId: number; productId: number; portName: string };
    getPortName(): string;
    getBaudRate(): number;
    availablePorts(): Array<{ portName: string; vendorId: number; productId: number; [key: string]: unknown }>;
  }

  /** @signalrgb @unimplemented */
  interface LcdModule {
    initialize(options: { width: number; height: number; circularPreview?: boolean }): void;
    getFrame(options?: { format?: "RGB" | "RGB565" | "JPEG" | "NZXT::Q565" | "NZXT::RGB565" | string }): number[];
    getSlicedFrame(...args: unknown[]): number[];
    getImageBuffer(...args: unknown[]): number[];
  }

  /** @signalrgb @unimplemented */
  interface UdpModule {
    createSocket(): Socket;
    send(ip: string, port: number, data: Bytes): number;
  }

  /** @signalrgb @unimplemented */
  interface TcpModule {
    createSocket(): Socket;
  }

  /** @signalrgb @unimplemented */
  interface DeviceDiscoveryModule {
    foundVirtualDevice(device: { type: string; name: string; supported: boolean; vendorId?: number; [key: string]: unknown }): void;
  }

  /** @signalrgb @unimplemented */
  interface PerformanceModule {
    StartTimer(name: string): void;
    EndTimer(name: string): void;
    LogReport(): void;
    GetTimers(): Record<string, unknown>;
  }

  /** @signalrgb @unimplemented */
  interface AppInfoModule {
    readonly version: string;
    readonly name: string;
    [key: string]: unknown;
  }

  /** `BinaryUtils` (global, and `@lightloom/BinaryUtils`). */
  interface BinaryUtilsStatic {
    WriteInt16LittleEndian(value: number): [number, number];
    WriteInt16BigEndian(value: number): [number, number];
    WriteInt32LittleEndian(value: number): [number, number, number, number];
    WriteInt32BigEndian(value: number): [number, number, number, number];
    ReadInt16LittleEndian(bytes: Bytes): number;
    ReadInt16BigEndian(bytes: Bytes): number;
    ReadInt32LittleEndian(bytes: Bytes): number;
    ReadInt32BigEndian(bytes: Bytes): number;
  }

  /** The error every unimplemented member throws. */
  interface HostApiMissing extends Error {
    name: "HostApiMissing";
  }

  // ------------------------------------------------------------------ network-plugin globals

  /** Global `service` of network plugins. @signalrgb @stub */
  interface Service {
    name: string;
    controllers: Array<{ id: unknown; obj: unknown }>;
    log(value: unknown, options?: object): void;
    addController(controller: { id: string; [key: string]: unknown }): void;
    removeController(controller: { id: string }): void;
    updateController(controller: object): void;
    hasController(id: string): boolean;
    getController(id: string): unknown;
    getSetting(group: string, key: string): unknown;
    saveSetting(group: string, key: string, value: unknown): void;
    removeSetting(group: string, key: string): void;
    announceController(controller: object): void;
    suppressController(controller: object): void;
    broadcast(message: string): void;
    resolve(host: string): void;
    [member: string]: unknown;
  }

  /** `XmlHttp` helper global. @signalrgb @stub (requests never leave the host yet) */
  interface XmlHttpStatic {
    Get(url: string, callback: (xhr: XMLHttpRequestLike) => void, ...args: unknown[]): void;
    Post(url: string, callback: (xhr: XMLHttpRequestLike) => void, data?: unknown, ...args: unknown[]): void;
    Put(url: string, callback: (xhr: XMLHttpRequestLike) => void, data?: unknown, ...args: unknown[]): void;
    Delete(url: string, callback: (xhr: XMLHttpRequestLike) => void, ...args: unknown[]): void;
  }

  /** The `XMLHttpRequest` global: every request completes at once with status 0. @signalrgb @stub */
  interface XMLHttpRequestLike {
    readyState: number;
    status: number;
    statusText: string;
    response: string;
    responseText: string;
    responseType: string;
    timeout: number;
    onreadystatechange: (() => void) | null;
    onload: (() => void) | null;
    onerror: (() => void) | null;
    ontimeout: (() => void) | null;
    open(method: string, url: string, async?: boolean): void;
    setRequestHeader(name: string, value: string): void;
    getResponseHeader(name: string): string | null;
    getAllResponseHeaders(): string;
    abort(): void;
    send(body?: unknown): void;
  }

  // ------------------------------------------------------------------ effects (lightscripts)

  /**
   * `window.engine` of an effect page (lightloom-renderer src/inject.rs; docs/host-api/
   * effects-engine.md §1.6). Effects are HTML pages rendered by Chromium (they do have the DOM);
   * the inputs are zero until the matching input source runs.
   */
  interface EffectEngine {
    audio: {
      /** Spectrum bands. */
      freq: Int8Array | number[];
      /** Loudness, dB, -100..0. */
      level: number;
      rawlevel: number;
      /** Stereo width 0..1. */
      width: number;
      /** 0 = pure tone .. 1 = noise. */
      density: number;
    };
    zone: {
      /** 28 x 20 screen grid, hue 0..360, index = row * 28 + col. */
      hue: Int16Array;
      /** 0..100. */
      saturation: Int8Array;
      /** 0..100. */
      lightness: Int8Array;
      /** 160 x 100 RGBA downscaled screen. */
      imagedata: Uint8ClampedArray;
    };
    /** Screen meters by `<meta meter>` name. */
    vision: Record<string, number | number[]>;
    /** Sensors by display name. */
    sensors: Record<string, SensorReading>;
    /** `engine.sensors[name]`, or `null`. */
    getSensorValue(name: string): SensorReading | null;
    /** Media session. @signalrgb @unimplemented */
    getNowPlaying?(): {
      title: string; artist: string; album: string; sourceApp: string; position: number; duration: number;
      isPlaying: boolean; dominantColors: Array<{ r: number; g: number; b: number; hex: string }>;
    };
  }

  interface SensorReading {
    value: number;
    min: number;
    max: number;
    type: string;
    units?: string;
  }

  /**
   * Functions an effect page may define; the renderer calls them when they exist. Each
   * `<meta property="p">` control is a global `p` and may come with `on<p>Changed()`; after
   * load every control is assigned, every hook called once, then `onEngineReady()`.
   */
  interface EffectHooks {
    onEngineReady?(): void;
    /** A tap on the canvas (canvas pixels). */
    onCanvasTapped?(x: number, y: number): void;
    /** `POST /v1/canvas/event` or the compat `POST /canvas/event`. */
    onCanvasApiEvent?(event: { sender: string; event: string }): void;
  }
}

// ---------------------------------------------------------------------- plugin globals

declare var device: Lightloom.Device;
/**
 * SMBus device sessions: the bus bound to the device's address. HID sessions: every member
 * throws. During `Scan(bus)` it does not exist (use the argument).
 */
declare var bus: Lightloom.FixedAddressBus;
/** After `device.addFeature("battery")`. */
declare var battery: Lightloom.Battery;
/** After `device.addFeature("keyboard")`. */
declare var keyboard: Lightloom.Keyboard;
/** After `device.addFeature("mouse")`. */
declare var mouse: Lightloom.Mouse;
/** After `device.addFeature("base64")`. */
declare var base64: Lightloom.Base64;
/** After `device.addFeature("udp")` (throws in Lightloom device sessions). */
declare var udp: Lightloom.UdpFeature;
/** After `device.addFeature("dtls")` (throws in Lightloom device sessions). */
declare var dtls: Lightloom.DtlsFeature;
/** Network plugins. */
declare var service: Lightloom.Service;
/** Network plugins: the plugin's own controller object. */
declare var controller: Record<string, unknown>;
/** Network plugins: the plugin's DiscoveryService instance. */
declare var discovery: Record<string, unknown>;
declare var XmlHttp: Lightloom.XmlHttpStatic;
declare var BinaryUtils: Lightloom.BinaryUtilsStatic;
/** Effect pages only (Chromium), not device plugins. */
declare var engine: Lightloom.EffectEngine;

// ---------------------------------------------------------------------- module specifiers
// `@SignalRGB/<name>` (and `<name>.js`) and the `@lightloom/<name>` alias resolve to the same
// host module; BinaryUtils, createColorArray and hexToRgb exist only under `@lightloom/`.

declare module "@SignalRGB/Errors.js" { const m: Lightloom.ErrorsModule; export default m; export const Assert: Lightloom.ErrorsModule["Assert"]; export const ContextError: Lightloom.ErrorsModule["ContextError"]; export const globalContext: Lightloom.ErrorsModule["globalContext"]; }
declare module "@SignalRGB/Errors" { export * from "@SignalRGB/Errors.js"; export { default } from "@SignalRGB/Errors.js"; }
declare module "@lightloom/Errors" { export * from "@SignalRGB/Errors.js"; export { default } from "@SignalRGB/Errors.js"; }
declare module "@lightloom/Errors.js" { export * from "@SignalRGB/Errors.js"; export { default } from "@SignalRGB/Errors.js"; }
declare module "@SignalRGB/base64" {
  export function encode(bytes: Lightloom.Bytes, urlSafe?: boolean): string;
  export function decode(text: string): number[];
  export function decodeToString(text: string): string;
  export const Encode: typeof encode;
  export const Decode: typeof decode;
  const m: Lightloom.Base64 & { decodeToString(text: string): string };
  export default m;
}
declare module "@lightloom/base64" { export * from "@SignalRGB/base64"; export { default } from "@SignalRGB/base64"; }
declare module "@SignalRGB/permissions" { const m: Lightloom.PermissionsModule; export default m; }
declare module "@lightloom/permissions" { const m: Lightloom.PermissionsModule; export default m; }
declare module "@SignalRGB/systeminfo" { const m: Lightloom.SystemInfoModule; export default m; }
declare module "@lightloom/systeminfo" { const m: Lightloom.SystemInfoModule; export default m; }
declare module "@SignalRGB/serial" { const m: Lightloom.SerialModule; export default m; }
declare module "@lightloom/serial" { const m: Lightloom.SerialModule; export default m; }
declare module "@SignalRGB/lcd" { const m: Lightloom.LcdModule; export default m; }
declare module "@lightloom/lcd" { const m: Lightloom.LcdModule; export default m; }
declare module "@SignalRGB/udp" { const m: Lightloom.UdpModule; export default m; }
declare module "@lightloom/udp" { const m: Lightloom.UdpModule; export default m; }
declare module "@SignalRGB/tcp" { const m: Lightloom.TcpModule; export default m; }
declare module "@lightloom/tcp" { const m: Lightloom.TcpModule; export default m; }
declare module "@SignalRGB/DeviceDiscovery" { const m: Lightloom.DeviceDiscoveryModule; export default m; }
declare module "@lightloom/DeviceDiscovery" { const m: Lightloom.DeviceDiscoveryModule; export default m; }
declare module "@SignalRGB/performance" { const m: Lightloom.PerformanceModule; export default m; }
declare module "@lightloom/performance" { const m: Lightloom.PerformanceModule; export default m; }
declare module "@SignalRGB/appInfo" { const m: Lightloom.AppInfoModule; export default m; }
declare module "@lightloom/appInfo" { const m: Lightloom.AppInfoModule; export default m; }

/** @lightloom */
declare module "@lightloom/BinaryUtils" {
  export const BinaryUtils: Lightloom.BinaryUtilsStatic;
  export const WriteInt16LittleEndian: Lightloom.BinaryUtilsStatic["WriteInt16LittleEndian"];
  export const WriteInt16BigEndian: Lightloom.BinaryUtilsStatic["WriteInt16BigEndian"];
  export const WriteInt32LittleEndian: Lightloom.BinaryUtilsStatic["WriteInt32LittleEndian"];
  export const WriteInt32BigEndian: Lightloom.BinaryUtilsStatic["WriteInt32BigEndian"];
  export const ReadInt16LittleEndian: Lightloom.BinaryUtilsStatic["ReadInt16LittleEndian"];
  export const ReadInt16BigEndian: Lightloom.BinaryUtilsStatic["ReadInt16BigEndian"];
  export const ReadInt32LittleEndian: Lightloom.BinaryUtilsStatic["ReadInt32LittleEndian"];
  export const ReadInt32BigEndian: Lightloom.BinaryUtilsStatic["ReadInt32BigEndian"];
  export default BinaryUtils;
}

/** @lightloom */
declare module "@lightloom/createColorArray" {
  export const LAYOUTS: readonly Lightloom.ColorLayout[];
  export const COLOR_ORDERS: readonly Lightloom.ColorOrder[];
  export function createColorArray(color: Lightloom.HexColor, count: number, layout?: Lightloom.ColorLayout, order?: Lightloom.ColorOrder | string): Lightloom.ColorArray;
  export function layoutColors(triples: Lightloom.RGB[], layout?: Lightloom.ColorLayout, order?: Lightloom.ColorOrder | string): Lightloom.ColorArray;
  export function layoutFlatColors(flat: Lightloom.Bytes, layout?: Lightloom.ColorLayout, order?: Lightloom.ColorOrder | string): Lightloom.ColorArray;
  export function reorderColor(rgb: Lightloom.RGB, order?: Lightloom.ColorOrder | string): Lightloom.RGB;
  export function normalizeColorOrder(order: string): Lightloom.ColorOrder;
  export function normalizeLayout(layout: string): Lightloom.ColorLayout;
  export default createColorArray;
}

/** @lightloom */
declare module "@lightloom/hexToRgb" {
  export function hexToRgb(hex: string): Lightloom.RGB;
  export function rgbToHex(r: number, g: number, b: number): Lightloom.HexColor;
  export function isHexColor(value: unknown): boolean;
  export function clampByte(n: number): number;
  export function normalizeHex(hex: string): Lightloom.HexColor;
  export default hexToRgb;
}
