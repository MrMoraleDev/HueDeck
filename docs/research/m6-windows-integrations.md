<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- Research brief for M6. Produced 2026-10-18 by a research pass and an independent fact-check pass; facts carry inline provenance tags. -->

# HueDeck M6: Windows integrations brief (Dynamic Lighting/LampArray, OpenRGB SDK, CM hotplug and device identity), independently re-verified

Verification pass: 2026-10-18.

**Pinned targets** (all in the repo `Cargo.lock`) [V-local]:
- `windows` 0.62.2
- `windows-core` 0.62.2
- `windows-future` 0.3.2
- `windows-numerics` 0.3.1
- `hidapi` 2.6.7

**Test machine:** Windows 11 Pro 25H2, build 26200.9711 [V-local]. The registry `ProductName` value still reads "Windows 10 Pro"; this is a known quirk.

**OpenRGB:** 1.1, released 2026-10-16 [V:S25,S41].

**Legend**
- **[V:Sx]** re-verified in this pass against source Sx (§6).
- **[V-local]** verified on this machine in this pass, either with read-only PowerShell WinRT/PnP/Appx/registry queries or by rebuilding and re-running the read-only probe.
- **[U]** unverified or inferred. Test it before you depend on it.
- **[design]** a proposal, not a fact.

OpenRGB is GPL. Its source and documentation were read for facts (numbers, layouts, behaviour) only. Copy none of it.

---

## 0. Decisions on one screen

### 0.1 LampArray backend: WinRT `Windows.Devices.Lights` inside huedeckd [design]

- **Background control needs package identity.** Background ("ambient") control requires package identity plus the `com.microsoft.windows.lighting` AppExtension. Microsoft: "enforced by the AmbientLightingServer, which only accepts connections from an AmbientLightingClient in a process with package identity (packaged app) supporting the extension" [V:S3].
- **Without identity, expect no visible output.**
  - S4 says: "An application can set lamp state at any time, but state will only be applied by the system while application is in focus." This is a general, RS5-era statement.
  - Combined with S3, a windowless, unpackaged huedeckd can enumerate and describe lamps, but its writes are not expected to be applied [inference; runtime U].
  - HueDeck's UI is a separate process, so even UI focus probably does not help [U].
- **Therefore:** ship a signed identity package with external location (a sparse MSIX) for `huedeckd.exe`. SignalRGB uses the same mechanism on this machine [V-local; V:S8].

### 0.2 A plugin beats the LampArray backend on the same physical device [design]

- **Deduplicate** by USB device instance id (§3).
- **Exclude HID TLCs** with UsagePage 0x59 / Usage 0x01 from the HID plugin scan. SignalRGB does the same: "Skipping blacklisted HID path: {}" + "LAMPARRAY" [V:S40; V-local strings].
- **Conflict to resolve.** The approved plan lists a first-party `generic-lamparray` plugin. With the 0x59 exclusion, that plugin could not open the collection [V:S54]. Pick one; the recommendation is the native backend only.
- **Acceptance-test device.** On this machine the Razer DeathAdder V2 Pro (1532:007C) is both plugin-supported and the only LampArray (`MI_03`) [V-local]. Use it instead of "a DL-certified keyboard" in the M6 acceptance test.

### 0.3 OpenRGB SDK: clean-room codec from the documented wire format [design]

- **Advertise max protocol 5 at first.** v6 (OpenRGB 1.0/1.1: unique ids, ACKs, flags) comes later.
  - OpenRGB 1.1's own client negotiates down cleanly. It waits about 1 s for the version reply, then sends SET_CLIENT_FLAGS (52) and SET_CLIENT_HOSTNAME (54) only if the negotiated version is 6 or higher [V:S42].
- **Bind `127.0.0.1:6742` by default.**
  - "Refuse on AddrInUse" alone is not enough on Windows. If a process under the same user is already bound to `0.0.0.0:6742`, a later bind to `127.0.0.1:6742` succeeds [V:S48].
  - OpenRGB 0.8 through 1.0rc3 bind `0.0.0.0` by default [V:S43].
  - Probe-connect first (§2.6).
- **Client bridge:** uses the same codec.
  - Require OpenRGB ≥ 1.1 when speaking v≤5. The 1.1 release notes say: "Fix an issue with the OpenRGB SDK server device indices on protocol versions < 6" [V:S41].
- **No permissive Rust protocol crate exists:**
  - `openrgb` 0.1.2 (2022-05-14) is GPL-2.0 [V:S26].
  - `openrgb2` 0.4.1 (2026-09-05) is GPL-2.0 [V:S26].

### 0.4 Hotplug [design; the facts below are verified]

- **Registrations.** Two `CM_Register_Notification` registrations, one for `GUID_DEVINTERFACE_HID` and one for `GUID_DEVINTERFACE_USB_DEVICE` [V:S1,S29].
- **Callback and reconciler.**
  - The callback copies the symbolic link onto a channel.
  - A reconciler applies a 500 ms quiet window per group. `architecture.md` already says "debounced 500 ms" [V:S54].
