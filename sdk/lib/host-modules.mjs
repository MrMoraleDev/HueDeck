// SPDX-License-Identifier: GPL-3.0-or-later
//
// `@SignalRGB/<name>` and `@lightloom/<name>` imports under Node, resolved like the Lightloom
// host does (prelude/modules/README.md): the pure-JS shims come from `prelude/`, the
// Rust-backed modules from their throwing facades (`HostApiMissing` when called), except
// `permissions` and `systeminfo`, which get the answers of a Lightloom device session on a
// machine with no facts (every partner permission granted; no motherboard / RAM data).
// `@lightloom/`-only names (BinaryUtils, createColorArray, hexToRgb) do not resolve under
// `@SignalRGB/`, as in the host.

import { registerHooks } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const prelude = join(repo, "prelude");
const stubs = join(dirname(fileURLToPath(import.meta.url)), "modules");

const PURE = { Errors: "Errors.js", base64: "base64.js" };
const LIGHTLOOM_ONLY = { BinaryUtils: "BinaryUtils.js", createColorArray: "createColorArray.js", hexToRgb: "hexToRgb.js" };
const FACADES = ["udp", "tcp", "serial", "lcd", "DeviceDiscovery", "performance", "appInfo"];
const SESSION = { permissions: "permissions.mjs", systeminfo: "systeminfo.mjs" };

/** The file a host specifier resolves to, or undefined when the host would refuse it. */
export function hostModuleFile(specifier) {
  const prefix = ["@SignalRGB/", "@lightloom/"].find((p) => specifier.startsWith(p));
  if (prefix === undefined) return undefined;
  const name = specifier.slice(prefix.length).replace(/\.js$/, "");
  if (Object.hasOwn(PURE, name)) return join(prelude, PURE[name]);
  if (Object.hasOwn(LIGHTLOOM_ONLY, name)) return prefix === "@lightloom/" ? join(prelude, LIGHTLOOM_ONLY[name]) : undefined;
  if (Object.hasOwn(SESSION, name)) return join(stubs, SESSION[name]);
  if (FACADES.includes(name)) return join(prelude, "modules", `${name}.js`);
  return undefined;
}

let registered = false;

/** Install the resolve hook once per process. */
export function registerHostModules() {
  if (registered) return;
  registered = true;
  registerHooks({
    resolve(specifier, context, nextResolve) {
      if (specifier.startsWith("@SignalRGB/") || specifier.startsWith("@lightloom/")) {
        const file = hostModuleFile(specifier);
        if (file === undefined) {
          throw new Error(`HostApiMissing: import '${specifier}' is not provided by the Lightloom host`);
        }
        return { url: pathToFileURL(file).href, shortCircuit: true, format: "module" };
      }
      return nextResolve(specifier, context);
    },
  });
}
