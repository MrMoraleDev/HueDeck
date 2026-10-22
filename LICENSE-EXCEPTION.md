<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom add-on exception

Lightloom is free software: you can redistribute it and/or modify it under the terms
of the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version. The full
text is in [LICENSE](LICENSE). Lightloom comes with no warranty.

The Lightloom authors grant the additional permission below so that device plugins,
effects and other add-ons can be written and shared under any license, and so that
community add-ons keep running in Lightloom whatever their license.

## Additional permission under GNU GPL version 3 section 7

1. **Add-ons.** An "Add-on" is a work that interacts with Lightloom only through
   Lightloom's add-on interfaces:
   - the **device plugin interface**: the objects and functions Lightloom makes
     available to device plugins (such as `device`, `bus`, `service` and the modules
     imported as `@SignalRGB/...` or `@lightloom/...`) and the exports Lightloom calls
     (such as `Validate`, `Initialize`, `Render` and `Shutdown`), documented in
     `docs/host-api/`;
   - the **effect interface**: HTML effect pages, their `<meta>` controls and the
     `engine` object Lightloom makes available to them;
   - the **file formats** of plugin manifests, components, layouts and presets
     described in `schemas/`;
   - the **local APIs** of `lightloomd` (HTTP and WebSocket).

2. **Permission.** You may write, use, copy, modify and convey an Add-on under terms
   of your choice. Lightloom may load and run it, and you may convey Lightloom together
   with Add-ons, without the Add-ons becoming subject to the GNU GPL. The
   Corresponding Source of such a combination does not include the source of those
   Add-ons.

3. **Limits.** Using the add-on interfaces does not make an Add-on a work based on
   Lightloom. Copying or adapting Lightloom's own GPL-licensed code does: an add-on that
   contains code taken from Lightloom (for example a function from one of the device
   plugins in `plugins/` or a module from `prelude/`) is covered by the GNU GPL like
   the code it took.

4. **Removal.** As section 7 of the GNU GPL allows, you may remove this additional
   permission from your copy of Lightloom, or from any part of it, when you convey it.

This permission is granted by the Lightloom authors for the code they hold the
copyright to. Code that Lightloom adapted from other GPL projects (each such file names
its source and its authors) is covered by the GNU GPL only.
