<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/msix-identity/

The HueDeck **identity package**: a sparse MSIX with external location. It holds no
program files. It gives `huedeckd.exe`, installed anywhere outside the package, a package
identity, and declares the `com.microsoft.windows.lighting` app extension. Windows needs
both before it lets a background process drive Dynamic Lighting (LampArray) devices as an
**ambient lighting controller**. Without it, Windows gives those devices only to the
foreground app, which the daemon never is. HueDeck then lists them and shows a warning, but
does not light them. Design and sources: `docs/windows-dynamic-lighting.md` and
`docs/research/m6-windows-integrations.md` §1.4.

| File | What |
|---|---|
| `AppxManifest.xml.in` | The package manifest. `build.ps1` fills `@NAME@` (`HueDeck.Lighting`), `@PUBLISHER@`, `@VERSION@` and `@EXECUTABLE@`. |
| `huedeckd.exe.manifest.in` | The side-by-side manifest fragment for `huedeckd.exe`. Its `<msix>` element ties the executable to the package; publisher, packageName and applicationId must match the package manifest. |
| `build.ps1` | Stages the manifest and the logos (rendered from `packaging/wix/huedeck.ico`), then packs with `MakeAppx.exe pack /nv`. It signs with `SignTool.exe` when given `-Pfx`, and with `-EmbedIn <huedeckd.exe>` merges the fragment into the executable with `mt.exe`. `-StageOnly` stops before packing. |

`build.ps1` **never installs, registers or trusts anything**. It prints the commands for
you to run. It needs the Windows SDK (MakeAppx, SignTool, mt) on PATH or under
`%ProgramFiles(x86)%\Windows Kits\10\bin`.

## Recipe

1. **Pick the publisher.** `Publisher` must be exactly the subject DN of the signing
   certificate.
   - Release builds: the code-signing certificate (SignPath Foundation or Azure Artifact
     Signing; not chosen yet, see `packaging/README.md`).
   - Development: a self-signed certificate, for example
     `New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=HueDeck Dev" -CertStoreLocation Cert:\CurrentUser\My`,
     exported to `dev.pfx` and `dev.cer`.
   - Windows 11 also takes unsigned packages whose publisher ends in
     `, OID.2.25.311729368913984317654407730594956997722=1`, registered with
     `-AllowUnsigned`. Whether the ambient lighting server accepts such an identity is not
     known yet.
2. **Build:**
   `./build.ps1 -Publisher "CN=HueDeck Dev" -Pfx dev.pfx -PfxPassword (Read-Host -AsSecureString) -EmbedIn <install dir>\huedeckd.exe`.
   Every rebuild that is registered again needs a higher `-Version`: Windows refuses to
   register a version that is already registered (0x80073CF9).
3. **Trust the dev certificate** (not needed for release certificates):
   `Import-Certificate -FilePath dev.cer -CertStoreLocation Cert:\CurrentUser\TrustedPeople`.
   Without it, registration fails with 0x800B0109.
4. **Register, per user, not elevated:**
   `Add-AppxPackage -Path out\huedeck-identity.msix -ExternalLocation <install dir>`.
   - `<install dir>` must be exactly the directory that holds `huedeckd.exe`.
     `-Executable` is relative to it.
   - If the external location is wrong, the package registers but `huedeckd` runs with
     no identity, and Windows reports no error.
   - Stop `huedeckd` first when re-registering.
   - Undo with `Get-AppxPackage HueDeck.Lighting | Remove-AppxPackage`.
   - Diagnostics: Event Viewer > Applications and Services Logs > Microsoft > Windows >
     AppxDeployment-Server.
     - 0x80073D54: the executable's `<msix>` element does not match the package.
     - 0x80073CF6: invalid manifest.
5. **Give HueDeck the devices.** Open Settings > Personalization > Dynamic Lighting
   (`ms-settings:personalization-lighting`) and:
   - turn on *Use Dynamic Lighting on my devices*;
   - drag HueDeck to the top of *Background light control*;
   - optionally turn off *Compatible apps in the foreground always control lighting*.

   These settings are per device and per USB port.

HueDeck never writes `HKCU\Software\Microsoft\Lighting` itself.

## Dev loop

`cargo run` executables have no identity unless a dev package is registered whose
external location is the cargo target directory (`target\debug`) and whose `huedeckd.exe`
carries the `<msix>` manifest (`-EmbedIn target\debug\huedeckd.exe` after each build).