- **Group key.** The first ancestor `USB\VID_…` without `&MI_`, reached by walking through any enumerator.
  - The interface-id format `USB\VID_v(4)&PID_d(4)&MI_z(2)` is documented [V:S46].
  - The vendor bus `RZVIRTUAL\` exists here [V-local].
- **Per-handle registrations.** Add one `CM_NOTIFY_FILTER_TYPE_DEVICEHANDLE` registration for every HID/USB handle HueDeck keeps open, and close the handle on QUERYREMOVE. An open handle "prevents the query remove of this device from succeeding" [V:S31,S32].
- **Never panic in callbacks.** Since Rust 1.81, "the non-unwind ABIs (e.g., "C") will now abort on uncaught unwinds"; `extern "system"` is one of them [V:S50].

---

## 1. Windows Dynamic Lighting and LampArray

### 1.1 Platform facts

| Fact | Status |
|---|---|
| "Currently, only HID LampArrays are supported." | [V:S4] |
| Foreground control: "Win10 version 1809 (October 2018) and later … foreground UWP and Win32 apps" (10.0.17763, docs tag RS5). Background: "Windows 11 Build 23466 (Preview) and later … foreground and background (ambient) UWP and Win32 apps." | [V:S3] |
| The AmbientLightingServer quote above. Unpackaged apps should use "Sparse Packaging and External Location". "For unpackaged app installations, there is also a side-by-side application manifest requirement for your executable." | [V:S3] |
| **Priority rules:**<br>• "By default, a foreground app is always assigned control of a LampArray device unless the user has specified otherwise in Settings."<br>• Among ambient apps, control goes to "the app prioritized in Settings".<br>• Toggles: *Use Dynamic Lighting on my devices*, *Compatible apps in the foreground always control lighting*, *Background light control* (drag an app to the top).<br>• "Ambient background settings are tied to a device and the port it's connected on"; another USB port makes it "a different device".<br>• A device not selected for background control runs in "Autonomous mode". | [V:S3] |
| `IsAvailable`: "Gets whether the lamp array is available or unavailable for control by this process." Device family: "Windows 11 Insider Preview (introduced in 10.0.23504.0)". API contract: "Windows.Foundation.UniversalApiContract (introduced in v15.0)". The C# and C++/WinRT projections show `ExperimentalAttribute`. | [V:S5] |
| **The event is `AvailabilityChanged`** (no "IsAvailableChanged" exists):<br>• "Is raised when the value of IsAvailable changes; which can happen when the user changes system access to the device through the device settings."<br>• Its args object is "always null".<br>• "Both foreground and background ("ambient") apps can receive and handle this event."<br>• "To use this event, you must declare the "com.microsoft.windows.lighting" AppExtension".<br>• `IsAvailable` and `AvailabilityChanged` live on `ILampArray2` (IID `050c181f-60a8-4711-a1af-1b1b4c658ea2`). | [V:S6,S1] |
| **Dynamic Lighting GA ("Moment 4"):**<br>• Microsoft's 22H2 update history lists "September 26, 2023—KB5030310 (OS Build 22621.2361) Preview".<br>• The KB5030310 notes do not mention Dynamic Lighting; Moment 4 features arrived via controlled feature rollout.<br>• The blog post "Dynamic Lighting is now available on Windows 11" is dated September 27, 2023 (as fetched).<br>• Min build 22621.2361 comes only from ASUS's FAQ and Neowin. | Dates/titles [V:S44,S18]; minimum build [U] |
| `LampArray.GetDeviceSelector()` here returns the string below this table. A LampArray is therefore the HID TLC 0x59/0x01, and `DeviceInformation.Id` is the HID interface path. | [V-local] |
| The only LampArray present: `\\?\HID#VID_1532&PID_007C&MI_03#8&5cc7230&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}`, named "Razer DeathAdder V2 Pro". | [V-local] |
| `LampInfo.Position`: meters, origin at the front-top-left corner of `BoundingBox`, left-handed (+X right, +Y down, +Z away from the user). The HID device reports µm and WinRT divides by 1,000,000. `BoundingBox`: X = width, Y = height, Z = depth, in meters. | [V:S7] |
| `MinUpdateInterval`: "Minimal interval (required by the device) between the system sending two updates for any one lamp." `TimeSpan.Duration` is "in 100-nanosecond units". | [V:S7] |
| **`SetColorsForIndices`** (available since 17763):<br>• "Size of desiredColors and lampIndexes arrays must be identical or call will fail."<br>• An unsupported color snaps to `NearestSupportedColor`.<br>• A non-existent index "is ignored and the remaining are still applied".<br>• Black = off. | [V:S45] |
| **HID usages, page 0x59** (all Feature reports in Microsoft's sample):<br>• LampArray 0x01<br>• LampArrayAttributesReport 0x02<br>• LampCount 0x03<br>• BoundingBox W/H/D µm 0x04–0x06<br>• LampArrayKind 0x07<br>• MinUpdateIntervalInMicroseconds 0x08<br>• LampAttributesRequestReport 0x20<br>• LampId 0x21<br>• LampAttributesResponseReport 0x22<br>• PositionX/Y/Z µm 0x23–0x25<br>• LampPurposes 0x26<br>• UpdateLatencyInMicroseconds 0x27<br>• Red/Green/Blue/IntensityLevelCount 0x28–0x2B<br>• IsProgrammable 0x2C<br>• InputBinding 0x2D<br>• LampMultiUpdateReport 0x50<br>• Red/Green/Blue/IntensityUpdateChannel 0x51–0x54<br>• LampUpdateFlags 0x55<br>• LampRangeUpdateReport 0x60<br>• LampIdStart/End 0x61/0x62<br>• LampArrayControlReport 0x70<br>• AutonomousMode 0x71 | [V:S14] (MIT; generated from HUT 1.4) |
| **Exporting HueDeck devices to Windows is out of scope** (ADR-0007):<br>• Devices become compatible "via a native FW implementation, or through a VHF driver".<br>• "VHF supports a HID source driver only in kernel mode." | [V:S3b,S46] |
| **SignalRGB's identity package, observed read-only:**<br>• `SignalRGB_1.0.2.6_x64__4n6rtbnm9e9y2`, family `SignalRGB_4n6rtbnm9e9y2`, `SignatureKind=Developer`, installed under `C:\Program Files\WindowsApps\…`.<br>• Manifest: `ProcessorArchitecture="x64"`, `uap10:AllowExternalContent=true`, rescap `runFullTrust` + `unvirtualizedResources`, `TargetDeviceFamily MinVersion=MaxVersionTested=10.0.19000.0`, `IgnorableNamespaces="uap uap2 uap3 rescap desktop uap10"`.<br>• `<Application Id="SignalRGB" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">` with **no `Executable` attribute**, `AppListEntry="none"`, and `<uap3:AppExtension Name="com.microsoft.windows.lighting" Id="Id" PublicFolder="public" DisplayName="Id"/>`.<br>• `Publisher` is the full EV DN (`… OID.2.5.4.15=Private Organization, … SERIALNUMBER=3817822`).<br>• The exe embeds `<msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="CN=SignalRGB (Whirlwind…)…" packageName="SignalRGB" applicationId="SignalRGB">`. | [V-local] |
| **OS state** (undocumented; observed only):<br>• `HKCU\Software\Microsoft\Lighting`: `AmbientLightingEnabled=0`, `ControlledByForegroundApp=0`, `Brightness=100`, `EffectType=0`, `EffectMode=0`, `Speed=7`, `UseSystemAccentColor=1`, `Color2=4278190080`. Reading `AmbientLightingEnabled=0` as "global toggle off" is an inference.<br>• `…\Lighting\Providers`: `1=WindowsLighting`, `2=SignalRGB_4n6rtbnm9e9y2`.<br>• `…\Lighting\Devices` holds **two** keys, `HID#VID_1532&PID_007D&MI_03#9&242d05be&0&0000#{4d1e55b2-…}` and `HID#VID_1532&PID_007D&MI_04#9&55c7fc&0&0000#{…}`. Each has `AmbientLightingEnabled`, `EffectType`, `Brightness`, `Speed`, `UseSystemAccentColor`, `EffectMode`, `Color`, `Color2`, `ToastInvoked`, `ControlledByForegroundApp`, plus its own `Providers` subkey (same two entries).<br>• There is **no key for the currently connected PID_007C LampArray**. | [V-local] |
| **SignalRGB and the WDL registry:**<br>• Its strings contain QSettings-style paths: `HKEY_CURRENT_USER\Software\Microsoft\Lighting\Devices`, `Devices/%1/AmbientLightingEnabled`, `Devices/%1/ControlledByForegroundApp`, `Devices/%1/Providers/1`.<br>• These sit next to the logs "Dynamic Lighting Device {} has an available plugin. Disabling Windows Dynamic Lighting for it" and "Disabled WDL for device".<br>• That SignalRGB writes there is strongly implied but not observed. The repo spec says exact keys are UNKNOWN. | [V-local strings; V:S40]; write [U] |
| OEM unattend setting `DefaultLightingProvider`: "the package family name of a preinstalled application that should be set as the default provider for Dynamic Lighting. This application must declare the "com.microsoft.windows.lighting" AppExtension". Consistent with the Providers lists holding package family names. | [V:S12]; interpretation [U] |
| Settings deep link: `ms-settings:personalization-lighting`, labelled "Dynamic Lighting". | [V:S13] |

The selector string returned on this machine:

```
System.Devices.InterfaceClassGuid:="{4D1E55B2-F16F-11CF-88CB-001111000030}" AND System.Devices.InterfaceEnabled:=System.StructuredQueryType.Boolean#True AND System.DeviceInterface.Hid.UsagePage:=89 AND System.DeviceInterface.Hid.UsageId:=1
```

### 1.2 Exact `windows` 0.62.2 surface (re-read from local registry sources) [V:S1]

**Feature implications:**
- `Devices_Lights = ["Devices"]` and `Devices = ["Foundation"]`, so `Foundation` is implied; listing it is harmless.
- Likewise `UI`, `System`, `Management` and `ApplicationModel` each pull in `Foundation`.

**LampArray core: `Devices_Enumeration`, `Devices_Lights`, `UI`.**
- Every `SetColor*`, `LampInfo::FixedColor` and `GetNearestSupportedColor` is `#[cfg(feature = "UI")]`.
- So is every Effects `Color`/`SetColor` accessor.

**Optional features:**
- `Devices_Lights_Effects`: playlists and the custom effect.
- `System`: `GetIndicesForKey`. `SetColorsForKey(s)` needs `System` **and** `UI`.
- `Storage_Streams`: `SendMessageAsync` / `RequestMessageAsync`.
- `Graphics_Imaging`: `LampArrayBitmapRequestedEventArgs::UpdateBitmap`.

**Identity:**
- `Management_Deployment` (pulls in `Foundation`, which provides `Uri`).
- `Win32_Storage_Packaging_Appx` for `GetCurrentPackageFullName(*mut u32, Option<PWSTR>) -> WIN32_ERROR`.
- `APPMODEL_ERROR_NO_PACKAGE = WIN32_ERROR(15700)` lives in `Win32::Foundation`.
- Add `ApplicationModel` only if you use `Package::Current`.

**Helper crates and conversions:**
- `windows` does **not** re-export `windows-numerics` or `windows-future`. `windows::Foundation::Numerics` only has `Plane`, `Quaternion` and `Rational`.
- Inherent methods (`.join()`, `.when(..)`) and `.await` work without naming those crates.
- To name `Vector3` or `IAsyncOperation` in HueDeck types, add direct dependencies `windows-numerics = "0.3"` and/or `windows-future = "0.3"`. The lockfile already resolves them to 0.3.1 and 0.3.2.
- `windows-future` 0.3.2 provides:
  - `join()`, which blocks;
  - `when<F: FnOnce(Result<T>) + Send + 'static>`;
  - `IntoFuture` when its `std` feature is on. The `windows` default `std` feature enables `windows-future/std`.
- `From<TimeSpan> for core::time::Duration` and its reverse exist in `extensions/Foundation/TimeSpan.rs`.

```rust
// windows-0.62.2/src/Windows/Devices/Lights/mod.rs  [V:S1]
impl LampArray {
  pub fn GetDeviceSelector() -> Result<HSTRING>;
  pub fn FromIdAsync(deviceid: &HSTRING) -> Result<IAsyncOperation<LampArray>>;
  pub fn DeviceId(&self) -> Result<HSTRING>;
  pub fn HardwareVendorId(&self) -> Result<u16>;   // also HardwareProductId / HardwareVersion -> u16
  pub fn LampArrayKind(&self) -> Result<LampArrayKind>; // Undefined0 Keyboard1 Mouse2 GameController3 Peripheral4 Scene5
                                                         // Notification6 Chassis7 Wearable8 Furniture9 Art10 Headset11 Microphone12 Speaker13
  pub fn LampCount(&self) -> Result<i32>;
  pub fn MinUpdateInterval(&self) -> Result<TimeSpan>;              // Duration: i64, 100 ns; Duration::from(ts) works
  pub fn BoundingBox(&self) -> Result<windows_numerics::Vector3>;   // meters {X,Y,Z}
  pub fn IsEnabled(&self) -> Result<bool>;  pub fn SetIsEnabled(&self, bool) -> Result<()>;
  pub fn BrightnessLevel(&self) -> Result<f64>; pub fn SetBrightnessLevel(&self, f64) -> Result<()>;  // (class Lamp uses f32)
  pub fn IsConnected(&self) -> Result<bool>; pub fn SupportsVirtualKeys(&self) -> Result<bool>;
  pub fn GetLampInfo(&self, lampindex: i32) -> Result<LampInfo>;
  pub fn GetIndicesForPurposes(&self, LampPurposes) -> Result<windows_core::Array<i32>>;
  #[cfg(feature="System")] pub fn GetIndicesForKey(&self, VirtualKey) -> Result<Array<i32>>;
  #[cfg(feature="UI")] pub fn SetColor(&self, Color) -> Result<()>;
  #[cfg(feature="UI")] pub fn SetColorForIndex(&self, i32, Color) -> Result<()>;
  #[cfg(feature="UI")] pub fn SetSingleColorForIndices(&self, Color, &[i32]) -> Result<()>;
  #[cfg(feature="UI")] pub fn SetColorsForIndices(&self, desiredcolors: &[Color], lampindexes: &[i32]) -> Result<()>;
  #[cfg(feature="UI")] pub fn SetColorsForPurposes(&self, Color, LampPurposes) -> Result<()>;
  #[cfg(all(feature="System", feature="UI"))] pub fn SetColorsForKey(s)(..);
  pub fn IsAvailable(&self) -> Result<bool>;   // casts to ILampArray2 -> Err(E_NOINTERFACE) where ILampArray2 is absent
  pub fn AvailabilityChanged<P0: Param<TypedEventHandler<LampArray, IInspectable>>>(&self, P0) -> Result<i64>;
  pub fn RemoveAvailabilityChanged(&self, token: i64) -> Result<()>;
}
// unsafe impl Send + Sync for LampArray / LampInfo
impl LampInfo { Index()->i32; Purposes()->LampPurposes; Position()->Vector3; Red/Green/Blue/GainLevelCount()->i32;
                FixedColor()->IReference<Color> /*UI; Err(Error::empty()) when the lamp has no fixed color*/;
                GetNearestSupportedColor(Color)->Color /*UI*/; UpdateLatency()->TimeSpan }
// LampPurposes(u32): Undefined0 Control1 Accent2 Branding4 Status8 Illumination16 Presentation32 (contains(), BitOr, ...)
// windows::UI::Color { A: u8, R: u8, G: u8, B: u8 }
// TypedEventHandler::<S,R>::new(F: Fn(Ref<S>, Ref<R>) -> Result<()> + Send + 'static); Ref::as_ref() -> Option<&T>
// DeviceInformation::FindAllAsync() (all devices) | FindAllAsyncAqsFilter(&HSTRING) <- this is "FindAllAsync(selector)"
//   | FindAllAsyncAqsFilterAndAdditionalProperties(..) -> IAsyncOperation<DeviceInformationCollection> (IntoIterator for &coll)
// DeviceInformation::CreateWatcherAqsFilter(&HSTRING) -> DeviceWatcher { Added, Updated, Removed, EnumerationCompleted, Stopped, Start, Stop }
// Effects (Devices_Lights_Effects):
//   LampArrayCustomEffect::CreateInstance<P0: Param<LampArray>>(P0, &[i32]); SetDuration/SetUpdateInterval(TimeSpan);
//   UpdateRequested(TypedEventHandler<LampArrayCustomEffect, LampArrayUpdateRequestedEventArgs>) / RemoveUpdateRequested
//     args: SinceStarted, SetColor, SetColorForIndex, SetSingleColorForIndices, SetColorsForIndices (UI);
//   LampArraySolidEffect / ColorRampEffect / BlinkEffect / BitmapEffect (BitmapRequested; UpdateBitmap needs Graphics_Imaging);
//   LampArrayEffectPlaylist::new/Append/Start/Stop/Pause + statics StartAll/StopAll/PauseAll;
//   CompletionBehavior ClearState0/KeepState1; StartMode Sequential0/Simultaneous1; RepetitionMode Occurrences0/Forever1.
```

**Threading and errors:**
- **No `RoInitialize` needed on worker threads.**
  - In `windows-core` 0.62.2, `imp/factory_cache.rs::load_factory` handles a `CO_E_NOTINITIALIZED` result from `RoGetActivationFactory` by calling `CoIncrementMTAUsage` and retrying [V:S1].
- **WinRT handlers run on system threads.**
  - In this pass's re-run, `DeviceWatcher::Added` ran on tid 7460 while main was tid 24580 (the earlier run: 4552 vs 15932) [V-local].
- **`DeviceWatcher` must subscribe to Added, Removed and Updated.** Otherwise "it will not receive an update if a device is added to the system after the initial device enumeration completes" [V:S16].
- **A null interface result is `Err(Error::empty())`**, per `windows_core::Type::from_abi`. Two cases matter here:
  - `FixedColor()` on a normal lamp.
  - A `FromIdAsync` that resolves to null.

  Treat both as "none", not as a failure [V:S1].
- **A panic inside a handler closure aborts the process** (Rust ≥ 1.81). Return `Err` instead [V:S50].

### 1.3 How HueDeck should use it [design unless marked]

- **No identity** (dev builds, or users who skip registration):
  - Enumerate and describe LampArrays for dedupe and the UI, but do not drive them.
  - Show a "Let HueDeck control Windows Dynamic Lighting devices" call to action.
  - `DeviceWatcher` and `FindAllAsyncAqsFilter` work unpackaged: the probe ran unpackaged and received `Added` [V-local].
- **With identity:**
  - Subscribe to `AvailabilityChanged` per device.
  - `IsAvailable == false` means paused: skip writes and show the reason. On `true`, resend a full frame. Microsoft's own sample does this: if `ApiInformation.IsPropertyPresent(…"IsAvailable")`, check `IsAvailable` and subscribe; otherwise just drive the device [V:S6].
  - Tell the user to drag HueDeck to the top of **Background light control**. `WindowsLighting` is entry 1 in the observed provider lists [V-local].
  - Optionally have them turn off "Compatible apps in the foreground always control lighting" [V:S3].
  - Deep-link with `ms-settings:personalization-lighting` [V:S13].
- **Do not write `HKCU\Software\Microsoft\Lighting`.** It is undocumented, and `architecture.md` says "No registry writes" [V:S54]. Reading it for diagnostics only is optional.
- **Gate the backend at runtime, not only by build number.**
  - Try `IsAvailable()`. On `Err` (no `ILampArray2`), assume "available" and rely on write errors. Ambient support needs build 23466+ (preview) or 22621.2361+ GA [U].

### 1.4 Identity package recipe (sparse MSIX with external location)

**`packaging/msix-identity/AppxManifest.xml`.** Microsoft's template [V:S8] plus the extension [V:S6,S10]. The XML below is valid as written.

```xml
<?xml version="1.0" encoding="utf-8"?>
<Package IgnorableNamespaces="uap uap3 uap10 rescap"
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap3="http://schemas.microsoft.com/appx/manifest/uap/windows10/3"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
  <Identity Name="HueDeck.Lighting" Publisher="CN=… (EXACT subject DN of the signing cert)" Version="0.6.0.0" ProcessorArchitecture="neutral"/>
  <Properties>
    <DisplayName>HueDeck</DisplayName><PublisherDisplayName>HueDeck contributors</PublisherDisplayName>
    <Logo>assets\StoreLogo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Resources><Resource Language="en-us"/></Resources>
  <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0"/></Dependencies>
  <Capabilities><rescap:Capability Name="runFullTrust"/><rescap:Capability Name="unvirtualizedResources"/></Capabilities>
  <Applications>
    <Application Id="huedeckd" Executable="huedeckd.exe" uap10:TrustLevel="mediumIL" uap10:RuntimeBehavior="win32App">
      <uap:VisualElements AppListEntry="none" DisplayName="HueDeck" Description="HueDeck lighting engine" BackgroundColor="transparent"
        Square150x150Logo="assets\Square150x150Logo.png" Square44x44Logo="assets\Square44x44Logo.png"/>
      <Extensions>
        <uap3:Extension Category="windows.appExtension">
          <uap3:AppExtension Name="com.microsoft.windows.lighting" Id="HueDeck" DisplayName="HueDeck" PublicFolder="public"/>
        </uap3:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
```

**Manifest rules** [V:S8]:
- **Publisher:** "`Publisher` must match the `Subject` of the certificate used to sign the application".
- **Version:** "You will not be able to register a version of an identity package on a system if that version of the package is already registered." The error is 0x80073CF9.
- **`ProcessorArchitecture`:** `neutral`.
- **`AllowExternalContent`:** "introduced in Windows build 10.0.19041.0".
- **`MinVersion`:** `10.0.19041.0`, or `10.0.26100.0` for 24H2+-only builds.
- **`MaxVersionTested`:** `10.0.26100.0`.
- **Keep** `runFullTrust` + `unvirtualizedResources`, `mediumIL`, `win32App` and `AppListEntry="none"`.

**`uap3:AppExtension` attribute constraints** [V:S10]:

| Attribute | Required | Constraint |
|---|---|---|
| `Name` | Yes | 2–255 characters |
| `Id` | Yes | 2–39 characters; alphanumeric, periods (not first) and dashes |
| `DisplayName` | Yes | 1–256 characters |
| `Description` | No | — |
| `PublicFolder` | **Yes** (schema attribute table) | — |

- On `PublicFolder`: the AvailabilityChanged snippet omits it, but AutoRGB [V:S15] and SignalRGB [V-local] both use `PublicFolder="public"`. Keep it.
- Namespace: `http://schemas.microsoft.com/appx/manifest/uap/windows10/3`, minimum Windows 10 1607 (14393).
- Whether the folder must exist under the external location is [U].

**`Executable` attribute.**
- SignalRGB omits `Application@Executable` and works [V-local].
- Microsoft's template includes it ("Add an `Application` element as shown for each executable associated with your application") [V:S8]. Follow the template.
- If the UI executable also needs identity, add a second `Application`. The extension must sit on the process that calls the lighting APIs [design/U].

**Build and sign** [V:S8]:
```
MakeAppx.exe pack /o /d packaging\msix-identity /nv /p out\huedeck-identity.msix      ("The /nv flag is required")
SignTool.exe sign /fd SHA256 /a /f cert.pfx /p <pwd> out\huedeck-identity.msix
```
- **Dev self-signed certificate.**
  - Import the public `.cer` into `Cert:\CurrentUser\TrustedPeople`, or `Cert:\LocalMachine\TrustedPeople` for machine-wide installs (elevated).
  - Without it, registration fails with `CERT_E_UNTRUSTEDROOT` (0x800B0109) [V:S8].
- **Microsoft's signing service** is now named "Azure Artifact Signing (formerly Trusted Signing)" [V:S51].
  - About $9.99/month.
  - Organizations: USA, Canada, EU, UK. Individuals: **USA and Canada only**.
  - SmartScreen reputation builds over time.
- **SignPath Foundation:**
  - SignPath signs MSIX/AppX with `<msix-file>` / `<appx-file>` under `<authenticode-sign>`. The `Identity@Publisher` must "match the subject of the code signing certificate"; sha1 is not supported [V:S49].
  - The Foundation certificate "is issued to SignPath Foundation", so `Publisher` = SignPath Foundation's DN. The exact string is [U].
  - "Every release needs manual approval for signing."
  - Projects need an "OSI-approved Open Source license without commercial dual-licensing" and "may not contain any proprietary, non open-source component".
  - The terms page is marked draft [V:S49].
- **Unsigned option** (Windows 11+, dev/CI only) [V:S9]:
  - The publisher must contain `OID.2.25.311729368913984317654407730594956997722=1`.
  - Install with `Add-AppxPackage -AllowUnsigned` or `AddPackageOptions::SetAllowUnsigned(true)`.
  - "Don't use this feature to distribute your app widely."
  - Admin is needed when the unsigned package "contains executable content". An external-location package "contains only a package manifest (not an executable or other application content)" [V:S11]. Whether admin is needed here is [U].
  - Whether the ambient server accepts an unsigned identity is [U].

**Fusion manifest in `huedeckd.exe`** (re-verified on a rebuilt probe) [V-local]:
- The embedded manifest contains the `<msix …>` element, the compatibility GUID `{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}` and `activeCodePage UTF-8`.
- link.exe added `<trustInfo>` itself, with `requestedExecutionLevel level="asInvoker" uiAccess="false"`.
- Run with no package registered, `GetCurrentPackageFullName` returns 15700.
- Do not add your own `trustInfo`. The reason (a possible conflict with the linker-generated block, which would need `/MANIFESTUAC:NO`) is [U].
- The `msix` attributes `publisher`, `packageName` and `applicationId` must match `Identity@Publisher`, `Identity@Name` and `Application@Id` [V:S8].

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly manifestVersion="1.0" xmlns="urn:schemas-microsoft-com:asm.v1" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <assemblyIdentity version="0.6.0.0" name="HueDeck.huedeckd" type="win32"/>
  <msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="CN=…same DN…" packageName="HueDeck.Lighting" applicationId="huedeckd"/>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application></compatibility>
  <asmv3:application><asmv3:windowsSettings xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings"><activeCodePage>UTF-8</activeCodePage></asmv3:windowsSettings></asmv3:application>
</assembly>
```

```rust
// crates/huedeck-daemon/build.rs — same directives as rust-lang/rust compiler/rustc/build.rs
// (cargo:rustc-link-arg-bin=rustc-main=/MANIFEST:EMBED and /MANIFESTINPUT:..) [V:S17]; the -bins variant was built here [V-local]
fn main() {
    let m = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("huedeckd.exe.manifest"); // generate from env HUEDECK_MSIX_PUBLISHER
    println!("cargo:rerun-if-changed={}", m.display());
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=huedeckd=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=huedeckd=/MANIFESTINPUT:{}", m.display());
    }
}
```

**Registration** [V:S8]:
- **Per-user:** `Add-AppxPackage -Path <msix> -ExternalLocation <installdir>`. Uninstall with `Get-AppxPackage HueDeck.Lighting | Remove-AppxPackage`.
- **Per-machine (elevated):** `Add-AppxPackage -Stage <msix> -ExternalLocation <dir>; Add-AppxProvisionedPackage -Online -PackagePath <msix>`. The API equivalents are `StagePackageByUriAsync` + `ProvisionPackageForAllUsersAsync`.
- **The `ExternalLocation` must be exactly the app's install directory.** Troubleshooting row: "Identity absent at runtime (no error)".
  - Whether `Executable="bin\huedeckd.exe"` under an install-root `ExternalLocation` works is [U].
- **Other troubleshooting codes:**
  - 0x80073D54: the `msix` element and the identity manifest do not match.
  - 0x80073CF6: invalid manifest.
- **Logs:** "Applications and Services Logs > Microsoft > Windows > AppxDeployment-Server".
- **Sideloading policy:** "On Windows 10 version 2004 and later, signed non-Store MSIX packages can generally be installed … without separately enabling sideloading". Enterprise policy can restrict this [V:S19].

```rust
// verified against windows 0.62.2 (compiled in the probe; never executed) [V:S1, V-local build]
use windows::{core::HSTRING, Foundation::Uri, Management::Deployment::{AddPackageOptions, PackageManager}};
use windows::Win32::{Foundation::APPMODEL_ERROR_NO_PACKAGE, Storage::Packaging::Appx::GetCurrentPackageFullName};
pub fn register_identity(msix_uri: &str /* file:///C:/… */, install_dir_uri: &str) -> windows::core::Result<()> {
    let pm = PackageManager::new()?;
    let opts = AddPackageOptions::new()?;
    opts.SetExternalLocationUri(&Uri::CreateUri(&HSTRING::from(install_dir_uri))?)?;
    // other setters: SetAllowUnsigned, SetDeferRegistrationWhenPackagesAreInUse, SetForceAppShutdown,
    // SetForceTargetAppShutdown, SetForceUpdateFromAnyVersion, SetStageInPlace, SetDeveloperMode, SetRetainFilesOnFailure ...
    // AddPackageByUriAsync casts to IPackageManager9 and returns IAsyncOperationWithProgress<DeploymentResult, DeploymentProgress>
    let r = pm.AddPackageByUriAsync(&Uri::CreateUri(&HSTRING::from(msix_uri))?, &opts)?.join()?;
    let hr = r.ExtendedErrorCode()?;                       // DeploymentResult: ErrorText(), ActivityId(), IsRegistered()
    if hr.is_err() { return Err(windows::core::Error::new(hr, r.ErrorText()?.to_string())); }
    Ok(())
}
pub fn has_identity() -> bool { let mut n = 0u32; unsafe { GetCurrentPackageFullName(&mut n, None) } != APPMODEL_ERROR_NO_PACKAGE }
```

**Upgrades.** While huedeckd runs, the identity package is probably "in use". Stop the daemon before re-registering, or use the defer/force options [U].

**Dev loop.** Register a dev package whose `ExternalLocation` is `target\debug`, signed with the TrustedPeople dev certificate. Otherwise `cargo run` has no identity [V:S8 mechanism].

### 1.5 Backend sketch (HAL) [design; API calls verified against windows 0.62.2]

```rust
// huedeck-hal (OS-neutral)
pub struct LampDesc { pub index: i32, pub pos_m: [f32; 3], pub purposes: u32, pub levels: [i32; 4], pub fixed: Option<[u8; 3]>, pub latency: Duration }
pub struct LampArrayDesc { pub id: String /* HID iface path */, pub name: String, pub group: Option<GroupKey> /* §3 */,
    pub vid: u16, pub pid: u16, pub kind: i32, pub bbox_m: [f32; 3], pub min_update: Duration, pub lamps: Vec<LampDesc> }
