<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/winget

| File | Purpose |
|---|---|
| `template/Lightloom.Lightloom.yaml` | version manifest |
| `template/Lightloom.Lightloom.installer.yaml` | installer manifest: `InstallerType: wix`, `Scope: user`, the MSI's URL, SHA-256 and `ProductCode`, `UpgradeCode` in `AppsAndFeaturesEntries` |
| `template/Lightloom.Lightloom.locale.en-US.yaml` | default locale: publisher, description, licence, tags |
| `render.ps1` | fills the templates for one release and writes `manifests/l/Lightloom/Lightloom/<version>/` (git-ignored) |

Manifest schema 1.10.0. The rendered set validates against
`manifest.{version,installer,defaultLocale}.1.10.0.json` from `microsoft/winget-cli`
(`schemas/JSON/manifests/v1.10.0/`).

## Why the ProductCode comes from the published MSI

`packaging/wix/lightloom.wxs` sets a fixed `UpgradeCode` (`{1C9B0603-FD78-41A6-BE05-78B638CDAD01}`)
and no `ProductCode`, so WiX generates a new ProductCode for every build. Each version is a
major upgrade of the previous one (`MajorUpgrade`). winget matches the installed package by
ProductCode (and by the UpgradeCode in `AppsAndFeaturesEntries`), so the manifest must carry the
ProductCode of the exact MSI on the release page. `render.ps1 -Tag` reads it from that file;
never render from a local build.

## Submitting a release

Needs Windows (render.ps1 reads the MSI through the Windows Installer COM object), a published
(not draft) GitHub release, and a GitHub account with a fork of `microsoft/winget-pkgs`.

1. Render and check the manifests:

   ```powershell
   packaging/winget/render.ps1 -Tag v0.1.0
   winget validate --manifest packaging/winget/manifests/l/Lightloom/Lightloom/0.1.0
   ```

   `render.ps1` downloads `SHA256SUMS` and `Lightloom-0.1.0-x64.msi` from the release, stops if
   the MSI does not match `SHA256SUMS` or its ProductVersion does not match the tag, and prints
   the ProductCode it used.

2. Optional, on a test machine or Windows Sandbox: install from the local manifest
   (`winget settings --enable LocalManifestFiles` once, as administrator), then
   `winget install --manifest packaging/winget/manifests/l/Lightloom/Lightloom/0.1.0`. It must
   install without a UAC prompt into `%LOCALAPPDATA%\Programs\Lightloom`.

3. Submit, either way:

   - **wingetcreate** (`winget install Microsoft.WingetCreate`):

     ```powershell
     wingetcreate submit --token <GitHub token with public_repo scope> `
       packaging/winget/manifests/l/Lightloom/Lightloom/0.1.0
     ```

     It forks `microsoft/winget-pkgs` if needed, pushes a branch and opens the pull request.

   - **By hand**: in a fork of `microsoft/winget-pkgs`, branch from `master`, copy the three
     files to `manifests/l/Lightloom/Lightloom/0.1.0/`, commit ("New package:
     Lightloom.Lightloom version 0.1.0"), push, and open a pull request against
     `microsoft/winget-pkgs` `master`, filling in its checklist.

4. Watch the pull request: the validation pipeline installs the MSI in a VM, scans it, and labels
   the pull request (`Azure-Pipeline-Passed`, `Validation-Completed`). A moderator merges it;
   `winget install Lightloom.Lightloom` works a few hours after the merge.

Later versions: the same steps with the new tag; the first pull request is "New package", every
later one "New version". `wingetcreate update Lightloom.Lightloom --version <v> --urls <msi url>
--submit` also works, but it takes the ProductCode from the MSI it downloads as well, so the
result is the same.

## Things the validation pipeline may flag

- **Unsigned MSI.** Releases are not Authenticode-signed yet (SIGNING in
  `.github/workflows/release-build.yml`). winget-pkgs accepts unsigned installers, but
  Defender/SmartScreen reputation checks can delay or block the pull request
  (`Needs-Manual-Validation` or `Validation-Defender-Error`). Answer the moderator with the
  link to the release workflow run.
- **Per-user install.** `Scope: user` and `InstallerType: wix` make winget pass `/quiet` (or
  `/passive`) and no `ALLUSERS`; the package is `Scope="perUser"`, so it never elevates. A
  silent install does not install PawnIO (RAM lighting): the description says so.
- **Publisher name.** `AppsAndFeaturesEntries.Publisher` must equal the MSI's `Manufacturer`
  (`Lightloom contributors` in `lightloom.wxs`); change both together.
