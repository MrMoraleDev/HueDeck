// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin data: the Razer products with a battery, for the battery queries of plugin.js.
// Hand-written (devices.js is generated from OpenRGB, which reads no battery, so this table lives
// apart from it). The product ids are those of devices.js; which of them answer the battery
// queries, and with which transaction id, are protocol facts taken from OpenRazer's kernel driver
// (https://github.com/openrazer/openrazer, driver/razermouse_driver.c and razerkbd_driver.c, the
// device lists of razer_attr_read_charge_level() / razer_attr_read_charge_status();
// GPL-2.0-or-later). No OpenRazer code is copied. docs/hardware/razer.md, "Battery".
//
// [product id]: { transactionId, chargingStatus }. chargingStatus false: the product answers no
// charging-status query (OpenRazer reports a fixed 0 for it), so only the level is read.

export const BATTERY = {
  // Mice (razermouse_driver.c)
  0x0024: { transactionId: 0xFF, chargingStatus: true },   // Mamba 2012 (Wired)
  0x0025: { transactionId: 0xFF, chargingStatus: true },   // Mamba 2012 (Wireless)
  0x003E: { transactionId: 0xFF, chargingStatus: true },   // Naga Epic Chroma
  0x0044: { transactionId: 0xFF, chargingStatus: true },   // Mamba 2015 (Wired)
  0x0045: { transactionId: 0xFF, chargingStatus: true },   // Mamba (Wireless)
  0x0059: { transactionId: 0x3F, chargingStatus: true },   // Lancehead 2017 (Wired)
  0x005A: { transactionId: 0x3F, chargingStatus: true },   // Lancehead 2017 (Wireless)
  0x006F: { transactionId: 0x1F, chargingStatus: true },   // Lancehead 2019 (Wireless)
  0x0070: { transactionId: 0x1F, chargingStatus: true },   // Lancehead 2019 (Wired)
  0x0072: { transactionId: 0x3F, chargingStatus: true },   // Mamba 2018 (Wireless)
  0x0073: { transactionId: 0x3F, chargingStatus: true },   // Mamba 2018 (Wired)
  0x007A: { transactionId: 0xFF, chargingStatus: true },   // Viper Ultimate (Wired)
  0x007B: { transactionId: 0xFF, chargingStatus: true },   // Viper Ultimate (Wireless)
  0x007C: { transactionId: 0x3F, chargingStatus: true },   // DeathAdder V2 Pro (Wired)
  0x007D: { transactionId: 0x3F, chargingStatus: true },   // DeathAdder V2 Pro (Wireless)
  0x0086: { transactionId: 0x1F, chargingStatus: true },   // Basilisk Ultimate (Wired)
  0x0088: { transactionId: 0x1F, chargingStatus: true },   // Basilisk Ultimate (Receiver)
  0x008F: { transactionId: 0x1F, chargingStatus: true },   // Naga Pro (Wired)
  0x0090: { transactionId: 0x1F, chargingStatus: true },   // Naga Pro (Wireless)
  0x00A7: { transactionId: 0x1F, chargingStatus: true },   // Naga Pro V2 (Wired)
  0x00A8: { transactionId: 0x1F, chargingStatus: true },   // Naga Pro V2 (Wireless)
  0x00AA: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro (Wired)
  0x00AB: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro (Wireless)
  0x00AF: { transactionId: 0x1F, chargingStatus: true },   // Cobra Pro (Wired)
  0x00B0: { transactionId: 0x1F, chargingStatus: true },   // Cobra Pro (Wireless)
  0x00B9: { transactionId: 0x1F, chargingStatus: false },  // Basilisk V3 X HyperSpeed
  0x00CC: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro 35K (Wired)
  0x00CD: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro 35K (Wireless)
  0x00D6: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro 35K Phantom Green (Wired)
  0x00D7: { transactionId: 0x1F, chargingStatus: true },   // Basilisk V3 Pro 35K Phantom Green (Wireless)
  // Keyboards (razerkbd_driver.c)
  0x0258: { transactionId: 0x1F, chargingStatus: true },   // BlackWidow V3 Mini HyperSpeed (Wired)
  0x0271: { transactionId: 0x9F, chargingStatus: true },   // BlackWidow V3 Mini HyperSpeed (Wireless)
  0x025A: { transactionId: 0x3F, chargingStatus: true },   // BlackWidow V3 Pro (Wired)
  0x025C: { transactionId: 0x9F, chargingStatus: true },   // BlackWidow V3 Pro (Wireless)
  0x0290: { transactionId: 0x9F, chargingStatus: true },   // DeathStalker V2 Pro (Wireless)
  0x0292: { transactionId: 0x1F, chargingStatus: true },   // DeathStalker V2 Pro (Wired)
  0x0296: { transactionId: 0x9F, chargingStatus: true },   // DeathStalker V2 Pro TKL (Wireless)
  0x0298: { transactionId: 0x1F, chargingStatus: true },   // DeathStalker V2 Pro TKL (Wired)
  0x02D5: { transactionId: 0x9F, chargingStatus: true },   // BlackWidow V4 TKL HyperSpeed (Wireless)
  0x02D7: { transactionId: 0x1F, chargingStatus: true },   // BlackWidow V4 TKL HyperSpeed (Wired)
};