pub enum LampEvent { Added { id: String, name: String }, Updated(String), Removed(String), EnumerationCompleted, Availability { id: String, available: bool } }
pub trait LampArrayBackend: Send + Sync {
    fn background_control_possible(&self) -> bool;                 // has_identity() && ILampArray2 present (IsAvailable() is Ok)
    fn watch(&self, tx: crossbeam_channel::Sender<LampEvent>) -> Result<Box<dyn Send>>; // guard: Stop() on drop
    fn open(&self, id: &str, tx: crossbeam_channel::Sender<LampEvent>) -> Result<(LampArrayDesc, Box<dyn LampArrayHandle>)>;
}
pub trait LampArrayHandle: Send {
    fn available(&self) -> bool;
    fn write(&mut self, rgb: &[[u8; 3]]) -> Result<()>;  // len == lamp count; honours min_update (coalesce: keep latest, send when due)
    fn set_brightness(&mut self, level: f64) -> Result<()>;
}

// huedeck-hal-windows/src/lamparray.rs
fn open(id: &str, tx: Sender<LampEvent>) -> windows::core::Result<WinLamp> {
    let la = LampArray::FromIdAsync(&HSTRING::from(id))?.join()?;   // or `.await`; Err(Error::empty()) if it resolves to null
    let n = la.LampCount()?;
    let token = la.AvailabilityChanged(&TypedEventHandler::<LampArray, IInspectable>::new(move |s, _| {
        if let Some(s) = s.as_ref() { let _ = tx.send(LampEvent::Availability { id: s.DeviceId()?.to_string(), available: s.IsAvailable()? }); }
        Ok(())                                     // never panic here (abort on unwind, Rust >= 1.81)
    })).ok();                                      // Err where ILampArray2 is absent; Err vs silent without identity+extension is [U]
    Ok(WinLamp { idx: (0..n).collect(), buf: vec![Color { A: 255, R: 0, G: 0, B: 0 }; n as usize],
                 min: Duration::from(la.MinUpdateInterval()?), last: Instant::now(), token, la })
}
impl LampArrayHandle for WinLamp {
    fn write(&mut self, rgb: &[[u8; 3]]) -> Result<()> {
        if self.last.elapsed() < self.min { return Ok(()); }   // design: better to coalesce and flush the latest frame when due
        for (c, s) in self.buf.iter_mut().zip(rgb) { *c = Color { A: 255, R: s[0], G: s[1], B: s[2] }; }
        self.la.SetColorsForIndices(&self.buf, &self.idx)?;     // arrays must be the same length [V:S45]
        self.last = Instant::now(); Ok(())
    }
    /* … */
}
// Drop: if let Some(t) = token { la.RemoveAvailabilityChanged(t) }
```

**Mapping into the HueDeck device model** [design]:
- **uid:** `lamparray:<GroupKey or path>`.
- **DeviceType from `LampArrayKind`:**

  | LampArrayKind | DeviceType |
  |---|---|
  | Keyboard | keyboard |
  | Mouse | mouse |
  | Headset | headphones |
  | Microphone | microphone |
  | Speaker | speakers |
  | Chassis | case |
  | everything else | other |

- **Geometry:**
  - `Size` grid = BoundingBox / pitch (for example 5 mm), clamped to 1..128.
  - `LedPositions` = quantized `Position`.
- **`LedNames`:** VirtualKey names when `SupportsVirtualKeys` (needs `System`); otherwise "Lamp n".
- **Fixed-color lamps:**
  - Skip lamps whose `FixedColor()` returns `Ok`.
  - `Err(empty)` means the lamp has no fixed color.
- **Driver path:** treat it as a native driver (no JS) on the same sampling and brightness path. Prefer HueDeck brightness over `SetBrightnessLevel`.

**Dedupe** [design]:
- **On `Added(path)`:**
  - If `group(path)` is claimed by a plugin (its VID:PID is in the plugin index), do not open it.
  - Show the hint "turn off Dynamic Lighting for this device in Settings".
- **HID plugin scan:** drop 0x59/0x01 TLCs (as SignalRGB does [V:S40]).

### 1.6 LampArray risks and M6 runtime checks

**Risks:**
- Background control is impossible without signed identity [V:S3].
- The user must reorder Background light control; WindowsLighting is first by default [V-local].
- With "Compatible apps in the foreground always control lighting" ON, foreground DL-aware apps pre-empt HueDeck [V:S3].
- A new USB port is a new device, so its DL settings are separate [V:S3].
- **Device path changes with connection mode.** The DL registry has keys for PID_007D (another product id / connection mode of the DeathAdder) but none for the connected PID_007C. Settings follow the device path [V-local; interpretation U].
- A plugin and Windows DL on the same device fight each other. HueDeck will not write the registry to disable WDL [V:S54; SignalRGB behaviour inferred from strings].
- `IsAvailable` / `AvailabilityChanged` can be absent or marked Experimental [V:S5].

**Runtime checks to run in M6** [U]:
1. `IsAvailable` for the unpackaged daemon.
2. Whether its writes are applied while HueDeck's own UI (a different process) has focus.
3. Behaviour when "Use Dynamic Lighting on my devices" is off (presumably the case here).
4. Whether HueDeck appears in Background light control right after registration, and under which name. SignalRGB's extension DisplayName is literally "Id", which suggests the package/app DisplayName is the one users see.
5. `SetColorsForIndices` cost and blocking.
6. Whether an unsigned identity is accepted.
7. Whether Windows opens the 0x59 collection exclusively. Keyboard/mouse exclusivity is documented; LampArray exclusivity is not.
8. Who writes the `Providers` lists.
9. Whether `AvailabilityChanged` subscription errors without the extension.

---

## 2. OpenRGB SDK network protocol (implement from facts; copy no GPL code or text)

### 2.1 Versions (header defines plus release tags) [V:S20,S21,S22,S25,S41,S43]

| v | First release (GitLab tag date) | Adds |
|---|---|---|
| 0 | **SDK server first shipped in 0.2** (2020-05-12; notes: "OpenRGB SDK server"; default port 1337). **0.3** (2020-07-21 commit, 2020-07-22 UTC): "Default SDK server port changed to 6742", GUI SDK client added. The official doc table says v0 = 0.3. | packets 0, 1, 50, 1000, 1050–1052, 1100, 1101 |
| 1 | 0.5 (2020-12-06 local / 2020-12-07 UTC) | 40 (versioning), vendor string, 100 DEVICE_LIST_UPDATED |
| 2 | 0.6 (2021-06-01) | 150–153 profile controls |
| 3 | 0.7 (2021-12-30); 0.8 (2022-11-28) is still v3 | mode brightness_min/max/brightness, 1102 SAVEMODE |
| 4 | 0.9 (2023-07-09) | zone segments, plugin packets 200/201 |
| 5 | 1.0rc1 (2025-02-22), rc2 (2025-09-14), **rc3 (2026-06-28), rc3.1 (2026-08-23)** | zone flags, controller flags, effects-only zones, alternative LED names, 1001 CLEARSEGMENTS / 1002 ADDSEGMENT. **140 RESCAN exists only from rc2** (rc1 already reports v5 without it). |
| 6 | 1.0 (2026-09-11), 1.1 (2026-10-16) | See the v6 list below this table. |

**v6 additions:**
- unique controller ids
- 10 ACK
- 51–53 server name / client and server flags; 54/55 hostnames are in the header but not in the doc
- 101–103 detection events
- 120–124 device-info lists
- 154–161 ProfileManager
- 250–254 SettingsManager
- 300–304 LogManager
- 1003–1005 ConfigureZone / ConfigureDevice / SetHidden
- 1103 UpdateZoneMode
- 1130/1131 device-specific configuration
- 1150 SIGNALUPDATE
- display names, per-zone modes, segment matrix and flags
- **removes `mode_value` and `led_value`**

**Source facts:**
- `OPENRGB_SDK_PROTOCOL_VERSION` per tag:
  - 1: 0.5
  - 2: 0.6
  - 3: 0.7 and 0.8
  - 4: 0.9
  - 5: rc1, rc2, rc3 and rc3.1
  - 6: 1.0, 1.1 and master
- The 1.0, 1.1 and master doc files are byte-identical (sha256 `1a1e035fcfab…`), as are their `NetworkProtocol.h` files (`36693d957c28…`).
- The Codeberg copies at tag `release_1.1` are identical too.
- Release notes and downloads "are now hosted on Codeberg" (`codeberg.org/OpenRGB/OpenRGB`).

**Doc discrepancies:**
- The doc still marks v6 "unreleased".
- The 1.1 header comment attributes v5 to "Release Candidate 1.0rc2".

**Default host and size history:**
- `OPENRGB_SDK_HOST` is absent up to 0.7.
- It is `"0.0.0.0"` from 0.8 through 1.0rc3.
- It is `"127.0.0.1"` from 1.0rc3.1 on.
- `OPENRGB_SDK_MAX_PACKET_SIZE (1024*1024*8)` first appears in 1.0rc3.1.

**1.1 release notes:**
- "Fix an issue with the OpenRGB SDK server not replying to the plugin list request in headless mode".
- "Fix an issue with the OpenRGB SDK server device indices on protocol versions < 6".

### 2.2 Framing and encoding

- **Header** = 16 bytes: `char[4] "ORGB"` | `u32 pkt_dev_id` (v≤5: controller *index*; v6: unique id) | `u32 pkt_id` | `u32 pkt_size` (payload bytes after the header) [V:S20,S22].
- **Little-endian throughout.**
  - The official Wireshark dissector reads every field with `le_uint`/`add_le`.
  - OpenRGB itself `memcpy`s native structs; `RGBColor` is an `unsigned int`.
  - OpenRGB.NET writes via `MemoryMarshal.Write` (native = LE on x86/ARM) [V:S24,S27,S42].
- **Strings:**
  - `u16` length **including the NUL**, then the bytes, then the NUL.
  - "Client Only" NUL-terminated string packets (50, 151–153, 155 request, settings JSON …) use `pkt_size = strlen+1` with no prefix [V:S20].
  - Exceptions:
    - The v6 `configuration_len` is a `u32` [V:S20].
    - 1131 carries `u32 configuration_string_size` [V:S20,S42].
- **RGBColor:** 4 bytes `R, G, B, 0x00`. In LE that is a `u32 0x00BBGGRR`; `ToRGBColor(r,g,b) = (b<<16)|(g<<8)|r` [V:S23,S43,S27].
- **`data_size` includes its own 4 bytes.** Verified in OpenRGB 1.1 source for:
  - the controller-data reply (`reply_size = sizeof(reply_size) + GetDeviceDescriptionSize(..)`, and `pkt_size == data_size`)
  - profile list and plugin list
  - AddSegment, ConfigureZone, ConfigureDevice
  - LED/mode update packets

  OpenRGB.NET writes `4+2+4n` for UPDATELEDS [V:S42,S27]. 1130/1131 have no `data_size` prefix [V:S42].
- **Defaults:** port 6742 ("ORGB" on a phone keypad); host `127.0.0.1` and maximum packet 8 MiB in 1.0rc3.1+ [V:S20,S22,S43].
- **Limits:**
  - Counts are `u16`, so at most 65535 LEDs/zones/modes/colors per block.
  - The matrix block is `u16 len = 8 + 4·w·h`, so at most 16381 cells, followed by `u32 height, u32 width, u32[h·w]`.
  - The map is row-major `Map[Y][X]`; values are zone-relative LED indexes; 0xFFFFFFFF means unused [V:S21,S23].

### 2.3 Packet IDs

Direction: C→S = client to server, S→C = server to client. The last column is the minimum protocol version. [V:S20,S21,S22; behaviour notes V:S42]

| id | name | dir | payload | reply | v |
|---|---|---|---|---|---|
| 0 | REQUEST_CONTROLLER_COUNT | C→S | — | `u32 count` (v6: plus `u32 ids[count]`). Reply header `pkt_dev_id` = 0. | 0 |
| 1 | REQUEST_CONTROLLER_DATA | C→S | v0: —; v1+: `u32` = highest version supported by both | `u32 data_size` + Device Data at that version. **Reply header echoes the requested `pkt_dev_id`** [V:S42]. | 0 |
| 10 | ACK | S→C | `u32 acked_pkt_id, u32 status` (0 OK, 1 generic, 2 unsupported, 3 not allowed, 4 invalid id, 5 invalid data); header echoes `pkt_dev_id`. OpenRGB 1.1 sends one after processing **every** client packet (also after replies; unknown ids → status 2), only to clients that negotiated ≥6. | — | 6 |
| 40 | REQUEST_PROTOCOL_VERSION | C→S | `u32` client max | `u32` server max (a v0 server never replies). OpenRGB 1.1 also sends 51 to v6 clients right after it. | 1 |
| 50 | SET_CLIENT_NAME | C→S | NUL-terminated string | none (ACK in v6) | 0 |
| 51 / 53 | SET_SERVER_NAME / SET_SERVER_FLAGS | S→C | string / `u32` flags: bits 0 RGBController, 1 Log, 2 Profile, 3 Plugin, 4 Settings, 5 detection, 6 device info, 16 local client granted (only to localhost clients that asked) | — | 6 |
| 52 | SET_CLIENT_FLAGS | C→S | `u32` (bits 0–4 as above; bit 16 = request local client) | answered by 53 | 6 |
| 54 / 55 | SET_CLIENT_HOSTNAME / SET_SERVER_HOSTNAME | C→S / S→C | string; 54 is answered by 55 | | 6 (header only) |
| 100 | DEVICE_LIST_UPDATED | S→C | — (header `pkt_dev_id` 0). OpenRGB 1.1 pushes it to **all** clients regardless of version. | client re-syncs | 1 |
| 101 / 102 / 103 | DETECTION_STARTED / PROGRESS_CHANGED / COMPLETE | S→C | 102: `data_size, u32 %, u16+str`; sent only to v6 clients | — | 6 |
| 120–124 | GET_I2C_BUS_INFO / HID_DEVICE_INFO / USB_DEVICE_INFO / SERIAL_PORTS / USB_SERIAL_PORTS | C→S | — | lists (use only when server flag 6 is set) | 6 |
| 140 | REQUEST_RESCAN_DEVICES | C→S | — | none | 5 (rc2+) |
| 150 | PROFILEMANAGER_GET_PROFILE_LIST (v≤5 name: REQUEST_PROFILE_LIST) | C→S | — | `data_size, u16 n, n×str` | 2 |
| 151 / 152 / 153 | SAVE / LOAD / DELETE_PROFILE (v≤5: REQUEST_SAVE/LOAD/DELETE_PROFILE) | C→S | string | none. After LOAD the client re-requests all controllers, then calls UpdateMode. In v6, save/delete (and 154 upload, 252–254) need local-client status. | 2 |
| 154–161 | UPLOAD / DOWNLOAD / GET_ACTIVE / ACTIVE_CHANGED / PROFILE_LOADED / ABOUT_TO_LOAD / LIST_UPDATED / CLEAR_ACTIVE | mixed | JSON or strings | The doc says clients "should acknowledge" 159 "with an ACK"; the 1.1 client actually echoes a zero-length **159** packet, and the server counts those. | 6 |
| 200 | PLUGINMANAGER_GET_PLUGIN_LIST (v≤5: REQUEST_PLUGIN_LIST) | C→S | — | `data_size, u16 n, {str name, str desc, str version, u32 index, u32 plugin_protocol_version}` | 4 |
| 201 | PLUGIN_SPECIFIC | C→S | `pkt_dev_id` = plugin index; `u32` plugin packet type + opaque data | optional | 4 |
| 250–254 | SETTINGSMANAGER_* | C→S | JSON strings | | 6 |
| 300–304 | LOGMANAGER_* | mixed | | | 6 |
| 1000 | RESIZEZONE | C→S | `i32 zone, i32 new_size` | none | 0 |
| 1001 | CLEARSEGMENTS | C→S | `i32 zone` | none | 5 |
| 1002 | ADDSEGMENT | C→S | `data_size, u32 zone, Segment` | none | 5 |
| 1003 | CONFIGUREZONE | C→S | `data_size, u32 zone, Zone` | | 6 |
| 1004 | CONFIGUREDEVICE | C→S | `data_size, u32 flags, u16+str name` | | 6 |
| 1005 | SETHIDDEN | C→S | `bool` (1 byte) | | 6 |
| 1050 | UPDATELEDS | C→S | `data_size, u16 n, n×RGBColor` | none | 0 |
| 1051 | UPDATEZONELEDS | C→S | `data_size, u32 zone, u16 n, n×RGBColor` | none | 0 |
| 1052 | UPDATESINGLELED | C→S | `i32 led, RGBColor` | none | 0 |
| 1100 | SETCUSTOMMODE | C→S | — | none | 0 |
| 1101 / 1102 | UPDATEMODE / SAVEMODE | C→S | `data_size, i32 mode_idx, Mode` | none | 0 / 3 |
| 1103 | UPDATEZONEMODE | C→S | `data_size, i32 zone, i32 mode, Mode` | | 6 |
| 1130 | SETDEVICESPECIFICCONFIGURATION | C→S | NUL-terminated JSON, `pkt_size = strlen+1` | | 6 |
| 1131 | SETDEVICESPECIFICZONECONFIGURATION | C→S | `i32 zone, u32 len(incl NUL), JSON` | | 6 |
| 1150 | SIGNALUPDATE | S→C | `data_size, u32 reason` + (reason 0 UPDATELEDS: `u16 n + colors`; any other reason: Device Data) | | 6 |

The packet layouts for 1000–1102 in the rc2 (v5) and 1.1 (v6) docs differ only in the wording `pkt_dev_idx` → `pkt_dev_id` [V: diff in this pass].

### 2.4 Device Data (reply to packet 1), field order at negotiated version v [V:S20,S21]

Prefix: `u32 data_size` (including itself).

1. `i32 type`
2. `str name`
3. **[v≥1]** `str vendor`
4. `str description`, `str version`, `str serial`, `str location`
5. `u16 num_modes`, `i32 active_mode`, Mode × num_modes
6. `u16 num_zones`, Zone × num_zones
7. `u16 num_leds`, LED × num_leds
8. `u16 num_colors`, RGBColor × num_colors
9. **[v≥5]** `u16 n` + `str` LED display (alternate) names × n, then `u32 controller_flags`
10. **[v≥6]** `str display_name`, then `u32 configuration_len` + bytes (NUL-terminated JSON)

Sub-blocks:
- **Mode:**
  - `str name`
  - [v≤5] `i32 mode_value`
  - `u32 flags, speed_min, speed_max`
  - [v≥3] `u32 brightness_min, brightness_max`
  - `u32 colors_min, colors_max, speed`
  - [v≥3] `u32 brightness`
  - `u32 direction, color_mode`
  - `u16 n` + colors

  For v≤5, "the client must track the mode_value received from the server and send it back as part of the UpdateMode/SaveMode packets".
- **Zone:**
  - `str name`
  - `i32 type`
  - `u32 leds_min, leds_max, leds_count`
  - matrix block (`u16 len`: 0, or 8+4wh)
  - [v≥4] `u16 nseg` + Segment × nseg
  - [v≥5] `u32 zone_flags`
  - [v≥6] `i32 active_mode` (−1 = use the device-level mode [V:S23]), `u16 n` + Mode × n, `str display_name`
- **Segment:**
  - `str name`
  - `i32 type`
  - `u32 start_idx`
  - `u32 leds_count`
  - [v≥6] matrix block, `u32 segment_flags`
- **LED:**
  - `str name`
  - [v≤5] `u32 led_value`

### 2.5 Enums (RGBControllerAPI.md, release_1.1) [V:S23]

**Device type:**

| Value | Type | Value | Type |
|---|---|---|---|
| 0 | Motherboard | 11 | Light |
| 1 | DRAM | 12 | Speaker |
| 2 | GPU | 13 | Virtual |
| 3 | Cooler | 14 | Storage |
| 4 | LED Strip | 15 | Case |
| 5 | Keyboard | 16 | Microphone |
| 6 | Mouse | 17 | Accessory |
| 7 | Mousemat | 18 | Keypad |
| 8 | Headset | 19 | Laptop |
| 9 | Headset Stand | 20 | Monitor |
| 10 | Gamepad | 21 | Unknown |

- The doc says: "Anything out of range should be considered Unknown."
- **Unknown's number shifts between versions:**

  | OpenRGB version | Unknown value |
  |---|---|
  | 0.5 | 12 |
  | 0.7 | 15 |
  | 0.9 | 19 |
  | 1.0rc2 / 1.1 | 21 |

  New types are added before Unknown ("DEVICE_TYPE_UNKNOWN needs to remain last") [V:S43]. Never send Unknown to old clients. Values above an old client's range read as Unknown there.

**Other enums:**
- **Controller flags (bits):**
  - 0 Local, 1 Remote, 2 Virtual, 3 Hidden
  - 8 Reset Before Update
  - 16/17 manually configurable name / device-specific
  - 24/25 manually configured name / device-specific
- **Zone type:**
  - 0 Single, 1 Linear, 2 Matrix, 3 Linear Loop
  - 4 Matrix Loop X, 5 Matrix Loop Y, 6 Segmented
- **Zone flags (bits):**
  - 0 configurable size effects-only, 1 configurable size, 2 name, 3 type, 4 matrix map, 5 segments, 6 device-specific
  - 12–17: the matching "manually configured" bits
  - 24: geometry may change
- **Segment flags:** bit 0 group start, bit 1 group member.
- **Mode flags (bits):**
  - 0 speed, 1 L/R direction, 2 U/D direction, 3 H/V direction, 4 brightness
  - 5 **per-LED color**, 6 mode-specific colors, 7 random
  - 8 manual save, 9 auto-save, 10 entire device only
- **Direction:** 0 L, 1 R, 2 Up, 3 Down, 4 Horizontal, 5 Vertical.
- **Color mode:** 0 None, 1 **Per-LED**, 2 Mode-specific, 3 Random.
- **SignalUpdate reasons:**

  | Value | Reason |
  |---|---|
  | 0 | UPDATELEDS |
  | 1 | UPDATEMODE |
  | 2 | SAVEMODE |
  | 3 | CONFIGUREZONE |
  | 4 | CLEARSEGMENTS |
  | 5 | ADDSEGMENT |
  | 6 | HIDDEN |
  | 7 | UNHIDDEN |
  | 8 | SETDEVICESPECIFICCONFIGURATION |
  | 9 | SETDEVICESPECIFICZONECONFIGURATION |
  | 10 | CONFIGUREDEVICE |

**Semantics:**
- `SetCustomMode()` "searches the mode list for "Direct", then "Custom", then "Static" modes with per-LED or mode-specific color support".
- `ResizeZone` "Only works if the zone has `ZONE_FLAG_MANUALLY_CONFIGURABLE_SIZE` or `ZONE_FLAG_MANUALLY_CONFIGURABLE_SIZE_EFFECTS_ONLY` set".

### 2.6 Session rules

**Server, v≤5** [design over verified facts]:
- **Negotiation:**
  - `proto = 0` until packet 40 arrives.
  - Reply to 40 with `SERVER_MAX = 5`; negotiated = min(client, 5). OpenRGB clamps the same way [V:S42].
- **0 → count.**
- **1 → encode at min(requested, 5).** The reply header's `pkt_dev_id` = the requested index [V:S42].
- **Index snapshot:**
  - Keep a per-connection index → uid snapshot.
  - When the registry changes, push 100. OpenRGB 1.1 sends it to every client regardless of version, so sending only to v≥1 is a stricter choice.
  - Ignore that client's RGB writes until it sends 0 again. Per the doc, the client "must immediately stop using the existing indexes".
- **Writes and modes:**
  - 1050–1052 → the SDK override buffer.
  - 1100 → switch the device to "Direct".
  - 1101/1102 for the Direct index → same as 1100; any other mode → ignore.
  - 1000 → resize a channel zone only when its configurable-size flag is set, then push 100.
- **Lists:** 150 / 200 → reply with empty lists (`data_size=6`, `n=0`; `data_size` counts itself [V:S42]).
- **Ignored packets:** 140 / 151–153 / 201 / 52 / 54 / unknown ids → consume the payload and ignore. v≤5 has no error reply; OpenRGB only ACKs v6 clients [V:S42].
- **Fatal input:** bad magic or a size above the cap → drop the connection. OpenRGB 1.1 does exactly this [V:S42].

**Server, v6 (later):**
- unique `u32` ids
- an ACK after every client packet, with status, including after replies (OpenRGB 1.1 behaviour) [V:S42]
- client/server flags; reply 53 to 52 and 55 to 54
- local-client privilege only for loopback peers [V:S20]
- send 51 after the 40 reply

**Bind and port ownership on Windows** [V:S48]:
- **The MS bind table** (Windows Server 2003+, same user account): a first socket bound *Default/Wildcard* (0.0.0.0) and a second bind *Default/Specific* (127.0.0.1) → "Success".
  - So `bind(127.0.0.1:6742)` does **not** fail while an older OpenRGB (0.8 → 1.0rc3 default `0.0.0.0`) listens under the same user.
  - Local clients would then reach HueDeck instead of OpenRGB.
- **Under different accounts** the second bind fails with WSAEACCES rather than WSAEADDRINUSE.
- **Mitigations** [design]:
  - Probe-connect `127.0.0.1:6742` first. If something answers 40 with an `ORGB` header, treat the port as owned and use a configurable alternate port.
  - Then bind with `SO_EXCLUSIVEADDRUSE` set ("All server applications must set SO_EXCLUSIVEADDRUSE"). Whether std/tokio set it by default is [U]; set it via `setsockopt` before `bind`.

**Client bridge** (HueDeck → OpenRGB) [design; protocol facts V:S20,S42]:
- **Connect.**
  - Send 40 (v5) and wait about 1 s; no reply means v0. OpenRGB's own client waits 200 × 5 ms; OpenRGB.NET defaults to `timeoutMs = 1000` [V:S27].
  - Send 50 "HueDeck", then 0, then 1 per controller, one at a time.
- **Drive** with 1100, then 1050 per frame.
- **Reader task.** It demultiplexes asynchronous 100 packets (plus 10/51/1150 if v6 is ever negotiated) from replies.
- **OpenRGB versions.**
  - Require OpenRGB ≥ 1.1 for v≤5, because 1.0 had the "device indices on protocol versions < 6" server bug [V:S41].
  - OpenRGB 1.0rc3.1+ binds 127.0.0.1 by default, so bridging to another PC needs the user to change OpenRGB's server host [V:S43].
- **Loop safety.**
  - Do not bridge controllers that HueDeck already drives. The format of OpenRGB `location` strings is [U].
  - Never re-export bridged controllers through HueDeck's own server, which would create a loop.

### 2.7 Rust sketch (no external protocol crate) [design]

`bytes` 1.12.1 is already in the lockfile. `tokio` is declared in `[workspace.dependencies]` but no member uses it yet (`huedeck-api`: "TODO(M4): axum, tower-http, serde_json, tokio") [V-local].

```rust
// crates/huedeck-api/src/openrgb/wire.rs
pub const MAGIC: [u8; 4] = *b"ORGB"; pub const HDR: usize = 16; pub const MAX_PAYLOAD: u32 = 8 << 20; pub const SERVER_MAX: u32 = 5;
pub mod pid { pub const CTRL_COUNT: u32 = 0; pub const CTRL_DATA: u32 = 1; pub const ACK: u32 = 10; pub const PROTO: u32 = 40;
  pub const CLIENT_NAME: u32 = 50; pub const SERVER_NAME: u32 = 51; pub const CLIENT_FLAGS: u32 = 52; pub const SERVER_FLAGS: u32 = 53;
  pub const CLIENT_HOSTNAME: u32 = 54; pub const SERVER_HOSTNAME: u32 = 55; pub const LIST_UPDATED: u32 = 100; pub const RESCAN: u32 = 140;
  pub const PROFILE_LIST: u32 = 150; pub const PLUGIN_LIST: u32 = 200; pub const RESIZE_ZONE: u32 = 1000; pub const UPDATE_LEDS: u32 = 1050;
  pub const UPDATE_ZONE_LEDS: u32 = 1051; pub const UPDATE_SINGLE_LED: u32 = 1052; pub const SET_CUSTOM_MODE: u32 = 1100;
  pub const UPDATE_MODE: u32 = 1101; pub const SAVE_MODE: u32 = 1102; }
