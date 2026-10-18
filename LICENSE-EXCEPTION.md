<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck add-on exception

HueDeck is free software: you can redistribute it and/or modify it under the terms
of the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version. The full
text is in [LICENSE](LICENSE). HueDeck comes with no warranty.

The HueDeck authors grant the additional permission below so that device plugins,
effects and other add-ons can be written and shared under any license, and so that
community add-ons keep running in HueDeck whatever their license.

## Additional permission under GNU GPL version 3 section 7

1. **Add-ons.** An "Add-on" is a work that interacts with HueDeck only through
   HueDeck's add-on interfaces:
   - the **device plugin interface**: the objects and functions HueDeck makes
     available to device plugins (such as `device`, `bus`, `service` and the modules
     imported as `@SignalRGB/...` or `@huedeck/...`) and the exports HueDeck calls
     (such as `Validate`, `Initialize`, `Render` and `Shutdown`), documented in
     `docs/host-api/`;
   - the **effect interface**: HTML effect pages, their `<meta>` controls and the
     `engine` object HueDeck makes available to them;
   - the **file formats** of plugin manifests, components, layouts and presets
     described in `schemas/`;
   - the **local APIs** of `huedeckd` (HTTP and WebSocket).

2. **Permission.** You may write, use, copy, modify and convey an Add-on under terms
   of your choice. HueDeck may load and run it, and you may convey HueDeck together
   with Add-ons, without the Add-ons becoming subject to the GNU GPL. The
   Corresponding Source of such a combination does not include the source of those
   Add-ons.

3. **Limits.** Using the add-on interfaces does not make an Add-on a work based on
   HueDeck. Copying or adapting HueDeck's own GPL-licensed code does: an add-on that
   contains code taken from HueDeck (for example a function from one of the device
   plugins in `plugins/` or a module from `prelude/`) is covered by the GNU GPL like
   the code it took.

4. **Removal.** As section 7 of the GNU GPL allows, you may remove this additional
   permission from your copy of HueDeck, or from any part of it, when you convey it.

This permission is granted by the HueDeck authors for the code they hold the
copyright to. Code that HueDeck adapted from other GPL projects (each such file names
its source and its authors) is covered by the GNU GPL only.
