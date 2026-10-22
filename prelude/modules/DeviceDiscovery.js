// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/DeviceDiscovery" (Rust-backed: reports a detected
// but unsupported/virtual device to the registry so the UI can list it).
//
// Corpus (2026-10-17, 1006 plugin .js files): 32 files import it
// (`import DeviceDiscovery from "@SignalRGB/DeviceDiscovery"`). Member calls:
//   DeviceDiscovery.foundVirtualDevice({type, name, supported: bool, vendorId[, productId]})  42
// Exe moc: DeviceDiscoveryModule signal foundVirtualDevice(QJSValue deviceInfo).
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["foundVirtualDevice"]);

const DeviceDiscovery = createFacade("@SignalRGB/DeviceDiscovery", MEMBERS, {
  hint: "virtual-device reporting arrives with the lightloom-engine registry (M2)",
});

export default DeviceDiscovery;