#[derive(Clone, Copy, Debug)] pub struct Header { pub dev: u32, pub id: u32, pub len: u32 }
impl Header {
    pub fn decode(b: &[u8; HDR]) -> Result<Self, WireError> {
        if b[..4] != MAGIC { return Err(WireError::Magic); }
        let u = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let h = Header { dev: u(4), id: u(8), len: u(12) };
        if h.len > MAX_PAYLOAD { return Err(WireError::TooLarge(h.len)); } Ok(h)
    }
    pub fn encode(&self, o: &mut BytesMut) { o.put_slice(&MAGIC); o.put_u32_le(self.dev); o.put_u32_le(self.id); o.put_u32_le(self.len); }
}
fn put_str(o: &mut BytesMut, s: &str) { o.put_u16_le((s.len() + 1) as u16); o.put_slice(s.as_bytes()); o.put_u8(0); }
pub struct Rgb(pub [u8; 4]);                                   // R,G,B,0
pub struct Mode { pub name: String, pub value: i32, pub flags: u32, pub speed: [u32; 3], pub brightness: [u32; 3],
                  pub colors_min: u32, pub colors_max: u32, pub direction: u32, pub color_mode: u32, pub colors: Vec<Rgb> }
pub struct Segment { pub name: String, pub ty: i32, pub start: u32, pub count: u32 }
pub struct Zone { pub name: String, pub ty: i32, pub min: u32, pub max: u32, pub count: u32,
                  pub matrix: Option<(u32 /*h*/, u32 /*w*/, Vec<u32>)>, pub segments: Vec<Segment>, pub flags: u32 }
pub struct Controller { pub ty: i32, pub name: String, pub vendor: String, pub description: String, pub version: String,
                        pub serial: String, pub location: String, pub modes: Vec<Mode>, pub active_mode: i32, pub zones: Vec<Zone>,
                        pub leds: Vec<String>, pub colors: Vec<Rgb>, pub alt_names: Vec<String>, pub flags: u32 }
impl Controller {
    /// Reply payload for packet 1: u32 data_size (incl. itself; equals pkt_size) + Device Data at version `v` (§2.4).
    pub fn encode(&self, v: u32) -> BytesMut { let mut b = BytesMut::new(); b.put_u32_le(0); /* … fields gated by v … */
        let n = b.len() as u32; b[0..4].copy_from_slice(&n.to_le_bytes()); b }
}
pub trait SdkRegistry: Send + Sync {                           // implemented by huedeck-engine
    fn snapshot(&self) -> Vec<(Uid, Controller)>;
    fn apply(&self, uid: &Uid, zone: Option<u32>, first: u32, colors: &[[u8; 3]]); // -> LightingMode::External
    fn set_direct(&self, uid: &Uid);  fn resize(&self, uid: &Uid, zone: u32, n: u32) -> bool;
    fn changed(&self) -> tokio::sync::broadcast::Receiver<()>;
}
// server: probe-connect 127.0.0.1:6742 first; then bind (SO_EXCLUSIVEADDRUSE) — on AddrInUse/WSAEACCES report "port owned by another app"
// (ADR-0005 policy: refuse, no SO_REUSEADDR); per connection: tokio::select!{ read_exact(16)+read_exact(len) -> dispatch(§2.6) ; changed.recv() -> push 100 }
```

**Test vectors** (checked byte-by-byte against the verified layouts):
- 40 with v5:
  ```
  4F 52 47 42 00 00 00 00 28 00 00 00 04 00 00 00 05 00 00 00
  ```
- 0 reply, count 3:
  ```
  4F 52 47 42 00 00 00 00 00 00 00 00 04 00 00 00 03 00 00 00
  ```
- 1050 to device 0, colors red then green:
  ```
  4F 52 47 42 00 00 00 00 1A 04 00 00 0E 00 00 00 | 0E 00 00 00 02 00 FF 00 00 00 00 FF 00 00
  ```

### 2.8 HueDeck → RGBController mapping [design]

- **Controllers:** one per HueDeck endpoint (device or subdevice).
- **Zones:**
  - The device body is one zone. Use Matrix (2) with a map built from `Size` + `LedPositions` (row-major, zone-relative indexes, 0xFFFFFFFF = empty) when the LEDs fit a grid; otherwise Linear (1).
  - Each channel is a Linear zone with `min = 0`, `max = LedLimit`, `count` = the current value.
  - Set zone flag bit 1 (configurable size) when v≥5. 1.1's `ResizeZone` requires it.
- **Modes:** `[{"Direct", flags: 1<<5, color_mode: 1}]`, with `active_mode = 0`. This matches OpenRGB's own `SetCustomMode` search order "Direct" → "Custom" → "Static" [V:S23]. What third-party clients look for is [U].
- **Type map** (HueDeck DeviceType → OpenRGB type; values valid for 0.9+ clients; older clients read out-of-range values as Unknown):

  | HueDeck | OpenRGB |
  |---|---|
  | motherboard | 0 |
  | ram | 1 |
  | gpu | 2 |
  | aio | 3 |
  | lightingcontroller | 4 |
  | keyboard | 5 |
  | mouse | 6 |
  | mousepad | 7 |
  | headphones | 8 |
  | speakers | 12 |
  | case | 15 |
  | microphone | 16 |
  | other / dongle / lcd | 17 |

- **Strings:**
  - vendor = plugin `Publisher`
  - version = plugin `Version`
  - serial = the uid's serial part
  - location = `HueDeck:<uid>`
- **Engine:** `LightingMode::External{colors, deadline}` per endpoint. Fall back to the canvas after about 5 s without updates or when the client disconnects.

**Security and operations:**
- **Exposure.**
  - Loopback by default. OpenRGB itself defaults to 127.0.0.1 only since 1.0rc3.1 [V:S43].
  - LAN is opt-in with an IP allowlist; the protocol has no authentication [V:S20].
- **Limits.**
  - Rate-limit 1050–1052 (for example 240/s per connection).
  - Cap sessions.
  - Cap the payload (8 MiB or lower).
- **Port conflicts.** If 6742 is owned, use a configurable alternate port (see the bind pitfall in §2.6).
- **Interop tests** (external tools only):
  - OpenRGB.NET sample (MIT; `MaxProtocolNumber = 4`; last push 2024-08-02) [V:S27].
  - OpenRGB 1.1 as SDK client: its protocol path against a v5 server is verified in its source [V:S42]. Since 0.3 the GUI "includes an SDK client" [V:S41]. The UI flow against a third-party server is [U].
  - Wireshark with OpenRGB's dissector, used as a tool and not vendored (GPL).
  - Watch for clients that look for an `OpenRGB.exe` process. A third-party server README says Project Aurora "may require renaming the binary to OpenRGB.exe" [U:S52].

### 2.9 Crates and licensing [V:S26,S27,S28,S53]

| Crate / project | License | Version / date | Role | Usable in-tree? |
|---|---|---|---|---|
| `openrgb` (nicoulaj) | GPL-2.0 | 0.1.2, 2022-05-14 | client | No (cargo-deny) |
| `openrgb2` (Achtuur) | GPL-2.0 | 0.4.1, 2026-09-05 | client. README: "will support the latest protocol version (currently 6) and will support previous protocol versions on a best effort basis"; v6 "Server updates" are not supported | No (cargo-deny) |
| other crates.io hits for "openrgb" | various | — | The search returns 8 crates. `cute_lights` (GPL-3.0-or-later) and `dmxvis` (MIT) depend on the GPL `openrgb` crate; `openrgb-daemon` and `codex-agent-indicator` are not protocol implementations. | — |
| GitHub (not crates.io) | — | — | A few MIT apps mention OpenRGB but were not audited. `ColorHoster` is a GPL-3.0 Rust v3 SDK **server**. | Facts only |
| OpenRGB.NET (C#) | MIT | master, 2024-08-02 | client, v0–v4 | Safe to *read* as a cross-check (attribution if anything is reused) |
| OpenRGB repo (`NetworkProtocol.h` `SPDX-License-Identifier: GPL-2.0-or-later`; docs; dissector) | GPL-2.0-or-later | — | — | Numbers and layouts are facts. Do not paste the header, source or doc text. |

---

## 3. Hotplug and device identity (cfgmgr32)

### 3.1 Exact API (windows 0.62.2) and feature flags [V:S1]

**Features:**
- `Win32_Devices_DeviceAndDriverInstallation` (already in the workspace).
- `Win32_Devices_HumanInterfaceDevice` (already in the workspace):
  - `GUID_DEVINTERFACE_HID = 4d1e55b2-f16f-11cf-88cb-001111000030`.
  - **The `DEVPKEY_DeviceInterface_HID_*` keys also live here** (§3.6).
- `Win32_Devices_Usb`: `GUID_DEVINTERFACE_USB_DEVICE = a5dcbf10-6530-11d2-901f-00c04fb951ed` (**ADD**).
- `Win32_Devices_Properties`:
  - Keys: `DEVPKEY_Device_InstanceId {78c34fc8-104a-4aca-9ea4-524d52996e57},256`, `DEVPKEY_Device_Parent {4340a6c5-…},8`, `DEVPKEY_Device_ContainerId {8c7ed206-…},2`, `DEVPKEY_Device_LocationPaths {a45c254e-…},37`, `DEVPKEY_Device_HardwareIds`, `DEVPKEY_Device_CompatibleIds`.
  - Types: `DEVPROPTYPE`, `DEVPROP_TYPE_STRING = 18`, `DEVPROP_TYPE_UINT16 = 5`, `DEVPROP_TYPE_STRING_LIST = 8210`.
  - **`CM_Get_Device_Interface_PropertyW` and `CM_Get_DevNode_PropertyW` are themselves `#[cfg(feature = "Win32_Devices_Properties")]`.**
- `Win32_Foundation`: provides `DEVPROPKEY`, `HANDLE`, `WIN32_ERROR`.

```rust
pub unsafe fn CM_Register_Notification(pfilter: *const CM_NOTIFY_FILTER, pcontext: Option<*const c_void>,
    pcallback: PCM_NOTIFY_CALLBACK, pnotifycontext: *mut HCMNOTIFICATION) -> CONFIGRET;
pub unsafe fn CM_Unregister_Notification(notifycontext: HCMNOTIFICATION) -> CONFIGRET;
pub type PCM_NOTIFY_CALLBACK = Option<unsafe extern "system" fn(hnotify: HCMNOTIFICATION, context: *const c_void,
    action: CM_NOTIFY_ACTION, eventdata: *const CM_NOTIFY_EVENT_DATA, eventdatasize: u32) -> u32>;
// CM_NOTIFY_FILTER { cbSize: u32, Flags: u32, FilterType: CM_NOTIFY_FILTER_TYPE, Reserved: u32, u: CM_NOTIFY_FILTER_0 }  size_of = 416 [V-local re-run]
// CM_NOTIFY_FILTER_0 = union { DeviceInterface: {ClassGuid: GUID}, DeviceHandle: {hTarget: HANDLE}, DeviceInstance: {InstanceId: [u16; 200]} }
// CM_NOTIFY_EVENT_DATA { FilterType, Reserved: u32, u: union } size_of = 36 [V-local re-run]; u.DeviceInterface = { ClassGuid: GUID, SymbolicLink: [u16; 1] }
//   -> flexible NUL-terminated WCHAR string; bound it with eventdatasize
// FILTER_TYPE: DEVICEINTERFACE=0, DEVICEHANDLE=1, DEVICEINSTANCE=2; FLAG_ALL_INTERFACE_CLASSES=1 (ClassGuid must be zero), FLAG_ALL_DEVICE_INSTANCES=2
// ACTION: INTERFACEARRIVAL=0, INTERFACEREMOVAL=1, QUERYREMOVE=2, QUERYREMOVEFAILED=3, REMOVEPENDING=4, REMOVECOMPLETE=5,
//         CUSTOMEVENT=6, INSTANCEENUMERATED=7, INSTANCESTARTED=8, INSTANCEREMOVED=9 (MAX=10)
// HCMNOTIFICATION(*mut c_void): impl windows_core::Free (calls CM_Unregister_Notification) -> windows::core::Owned<HCMNOTIFICATION> (Owned::new is unsafe)
pub unsafe fn CM_Get_Device_Interface_PropertyW<P0: Param<PCWSTR>>(iface: P0, key: *const DEVPROPKEY, ty: *mut DEVPROPTYPE,
    buf: Option<*mut u8>, size: *mut u32, flags: u32 /*0*/) -> CONFIGRET;   // buf=None requires *size=0 -> CR_BUFFER_SMALL(26) + needed size
pub unsafe fn CM_Locate_DevNodeW<P1: Param<PCWSTR>>(pdn: *mut u32, id: P1, flags: CM_LOCATE_DEVNODE_FLAGS) -> CONFIGRET;
    // NORMAL=0 (present only), PHANTOM=1 (also non-present -> use on removal), CANCELREMOVE=2 (NEVER: cancels a removal), NOVALIDATION=4 ("Not used")
pub unsafe fn CM_Get_Parent(pdn: *mut u32, dn: u32, flags: u32 /*"Not used, must be zero"*/) -> CONFIGRET;
pub unsafe fn CM_Get_Device_ID_Size(len: *mut u32, dn: u32, flags: u32) -> CONFIGRET;
pub unsafe fn CM_Get_Device_IDW(dn: u32, buffer: &mut [u16] /* size+1 */, flags: u32 /*0*/) -> CONFIGRET;
pub unsafe fn CM_Get_DevNode_PropertyW(dn: u32, key: *const DEVPROPKEY, ty: *mut DEVPROPTYPE, buf: Option<*mut u8>, size: *mut u32, flags: u32) -> CONFIGRET;
pub unsafe fn CM_Get_Device_Interface_List_SizeW<P2>(len: *mut u32, class: *const GUID, devid: P2, flags: CM_GET_DEVICE_INTERFACE_LIST_FLAGS) -> CONFIGRET;
pub unsafe fn CM_Get_Device_Interface_ListW<P1>(class: *const GUID, devid: P1, buf: &mut [u16], flags) -> CONFIGRET; // PRESENT=0, ALL_DEVICES=1; multi-sz
pub unsafe fn CM_MapCrToWin32Err(cr: CONFIGRET, default: u32) -> u32;
// MAX_DEVICE_ID_LEN=200; CR_SUCCESS=0, CR_NO_SUCH_DEVNODE=13, CR_FAILURE=19, CR_BUFFER_SMALL=26, CR_NO_SUCH_VALUE=37, CR_NO_SUCH_DEVICE_INTERFACE=55
```

**Documented semantics** [V:S29–S33]:
- **Availability:** "Available in Microsoft Windows 8 and later versions of Windows" (CfgMgr32.dll).
- **No initial-state events.** It "does not provide notification of existing device interfaces". Register first, then call `CM_Get_Device_Interface_List`. An interface enabled in between is reported twice, so dedupe.
- **Callback discipline.**
  - "Be sure to handle Plug and Play device events as quickly as possible"; offload blocking I/O.
  - Return `ERROR_SUCCESS`; `ERROR_CANCELLED` is allowed only to veto `DEVICEQUERYREMOVE`, and Microsoft recommends not vetoing.
- **Unregistration.**
  - "Do not call CM_Unregister_Notification from a notification callback … because CM_Unregister_Notification waits for pending callbacks to finish". Defer it.
  - "Do not free the context structure until after the work item has called CM_Unregister_Notification".
- **Per-handle (`DEVICEHANDLE`) registrations:**
  - On `QUERYREMOVE`, close the handle; otherwise "your open handle prevents the query remove of this device from succeeding".
  - On `REMOVEPENDING`/`REMOVECOMPLETE`, close it and unregister from a deferred routine. These can arrive without a preceding `QUERYREMOVE`.
  - After `CM_Register_Notification` returns, the handle can be closed without affecting the registration.
- **Callback thread: not documented.** Design for an arbitrary, possibly concurrent system thread [U]. Microsoft's unregister sample guards its context with a `CRITICAL_SECTION`. nusb uses `Mutex<VecDeque>` + `Waker` [V:S36].
- **List race.** "Between calling CM_Get_Device_Interface_List_Size … and calling CM_Get_Device_Interface_List … a new device interface can be added … Callers should … retry … if CM_Get_Device_Interface_List returns CR_BUFFER_SMALL" [V:S33].

### 3.2 Registration sketch (the probe's equivalent compiled and ran in this pass: both registrations CR_SUCCESS; unregister → CONFIGRET(0)) [V-local]

```rust
struct Ctx { tx: crossbeam_channel::Sender<Raw> }               // unbounded Sender: Send+Sync, never blocks
enum Raw { Arrive(String), Remove(String) }
unsafe extern "system" fn on_pnp(_: HCMNOTIFICATION, ctx: *const c_void, a: CM_NOTIFY_ACTION,
                                 d: *const CM_NOTIFY_EVENT_DATA, size: u32) -> u32 {
    // MUST NOT PANIC (abort on unwind through extern "system", Rust >= 1.81)
    let ctx = unsafe { &*(ctx as *const Ctx) };
    if !d.is_null() && unsafe { (*d).FilterType } == CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE {
        let p = unsafe { (*d).u.DeviceInterface.SymbolicLink.as_ptr() };
        let max = (size as usize).saturating_sub(p as usize - d as usize) / 2;
        let link = from_wide(unsafe { std::slice::from_raw_parts(p, max) });
        let _ = ctx.tx.send(match a { CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL => Raw::Arrive(link), _ => Raw::Remove(link) });
    }
    ERROR_SUCCESS.0
}
pub struct CmWatch { regs: Vec<windows::core::Owned<HCMNOTIFICATION>>, _ctx: Box<Ctx> } // field order = drop order: unregister first, then free ctx
pub fn watch(tx: crossbeam_channel::Sender<Raw>) -> Result<CmWatch, CONFIGRET> {
    let ctx = Box::new(Ctx { tx }); let mut regs = Vec::new();
    for class in [GUID_DEVINTERFACE_HID, GUID_DEVINTERFACE_USB_DEVICE] {
        let f = CM_NOTIFY_FILTER { cbSize: size_of::<CM_NOTIFY_FILTER>() as u32, Flags: 0, FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
                                   Reserved: 0, u: CM_NOTIFY_FILTER_0 { DeviceInterface: CM_NOTIFY_FILTER_0_0 { ClassGuid: class } } };
        let mut h = HCMNOTIFICATION(std::ptr::null_mut());
        let cr = unsafe { CM_Register_Notification(&f, Some(&*ctx as *const Ctx as *const c_void), Some(on_pnp), &mut h) };
        if cr != CR_SUCCESS { return Err(cr); }                 // locals drop regs before ctx: safe
        regs.push(unsafe { windows::core::Owned::new(h) });
    }
    Ok(CmWatch { regs, _ctx: ctx })   // THEN enumerate present interfaces (CM_Get_Device_Interface_ListW, PRESENT, retry on CR_BUFFER_SMALL) and dedupe
}
// Per opened HID/USB handle: a second registration with FilterType DEVICEHANDLE (hTarget = handle); on QUERYREMOVE close it [V:S31,S32].
```

**nusb for raw USB:** nusb 0.2.8 (2026-10-14, Apache-2.0 OR MIT) already provides `nusb::watch_devices()` [V:S36].
- On Windows it registers `CM_Register_Notification` on `GUID_DEVINTERFACE_USB_DEVICE`, using windows-sys.
- It resolves the instance id inside the callback and locates devnodes with `CM_LOCATE_DEVNODE_PHANTOM`.
- Its Drop runs `CM_Unregister_Notification` before freeing the context.
- Its docs warn: "On Windows, the interfaces of a composite device might not be ready when the `Connected` event is emitted … you should retry after a short delay if opening or claiming fails".
- You can reuse it for USB-level events and keep the HID registration in HueDeck. `nusb = "0.2"` is declared in the workspace but not yet locked.

### 3.3 HID interface path → USB device instance id (walk verified on this machine) [V-local]

```
interface_instance_id(path) = CM_Get_Device_Interface_PropertyW(path, DEVPKEY_Device_InstanceId)   // DEVPROP_TYPE_STRING
dn = CM_Locate_DevNodeW(id, NORMAL  /* PHANTOM on removal */)
loop ≤ 8: dn = CM_Get_Parent(dn,0); id = CM_Get_Device_ID_Size + CM_Get_Device_IDW
    if id starts "USB\VID_" && !contains("&MI_")                    -> group key (composite parent or single-function device)
    if id starts USB\ROOT_HUB | PCI\ | ACPI\ | ROOT\ | HTREE\ | BTH*  -> no USB parent (BT, I2C/SPI HID, …)
```
(`USB\VID_v(4)&PID_d(4)&MI_z(2)` is the documented per-interface device id; z = `bInterfaceNumber` [V:S46].)

**Observed chains:**
- **Razer LampArray (3 levels):**
  1. `HID\VID_1532&PID_007C&MI_03\8&5CC7230&0&0000`
  2. → `USB\VID_1532&PID_007C&MI_03\7&14B7ADF8&0&0003` (service HidUsb)
  3. → `USB\VID_1532&PID_007C\000000000000` (service usbccgp)
- **Vendor virtual bus (4 levels):**
  1. `HID\VID_1532&PID_007C&MI_00&COL03\9&2783460C&0&0000`
  2. → `RZVIRTUAL\VID_1532&PID_007C&MI_00&COL03\8&3B4F7E3E&0&01` (service RzDev_007c, CompatibleIds `RAZER\VCONDevice`)
  3. → `USB\VID_1532&PID_007C&MI_01\7&14B7ADF8&0&0001`
  4. → `USB\VID_1532&PID_007C\000000000000`

  Notes:
  - The `MI_` in the HID path (00) differs from the real USB interface (01).
  - A naive two-level walk misses this.
  - With the any-enumerator walk, all 9 Razer HID devnodes group under one key.
- **Grouping result: 20 present HID interfaces form 7 groups** (re-run of the probe plus a PowerShell walk):

  | Group key (USB device instance id) | HID interfaces |
  |---|---|
  | MSI `USB\VID_1462&PID_7C91\A00000000001` | 1 |
  | Sonix `USB\VID_320F&PID_5064\7&0000000&0&3` | 6 |
  | NZXT `USB\VID_1E71&PID_2007\00000000001A` | 1 |
  | SteelSeries `USB\VID_1038&PID_12AA\00000000` | 1 |
  | Razer `USB\VID_1532&PID_007C\000000000000` | 9 |
  | **Sony DualSense** `USB\VID_054C&PID_0CE6\6&00000000&0&10` (location-based id) | 1 |
  | **C-Media** `USB\VID_0D8C&PID_0005\000_0000/00/00_00000` (instance id contains `/`) | 1 |

  `GUID_DEVINTERFACE_USB_DEVICE` interfaces present: 9.

**Caveats and alternatives:**
- **hidapi-rs 2.6.7 caveat.** HueDeck uses hidapi's `windows-native` feature in `huedeck-hal-windows` [V-local]. Its `get_internal_info` [V:S35]:
  - takes the *immediate* parent devnode;
  - derives `bus_type` from that parent's CompatibleIds (`USB*` → Usb, `BTHENUM`, `BTHLEDEVICE`, `PNP0C50` I2C, `PNP0C51` SPI);
  - only for Usb calls `get_usb_info`, which reads `MI_` from that parent's HardwareIds.

  For the RZVIRTUAL-backed collection the parent's CompatibleIds is `RAZER\VCONDevice` [V-local], so it reports bus Unknown and `interface_number = -1` (code reading; not executed) [V:S35 / U runtime]. Take the endpoint interface from `&MI_xx` in the HID path, as SignalRGB does [V:S40], and group with the walk above.
- **Faster alternative.** `CM_Get_DevNode_PropertyW(dn, &DEVPKEY_Device_Parent)` returns the parent's instance id directly. The `CM_Get_Parent` docs point to this key [V:S33].
- **Casing differs across APIs.**
  - `DEVPKEY_Device_Parent` returned `…MI_03\7&14b7adf8&0&0003` and `RZVIRTUAL\…&Col03\8&3b4f7e3e&0&01`.
  - Interface paths use `&Col03`.
  - `CM_Get_Device_IDW` and `Get-PnpDevice` return uppercase (`7&14B7ADF8…`, `&COL03`) [V-local].
  - Compare case-insensitively, and normalize to uppercase before forming uids. SignalRGB's example uid is uppercase [V:S40].

### 3.4 Debounce and reconcile [design; architecture.md: "CM_Register_Notification (debounced 500 ms)" V:S54]

- **Reconciler thread** (std thread, `crossbeam::recv_timeout`):
  - On each `Raw` event: key = `cache.get(path)` or `resolve(path)` (NORMAL walk, then PHANTOM, then VID/PID parsed from the path as a provisional key).
  - Set `pending[key] = now + 500 ms`, resetting on every event.
- **When a key's deadline passes:**
  - List present HID interfaces (`CM_Get_Device_Interface_ListW(GUID_DEVINTERFACE_HID, PRESENT)`, retrying on `CR_BUFFER_SMALL`).
  - Resolve unknown paths into the path→key cache and collect this key's paths.
  - Diff against the registry, then emit `GroupArrived{key, endpoints}`, `GroupChanged` or `GroupRemoved{key}`. tokio's `UnboundedSender::send` is synchronous.
- **Composite devices** report N staggered interface arrivals (9 for the Razer); the quiet window collapses them.
- **Open** with a retry of 3 × 250 ms, because composite interfaces may not be ready yet [V:S36].
- **Unplug:**
  - Stop plugin I/O on the first removal for an endpoint in use, but tear down only after the quiet window.
  - A replug within the window counts as a reconnect, followed by `Initialize()`.
  - Register `DEVICEHANDLE` for every open handle and close it on `QUERYREMOVE` [V:S32].
- **Suspend/resume:** §3.8.

### 3.5 Identity rules

| Rule | Status |
|---|---|
| Group key = the USB device instance id. Compat uid = `vid:pid:<instance suffix after the last \, with & → ->`, uppercase. Examples: `1462:7c91:A00000000001`, `320f:5064:7-0000000-0-3`, `054c:0ce6:6-00000000-0-10`. | [V:S40 grammar; ids V-local] |
| **Hazard:** instance ids can contain `/`, e.g. C-Media `0d8c:0005:000_0000/00/00_00000`. Escape uids before using them in file paths, URLs or registry-like keys. | [V-local; design] |
| An instance id "contains serial number information, if supported by the underlying bus, or some kind of location information", and "is persistent across system restarts". | [V:S34] |
| Serial-less devices (the Sonix and Sony here) get location-based ids that change when the device moves port. Some vendors report constant serials (Razer `000000000000`), so two identical units may collide [U]. Disambiguate with `DEVPKEY_Device_LocationPaths`, a `STRING_LIST` holding a PCI path and an ACPI path. Observed examples: Razer `PCIROOT(0)#PCI(0801)#PCI(0003)#USBROOT(0)#USB(4)` + `ACPI(_SB_)#…#ACPI(PRT4)`; Sonix behind a hub `PCIROOT(0)#PCI(0801)#PCI(0003)#USBROOT(0)#USB(1)#USB(3)`. | [V-local] |
| **ContainerId:**<br>• The HID, USB-interface and USB-device levels share one ContainerId here (Razer `{1D6226C9-A1C1-5D75-9397-308FDDB39917}`).<br>• Internal devices got distinct containers here (MSI `{74078AD9-…}`). The root hub shows `{00000000-0000-0000-FFFF-FFFFFFFFFFFF}`.<br>• Documented heuristic: a devnode whose removable capability is FALSE "inherit[s] the container ID from its parent devnode", so internal devices can collapse into the PC's container on other machines. Use ContainerId only as a secondary or BT key. | [V-local; V:S46] |
| Bluetooth: group by the `BTHENUM\` / `BTHLEDEVICE\` ancestor, falling back to ContainerId. No BT HID device here to verify. | [U] |

### 3.6 Classify HID interfaces without opening them [V-local; constants V:S1]

`CM_Get_Device_Interface_PropertyW(path, &DEVPKEY_DeviceInterface_HID_UsagePage | _UsageId)` returns `DEVPROP_TYPE_UINT16`.

The keys are in `windows::Win32::Devices::HumanInterfaceDevice`:
- fmtid `cbf38310-4a17-4310-a1eb-247f0b67593b`
- pids: UsagePage 2, UsageId 3, IsReadOnly 4, VendorId 5, ProductId 6, VersionNumber 7, BackgroundAccess 8, WakeScreenOnInputCapable 9

Values read here:

| Device / interface | UsagePage / Usage |
|---|---|
| Razer `MI_03` | 0x59 / 0x01 (LampArray) |
| NZXT | 0xFF00 / 0x01 |
| Sonix `Col04` | 0xFF1C / 0x92 |
| MSI 7C91 | 0x01 / 0x00 |
| SteelSeries `MI_05` | 0x0C / 0x01 |

- **Keyboard TLCs** (0x01/0x06) have a `\KBD` suffix on the interface path.
- Microsoft: "The system opens all keyboard and mouse collections for its exclusive use" [V:S46].
- Use these properties to drop LampArray and system keyboard/mouse TLCs, and to pre-fill endpoint usage/usage_page, before calling `CreateFile`.

### 3.7 Hotplug pitfalls

- Unregistering inside the callback deadlocks; the context must outlive its registrations [V:S30].
- No initial-state events are delivered, and duplicates are possible [V:S29,S32].
- Removal paths may no longer resolve with NORMAL. Use the cache, `CM_LOCATE_DEVNODE_PHANTOM`, or VID/PID parsing [V:S33,S36].
- Never use `CM_LOCATE_DEVNODE_CANCELREMOVE`; it "cancels the removal of the device" [V:S33].
- Vendor virtual buses (RZVIRTUAL) [V-local].
- `GUID_DEVINTERFACE_USB_DEVICE` is one interface per USB device (9 present here) [V-local].
- **Open handles block query-remove** unless you hold `DEVICEHANDLE` registrations and close the handles [V:S31,S32].
- **A panic in any `extern "system"` callback aborts the daemon** [V:S50].

### 3.8 Suspend/resume (gap filled; was "not researched") [V:S47,S1]

- **API:** `PowerRegisterSuspendResumeNotification(Flags, Recipient, *RegistrationHandle)` in Powrprof.dll.
  - "Windows 8 [desktop apps only]".
  - Flags "must be DEVICE_NOTIFY_CALLBACK".
  - Recipient points to `DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS`.
  - The callback receives `PBT_APMSUSPEND` (4), `PBT_APMRESUMESUSPEND` (7) or `PBT_APMRESUMEAUTOMATIC` (18).
  - No window is needed.
- **In windows 0.62.2:**
  - It lives in `Win32::System::Power` and is gated on `Win32_UI_WindowsAndMessaging`. `DEVICE_NOTIFY_CALLBACK = REGISTER_NOTIFICATION_FLAGS(2)` and `PBT_*` live there too.
  - Signature: `(flags: REGISTER_NOTIFICATION_FLAGS, recipient: HANDLE, registrationhandle: *mut *mut c_void) -> WIN32_ERROR`.
  - The callback type is `PDEVICE_NOTIFY_CALLBACK_ROUTINE = Option<unsafe extern "system" fn(context: *const c_void, type: u32, setting: *const c_void) -> u32>`.
  - `PowerUnregisterSuspendResumeNotification` takes `HPOWERNOTIFY(isize)`, so convert the raw pointer.
  - Keep the subscribe-parameters struct alive until you unregister [U].
- **Feature flags:** `Win32_System_Power` + `Win32_UI_WindowsAndMessaging`.
- **Re-enumeration storms** on resume are absorbed by the 500 ms debounce [design].

---

## 4. Workspace and Cargo changes

```toml
# Cargo.toml [workspace.dependencies] — keep the current Win32 set
# (Win32_Foundation, Win32_Devices_DeviceAndDriverInstallation, Win32_Devices_HumanInterfaceDevice, Win32_Devices_Properties,
#  Win32_System_Threading, Win32_Storage_FileSystem, Win32_Security  [V-local]) and add:
windows = { version = "0.62", features = [ "Win32_Foundation", "Win32_Devices_DeviceAndDriverInstallation", "Win32_Devices_HumanInterfaceDevice",
  "Win32_Devices_Properties", "Win32_System_Threading", "Win32_Storage_FileSystem", "Win32_Security",
  "Win32_Devices_Usb", "Win32_Storage_Packaging_Appx",
  "Win32_System_Power", "Win32_UI_WindowsAndMessaging" ] }        # last two only if §3.8 is adopted
windows-numerics = "0.3"   # only if HueDeck code names Vector3 (windows does not re-export it)
windows-future = "0.3"     # only if HueDeck code names IAsyncOperation & co.
# crates/huedeck-hal-windows/Cargo.toml (feature "lamparray", default on):
windows = { workspace = true, features = ["Devices_Enumeration", "Devices_Lights", "UI"] }  # Foundation implied; + "Devices_Lights_Effects", "System" if used
# crate that registers identity (huedeck-setup or a huedeck-daemon "lighting enable" subcommand):
windows = { workspace = true, features = ["Management_Deployment"] }  # implies Management + Foundation
```

- Cargo features are additive across the workspace.
- The probe rebuilt offline in this pass in 12.6 s, compiling `windows` with WinRT + Win32 features [V-local].
- **OpenRGB needs no new external crate.**
  - `bytes` 1.12.1 is already locked.
  - `tokio` (and `crossbeam-channel`, `nusb`) are declared in `[workspace.dependencies]` but are not yet in `Cargo.lock`, because no member uses them yet [V-local].

---

## 5. Risk register (ranked)

1. **Ambient LampArray needs a signed identity package.**
   - Requirements: a certificate-subject/Publisher match; SignPath manual approvals, a Publisher that will be SignPath Foundation, and its "no proprietary components" rule; or Azure Artifact Signing's eligibility limits for individuals outside the USA/Canada. Dev builds have no identity.
   - *Mitigation:* describe-only mode when there is no identity; a dev registration script with the TrustedPeople dev certificate. [V:S3,S8,S49,S51]
2. **The user must give HueDeck priority** over WindowsLighting/SignalRGB in Settings, and foreground apps can pre-empt it.
   - Per-device settings are tied to device path and port.
   - *Mitigation:* an `IsAvailable`-driven UI plus the deep link. [V:S3,S13; V-local]
3. **Double control on the same device** (the Razer: plugin + DL; or a SignalRGB package still registered).
   - *Mitigation:* group-key dedupe and user guidance; no registry writes. Also resolve the plan's `generic-lamparray` vs 0x59-exclusion conflict. [V-local; V:S54]
4. **OpenRGB protocol drift.**
   - v6 shipped 2026-09-11 with new semantics (unique ids, ACK after every packet, flags, removed `mode_value`/`led_value`).
   - OpenRGB 1.0's server mis-indexed devices for v<6 clients until 1.1.
   - *Mitigation:* advertise v5; bridge only to OpenRGB ≥ 1.1; add v6 after a conformance test against 1.1. [V:S20,S22,S25,S41,S42]
5. **GPL contamination.** OpenRGB code and doc text, the dissector, and the `openrgb`/`openrgb2`/`cute_lights` crates are GPL.
   - *Mitigation:* clean-room codec from facts; cross-check only against MIT OpenRGB.NET. [V:S22,S26,S27,S53]
6. **OpenRGB server exposure and port shadowing.**
   - No authentication.
   - A same-user wildcard listener (older OpenRGB) does not make a 127.0.0.1 bind fail.
   - *Mitigation:* loopback by default, probe-connect before bind, `SO_EXCLUSIVEADDRUSE`, refuse or use an alternate port, rate and size caps. [V:S20,S43,S48]
7. **Hotplug correctness:** virtual buses, removal resolution, composite readiness, callback threading, handle-blocked removals, and abort-on-panic callbacks.
   - *Mitigation:* walk through any enumerator; path cache plus PHANTOM; 500 ms quiet window; open retries; DEVICEHANDLE registrations; panic-free callbacks. [V-local; V:S32,S36,S50; threading U]
8. **uid stability.**
   - Serial-less devices, duplicate serials, inconsistent casing, `/` in instance ids.
   - *Mitigation:* uppercase-normalize, escape, and use LocationPaths as a tiebreak. [V-local; collision U]

---

## 6. Sources

**Primary and local:**
- **S1** — local cargo registry sources, mirroring https://docs.rs/windows/0.62.2.
  - Crates: `windows` 0.62.2, `windows-core` 0.62.2, `windows-future` 0.3.2, `windows-numerics` 0.3.1, `windows-result` 0.4.1.
  - Files read:
    - `Devices/Lights/mod.rs`, `Devices/Lights/Effects/mod.rs`, `Devices/Enumeration/mod.rs`
    - `Management/Deployment/mod.rs`
    - `Win32/Devices/{DeviceAndDriverInstallation,Properties,HumanInterfaceDevice,Usb}/mod.rs`
    - `Win32/System/Power/mod.rs`, `Win32/Storage/Packaging/Appx/mod.rs`
    - `extensions/Foundation/TimeSpan.rs`, `Cargo.toml` features
    - `windows-core/src/imp/factory_cache.rs`, `windows-core/src/type.rs`, `windows-core/src/ref.rs`
    - `windows-future/src/{join,when,future}.rs`
- **S2** — the probe crate (read-only at runtime; LampArray open/write and package registration are compiled but never executed): `<research-cache>`. Rebuilt and re-run in this pass with `CARGO_TARGET_DIR=…/scratchpad/research/winint-probe-verify-target`.
- **V-local** read-only queries, 2026-10-18:
  - WinRT `LampArray::GetDeviceSelector` and `FindAllAsync` from PowerShell.
  - `Get-PnpDevice` / `Get-PnpDeviceProperty`: Parent, Service, CompatibleIds, ContainerId, LocationPaths.
  - `Get-AppxPackage SignalRGB` and `C:/Program Files/WindowsApps/SignalRGB_1.0.2.6_x64__4n6rtbnm9e9y2/AppxManifest.xml`.
  - A read of `HKCU\Software\Microsoft\Lighting`.
  - SignalRGB string dump `…/scratchpad/signalrgb_exe_strings.txt`, lines 109731, 110571–110576, 111240–111242, 118460–118500, 121030–121085, 184929.
  - The embedded manifest extracted from the rebuilt probe exe.
  - Repo `Cargo.lock` and `Cargo.toml` files.

**Microsoft documentation and samples** (raw markdown fetched from the MicrosoftDocs GitHub repos where available; learn.microsoft.com dates checked):
- **S3** https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/lighting-dynamic-lamparray (`updated_at` 2026-05-28; source `ms.date` 2024-12-05)
- **S3b** https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/dynamic-lighting-devices
- **S4** https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray
- **S5** https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray.isavailable
- **S6** https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray.availabilitychanged
- **S7** …/windows.devices.lights.lampinfo.position, …/lamparray.boundingbox, …/lamparray.minupdateinterval, …/windows.foundation.timespan
- **S8** https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps (Last updated 2026-10-03; GitHub source `ms.date` 2026-04-08; content checked equal)
- **S9** https://learn.microsoft.com/en-us/windows/msix/package/unsigned-package
- **S10** https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap3-appextension
- **S11** …/windows.management.deployment.addpackageoptions.externallocationuri and …allowunsigned
- **S12** https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-devices-lights-winrt-defaultlightingprovider
- **S13** https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings-app
- **S14** https://github.com/microsoft/ArduinoHidForWindows/blob/main/src/LampArrayReportDescriptor.h (MIT)
- **S15** https://github.com/microsoft/Dynamic-Lighting-AutoRGB (MIT; `AutoRGBPrototype (Package)/Package.appxmanifest`)
- **S16** https://learn.microsoft.com/en-us/uwp/api/windows.devices.enumeration.devicewatcher.updated
- **S17** https://github.com/rust-lang/rust/blob/master/compiler/rustc/build.rs
- **S18** https://blogs.windows.com/windowsexperience/?p=178499 (WebFetch: "Dynamic Lighting is now available on Windows 11", September 27, 2023) and https://www.neowin.net/news/tags/moment_4_update/ (secondary)
- **S19** https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/msix-windows10-windows11
- **S44** https://support.microsoft.com/en-us/topic/windows-11-version-22h2-update-history-ec4229c3-9c5f-4e75-9d6d-9025ab70fcce and https://support.microsoft.com/en-us/help/5030310
- **S45** https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray.setcolorsforindices
- **S46** windows-driver-docs:
  - hid/keyboard-and-mouse-hid-client-drivers.md
  - install/standard-usb-identifiers.md
  - install/how-container-ids-are-generated-from-the-removable-device-capability.md
  - hid/virtual-hid-framework--vhf-.md
- **S47** sdk-api: powerbase/nf-powerbase-powerregistersuspendresumenotification.md
- **S48** https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
- **S51** https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options (Azure Artifact Signing; SignPath Foundation paragraph)

**OpenRGB and related** (GPL; facts only):
- **S20** https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/OpenRGBSDK.md (identical to release_1.0, master and the Codeberg tag)
- **S21** …/release_candidate_1.0rc2/Documentation/OpenRGBSDK.md (identical to rc3 and rc3.1; rc1 lacks 140)
- **S22** https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkProtocol.h
- **S23** https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/RGBControllerAPI.md
- **S24** …/release_1.1/Documentation/OpenRGB%20SDK%20Wireshark%20Dissector.lua
- **S25** https://gitlab.com/api/v4/projects/CalcProgrammer1%2FOpenRGB/releases
- **S41** https://codeberg.org/api/v1/repos/OpenRGB/OpenRGB/releases/tags/{release_0.2,release_0.3,release_1.0,release_1.1}; release notes now live on Codeberg
- **S42** OpenRGB 1.1 source: `NetworkServer.cpp`, `NetworkClient.cpp`, `RGBController/RGBController.cpp`, `RGBController/RGBController_Network.cpp`, `ProfileManager.cpp` at https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/release_1.1/…
- **S43** `NetworkProtocol.h` at release_0.2 … release_1.1 (incl. rc1/rc2/rc3/rc3.1), `RGBController/RGBController.h` at 0.5/0.7/0.9/rc2, and `NetworkServer.cpp` at 0.2
- **S26** https://crates.io/api/v1/crates/openrgb, …/openrgb2, …/crates?q=openrgb
- **S27** https://github.com/diogotr7/OpenRGB.NET (MIT; `ProtocolVersion.cs`, `OpenRGBClient.cs`, `UpdateLedsArg.cs`, `Color.cs`)
- **S28** https://github.com/Achtuur/openrgb-rs2 (README)
- **S52** https://github.com/Azarattum/ColorHoster (README; GPL-3.0 third-party v3 SDK server; claims only)
- **S53** crates.io API for cute_lights, dmxvis, codex-agent-indicator and openrgb-daemon (license and dependencies)

**cfgmgr32 documentation:**
- **S29** https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification
- **S30** …/nf-cfgmgr32-cm_unregister_notification
- **S31** …/ns-cfgmgr32-cm_notify_filter, …/ns-cfgmgr32-cm_notify_event_data, …/ne-cfgmgr32-cm_notify_action
- **S32** https://learn.microsoft.com/en-us/windows-hardware/drivers/install/registering-for-notification-of-device-interface-arrival-and-device-removal
- **S33** …/nf-cfgmgr32-cm_get_parent, …cm_locate_devnodew, …cm_get_device_idw, …cm_get_device_interface_propertyw, …cm_get_device_interface_listw
- **S34** https://learn.microsoft.com/en-us/windows-hardware/drivers/install/instance-ids, …/device-instance-ids and …/usbcon/enumeration-of-the-composite-parent-device

**Rust crates, tooling and signing:**
- **S35** hidapi 2.6.7 `src/windows_native/device_info.rs` (MIT), local registry copy; https://github.com/ruabmbua/hidapi-rs
- **S36** https://github.com/kevinmehall/nusb at tag v0.2.8 (`src/lib.rs`, `src/platform/windows_winusb/{hotplug.rs,cfgmgr32.rs}`; Apache-2.0 OR MIT)
- **S37** https://www.powershellgallery.com/packages/SignPath/ (superseded for MSIX facts by S49)
- **S49** https://docs.signpath.io/artifact-configuration/reference and https://signpath.org/terms (draft)
- **S50** https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/ ("Abort on uncaught panics in extern "C" functions")

**Repo (read-only):**
- **S40** `docs/host-api/macros-system.md` §3.3 and §3.5
- **S54** read-only HueDeck repo context:
  - `docs/architecture.md` line 144 (CM_Register_Notification "debounced 500 ms") and line 160 ("No registry writes")
  - `docs/adr/0005-signalrgb-compat-profile.md` (refuse to bind; no SO_REUSEADDR)
  - `docs/adr/0007-no-custom-kernel-driver.md`
  - approved plan `%USERPROFILE%/.claude/plans/use-rea-to-find-serialized-salamander.md` (M6 scope; the `generic-lamparray` plugin)

Verifier downloads: `<research-cache>` (subfolders `ms/`, `openrgb/`, `crates/`, `rust/`, `misc/`).

---

## Fact-check corrections applied to the first draft

- uap3:AppExtension PublicFolder: the brief said optional; Microsoft's schema reference attribute table marks PublicFolder 'Required: Yes' (only the AvailabilityChanged snippet omits it). Name is 2-255 chars (the brief listed only Id's 2-39). The manifest XML was already correct.
- Dynamic Lighting GA: KB5030310 (OS Build 22621.2361) Preview is dated 2023-09-26 in Microsoft's update history, but its notes do not mention Dynamic Lighting (controlled feature rollout). The Windows Experience Blog post 'Dynamic Lighting is now available on Windows 11' is dated 2023-09-27 as fetched, not 09-26. The minimum build 22621.2361 comes only from secondary sources (ASUS FAQ, Neowin).
- OpenRGB protocol v0: the SDK server first shipped in OpenRGB 0.2 (2020-05-12, release notes 'OpenRGB SDK server', default port 1337). 0.3 changed the default port to 6742 and added the GUI SDK client. The official doc table's 'v0 = 0.3' is kept only as the doc's view.
- OpenRGB version table was incomplete: 0.8 (2022-11-28) is still v3; 1.0rc3 (2026-06-28) and 1.0rc3.1 (2026-08-23) are still v5; RESCAN 140 exists only from 1.0rc2 even though rc1 already declares v5.
- OpenRGB default bind host: OPENRGB_SDK_HOST was '0.0.0.0' from 0.8 through 1.0rc3 and became '127.0.0.1' (together with OPENRGB_SDK_MAX_PACKET_SIZE = 8 MiB) only in 1.0rc3.1. The brief implied loopback was OpenRGB's long-standing default.
- New bind pitfall (MS Winsock SO_REUSEADDR/SO_EXCLUSIVEADDRUSE table): under the same user, binding 127.0.0.1:6742 SUCCEEDS while another process holds 0.0.0.0:6742, so 'refuse on AddrInUse' alone cannot detect older OpenRGB. Under different accounts the error is WSAEACCES, not WSAEADDRINUSE. Added probe-connect-before-bind and SO_EXCLUSIVEADDRUSE.
- OpenRGB 1.1 release notes (Codeberg) say: 'Fix an issue with the OpenRGB SDK server device indices on protocol versions < 6' and fix the plugin-list reply in headless mode. The client bridge must require OpenRGB >= 1.1 when speaking v<=5. Release notes and downloads now live on Codeberg; the GitLab files are identical.
- Now VERIFIED from OpenRGB 1.1 NetworkServer.cpp (previously U): the REQUEST_CONTROLLER_DATA reply header echoes the requested pkt_dev_id; the REQUEST_CONTROLLER_COUNT reply header uses pkt_dev_id 0.
- Now VERIFIED (previously U beyond UPDATELEDS): data_size includes its own 4 bytes, and pkt_size == data_size, for the controller-data reply, profile list, plugin list, AddSegment, ConfigureZone, ConfigureDevice and LED/mode updates. 1130 and 1131 have no data_size prefix.
- Now VERIFIED (previously U), v6 ACK behaviour: the OpenRGB 1.1 server sends ACK (10) after processing every client packet, including requests that also get a reply; unknown ids get status 2; the ACK goes only to clients that negotiated >= 6, and its header echoes pkt_dev_id. For PROFILE_ABOUT_TO_LOAD (159) the doc says 'ACK', but the 1.1 client actually echoes a zero-length 159 packet, which the server counts.
- Added verified server behaviours: on packet 40 a v6 client also gets SET_SERVER_NAME (51); 52 is answered by 53 and 54 by 55; DEVICE_LIST_UPDATED (100) is pushed to ALL clients regardless of version, while detection events 101-103 go only to v6 clients; bad magic or an oversize packet closes the connection.
- Verified the interop basis for advertising v5: the OpenRGB 1.1 client waits about 1 s (200 x 5 ms) for the version reply, falls back to v0, and sends 52/54 only when the negotiated version is >= 6.
- Packet 1004 CONFIGUREDEVICE layout corrected to 'data_size, u32 flags, u16+str name' (the brief omitted data_size). Packet 1003 is 'data_size, u32 zone, Zone'. Added the header-only packets 54/55 (SET_CLIENT_HOSTNAME / SET_SERVER_HOSTNAME) to the table.
- The 'clients look for Direct' convention is partly verified: RGBControllerAPI.md says SetCustomMode 'searches the mode list for Direct, then Custom, then Static'. ResizeZone only works when the zone has MANUALLY_CONFIGURABLE_SIZE (or the _EFFECTS_ONLY variant). In v6, zone active_mode -1 means 'use the device mode'.
- Device-type Unknown shift is now VERIFIED: DEVICE_TYPE_UNKNOWN is 12 in 0.5, 15 in 0.7, 19 in 0.9 and 21 in 1.0rc2/1.1 ('DEVICE_TYPE_UNKNOWN needs to remain last').
- Grouping list was incomplete: the brief claimed 7 groups but listed 5. Added Sony DualSense 054C:0CE6 (location-based id 6&00000000&0&10) and C-Media 0D8C:0005, whose instance id contains '/' (000_0000/00/00_00000). That makes '/' a uid hazard for paths and URLs.
- Instance-id casing differs by API: DEVPKEY_Device_Parent returned lowercase hex (7&14b7adf8..., RZVIRTUAL ...&Col03) while CM_Get_Device_IDW and Get-PnpDevice return uppercase. Uids must be uppercase-normalized (SignalRGB's examples are uppercase), not just compared case-insensitively.
- Registry observation corrected: there are TWO per-device keys (PID_007D MI_03 and MI_04, each with its own Providers subkey and extra values Color and ToastInvoked) and none for the currently connected PID_007C LampArray.
- 'SignalRGB does write HKCU Software Microsoft Lighting' downgraded to strongly implied but not observed. Only strings were seen: QSettings paths HKEY_CURRENT_USER/Software/Microsoft/Lighting/Devices plus Devices/%1/AmbientLightingEnabled, ControlledByForegroundApp and Providers/1, next to 'Disabled WDL for device'. The repo spec says the exact keys are UNKNOWN.
- windows-crate module correction: the DEVPKEY_DeviceInterface_HID_* constants live in windows::Win32::Devices::HumanInterfaceDevice, not Win32_Devices_Properties; WakeScreenOnInputCapable (pid 9) also exists. CM_Get_Device_Interface_PropertyW and CM_Get_DevNode_PropertyW are themselves gated on the Win32_Devices_Properties feature.
- windows 0.62.2 does NOT re-export windows-numerics or windows-future. Naming Vector3 or IAsyncOperation needs direct dependencies (0.3). The Foundation feature is implied by Devices_Lights (Devices -> Foundation). From<TimeSpan> for core::time::Duration exists, so the manual 100 ns conversion is unnecessary.
- windows-core returns Err(Error::empty()) for null interface results, e.g. LampInfo::FixedColor() on a normal lamp or FromIdAsync resolving to null. The HAL must treat this as 'none', not as a failure.
- API naming clarified: the event is AvailabilityChanged on ILampArray2 (IID 050c181f-60a8-4711-a1af-1b1b4c658ea2); 'IsAvailableChanged' does not exist. FindAllAsync(selector) is projected as DeviceInformation::FindAllAsyncAqsFilter.
- Added SetColorsForIndices remarks (verified): the arrays must be the same size or the call fails; unsupported colors snap to the nearest supported color; non-existent indices are ignored; Black = off.
- Upgraded to VERIFIED: 'the system opens all keyboard and mouse collections for its exclusive use' (MS HID docs). Container ID inheritance for non-removable devnodes is now a documented heuristic, so internal devices can collapse into the parent or PC container (previously U). The CM_Get_Device_Interface_ListW CR_BUFFER_SMALL retry is documented by Microsoft. VHF supports HID source drivers only in kernel mode.
- Signing updates: Microsoft now calls its service 'Azure Artifact Signing (formerly Trusted Signing)'; individuals are eligible only in the USA and Canada. SignPath MSIX/AppX support is verified from SignPath docs (msix-file / appx-file elements; the previously cited S37 did not support it). The SignPath Foundation certificate is issued to SignPath Foundation, every release needs manual approval, and projects may not contain proprietary components (terms marked draft).
- Gap filled: suspend/resume researched. PowerRegisterSuspendResumeNotification (Win8+, DEVICE_NOTIFY_CALLBACK=2, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PBT_APMSUSPEND=4, PBT_APMRESUMESUSPEND=7, PBT_APMRESUMEAUTOMATIC=18). In windows 0.62.2 it needs Win32_System_Power + Win32_UI_WindowsAndMessaging, and the unregister function takes HPOWERNOTIFY(isize) while register yields *mut c_void.
- New pitfall (Rust 1.81 release notes): unwinding out of non-unwind ABIs such as extern "system" aborts the process. The CM callback, power callback and WinRT handler closures must be panic-free.
- New hotplug requirement: add per-handle CM_NOTIFY_FILTER_TYPE_DEVICEHANDLE registrations and close handles on QUERYREMOVE. Otherwise HueDeck's open HID/USB handles block query-remove (disable device or driver update).
- Workspace dependency correction: tokio, crossbeam-channel and nusb are declared in [workspace.dependencies] but are not yet in Cargo.lock (no member uses them; huedeck-api has 'TODO(M4) ... tokio'). bytes 1.12.1 is locked. HueDeck uses hidapi's windows-native feature, so the hidapi caveat applies directly.
- Added S12 usage: the DefaultLightingProvider unattend setting takes a package family name that must declare the lighting extension, which supports reading the Providers registry lists as package family names.
- Added interop notes: third-party v3 SDK servers exist (ColorHoster, GPL-3.0, Rust), and its README claims the OpenRGB GUI works as a client but that Project Aurora may require the server process to be named OpenRGB.exe (unverified claim).
- Design inconsistency flagged: the approved plan lists a first-party 'generic-lamparray' plugin, while the brief drops 0x59/0x01 TLCs from the HID plugin scan. The orchestrator must pick one.
- Minor date/wording fixes: S3 learn page updated 2026-05-28 (source ms.date 2024-12-05); S8 live page 2026-10-03 (GitHub source ms.date 2026-04-08, same content); OpenRGB 0.5/0.3 local commit dates vs UTC release dates noted; S4's 'applied only while in focus' sentence is general rather than specific to unpackaged apps. The probe rebuild (12.6 s), struct sizes (416/36), USB_DEVICE count (9), thread separation (tid 7460 vs 24580) and the embedded manifest (msix element plus linker trustInfo asInvoker) were re-verified.
- Process note (transparency): during this pass the verifier accidentally created a 'nusb/' folder holding 3 downloaded nusb source files in the repo root (shell cwd mistake). It was moved to the scratchpad and the folder removed within about a minute; no other repo file was written.

## Still unverified after the fact-check

- Whether an unpackaged, windowless huedeckd sees LampArray.IsAvailable == true, and whether its SetColor* writes are ever applied. FromIdAsync was deliberately not executed, to keep the system untouched.
- Whether daemon writes are applied while HueDeck's own UI, a separate process, has focus.
- LampArray API behaviour when 'Use Dynamic Lighting on my devices' is off. AmbientLightingEnabled=0 here, and mapping that registry value to the toggle is itself an inference.
- Whether subscribing to AvailabilityChanged without identity or the extension returns an error or silently never fires.
- Whether ILampArray2 (IsAvailable/AvailabilityChanged) is present on GA builds 22621.2361+ and 22631. The docs say 'introduced in 10.0.23504.0' (Insider).
- Dynamic Lighting GA minimum build 22621.2361: secondary sources only; the official KB5030310 notes are silent.
- Whether HueDeck appears in Background light control immediately after registration, and which name is shown. SignalRGB's extension DisplayName is literally 'Id', which suggests the package/app DisplayName is user-facing.
- Whether the AmbientLightingServer accepts an UNSIGNED identity package (OID.2.25... + AllowUnsigned), and whether registering such an external-location package needs admin.
- Exact Publisher DN string of the SignPath Foundation certificate (only 'issued to SignPath Foundation' is verified); the SignPath Foundation terms page is marked draft.
- Whether the identity package is 'in use' while huedeckd runs (upgrade flow; DeferRegistrationWhenPackagesAreInUse / ForceAppShutdown).
- Whether Executable='bin/huedeckd.exe' under an install-root ExternalLocation keeps identity, and whether the uap3:AppExtension PublicFolder directory must exist.
- Whether a trustInfo block in the input fusion manifest conflicts with link.exe's generated one (/MANIFESTUAC:NO). The test build failed for an unrelated LNK1104 reason.
- Cost and blocking behaviour of LampArray.SetColorsForIndices per frame.
- Whether Windows opens the HID LampArray (0x59) collection exclusively. This is documented for keyboards and mice only.
- Who writes HKCU/Software/Microsoft/Lighting/Providers and the per-device keys (OS vs apps), and whether SignalRGB actually writes AmbientLightingEnabled (strings only).
- CM_Register_Notification callback threading and concurrency: not documented, and no PnP event fired during this pass's probe run.
- Whether DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS must stay alive for the lifetime of a PowerRegisterSuspendResumeNotification registration (kept alive by design).
- Default socket options of std/tokio TcpListener on Windows (SO_EXCLUSIVEADDRUSE presumably not set).
- Bluetooth HID grouping by the BTHENUM/BTHLEDEVICE ancestor (no BT HID device present).
- Duplicate USB serials (e.g. Razer 000000000000) colliding for two identical units.
- hidapi-rs 2.6.7 windows-native reporting bus Unknown and interface_number -1 for RZVIRTUAL-backed collections: established by code reading, not executed.
- Format of OpenRGB controller 'location' strings for de-duplication in the client bridge.
- Whether third-party OpenRGB-aware tools (Aurora, Artemis, etc.) connect to a non-OpenRGB server, or check for an OpenRGB.exe process (ColorHoster README claim only). Also whether the OpenRGB 1.1 GUI's SDK-client UI flow works against HueDeck; its protocol path is verified in source.
- Whether any MIT-licensed GitHub app (not on crates.io) contains a self-contained OpenRGB protocol implementation (not audited).
