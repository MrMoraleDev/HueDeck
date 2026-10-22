# SPDX-License-Identifier: GPL-3.0-or-later
"""PSNR of lightloom-renderer (CEF) frames against stock Chrome headless on the same page.

Each effect is wrapped in a harness copy that freezes time (rAF timestamp, performance.now,
Date), seeds Math.random, defines the control defaults and a zeroed window.engine, runs
SignalRGB's load sequence (on<prop>Changed hooks, onEngineReady) in stock Chrome (Lightloom's
renderer injects the same itself) and lets exactly one animation frame run. Both browsers
render that copy with the same compositing mode; the last CEF paint is compared with Chrome's
screenshot.

Needs numpy and Pillow, and a Chrome or Chromium to compare with.

Usage: python -I psnr.py <lightloom-renderer> <chrome> <out_dir> <effect.html>...
Writes <out_dir>/psnr.json and prints one line per effect and mode.
"""
import html
import json
import math
import os
import pathlib
import re
import subprocess
import sys

import numpy as np
from PIL import Image

renderer, chrome, out_dir = sys.argv[1:4]
effects = sys.argv[4:]
os.makedirs(out_dir, exist_ok=True)

META = re.compile(r"<meta\s+[^>]*property\s*=\s*\"([^\"]+)\"[^>]*>", re.I)
ATTR = re.compile(r"(\w+)\s*=\s*\"([^\"]*)\"")
NL = chr(10)


def controls(src):
    """`var` statements with the defaults, and the hook names of the load sequence."""
    out, hooks = [], []
    src = re.sub(r"<!--.*?-->", "", src, flags=re.S)
    for m in META.finditer(src):
        attrs = {k.lower(): html.unescape(v) for k, v in ATTR.findall(m.group(0))}
        name, kind, default = attrs.get("property"), attrs.get("type", "").lower(), attrs.get("default", "")
        if not name or not re.fullmatch(r"[A-Za-z_$][\w$]*", name):
            continue
        if kind in ("number", "hue"):
            try:
                lit = json.dumps(float(default) if "." in default else int(default))
            except ValueError:
                lit = "0"
        elif kind == "boolean":
            lit = "true" if default.lower() in ("true", "1") else "false"
        else:
            lit = json.dumps(default)
        out.append(f"var {name} = {lit};")
        hooks.append(f"on{name}Changed")
    return out, hooks + ["onEngineReady"]


PRELUDE = """<script>
var __lightloomInjected = !!window.engine;
(function () {
  var RealDate = Date, fixedMs = 1700000000000;
  function FixedDate() {
    if (arguments.length) return new (Function.prototype.bind.apply(RealDate, [null].concat([].slice.call(arguments))))();
    return new RealDate(fixedMs);
  }
  FixedDate.now = function () { return fixedMs; };
  FixedDate.UTC = RealDate.UTC;
  FixedDate.parse = RealDate.parse;
  FixedDate.prototype = RealDate.prototype;
  window.Date = FixedDate;
  // The one animation frame runs after load and the hooks, in both browsers.
  var n = 0, raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = function (cb) { if (n++ < 1) window.__pendingFrame = cb; return 0; };
  window.__runFrame = function () { if (window.__pendingFrame) raf(function () { window.__pendingFrame(1000); }); };
  performance.now = function () { return 1000; };
  var s = 42;
  Math.random = function () { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 4294967296; };
})();
window.engine = window.engine || { audio: { freq: new Int8Array(200), level: -100, rawlevel: 0, width: 0, density: 0 },
  zone: { hue: new Int16Array(560), saturation: new Int8Array(560), lightness: new Int8Array(560), imagedata: new Uint8ClampedArray(64000) },
  vision: {}, sensors: {}, getSensorValue: function () { return null; } };
@VARS@
window.addEventListener("load", function () {
  if (!__lightloomInjected) {
    @HOOKS@.forEach(function (h) { if (typeof window[h] === "function") { try { window[h](); } catch (e) { console.error(e); } } });
  }
  window.__runFrame();
});
</script>
"""


def harness(effect, dest):
    src = pathlib.Path(effect).read_text(encoding="utf-8", errors="replace")
    base = pathlib.Path(effect).resolve().parent.as_uri() + "/"
    lines, hooks = controls(src)
    inject = f'<base href="{base}">' + PRELUDE.replace("@VARS@", NL.join(lines)).replace("@HOOKS@", json.dumps(hooks))
    m = re.search(r"<head[^>]*>", src, re.I)
    page = src[: m.end()] + inject + src[m.end():] if m else inject + src
    pathlib.Path(dest).write_text(page, encoding="utf-8")


def cef_frame(page, tag, sw):
    ring = os.path.join(out_dir, tag + ".shm")
    png = os.path.join(out_dir, tag + "-cef.png")
    cmd = [renderer, "--effect", page, "--out", ring, "--frames", "3", "--snapshot-frames", "1,2,3",
           "--png", png, "--timeout-factor", "40"] + (["--software-compositing"] if sw else [])
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    r = json.loads(p.stdout)
    snaps = [s for s in r.get("snapshots", []) if s.get("png")]
    return (snaps[-1]["png"] if snaps else None), r


def chrome_frame(page, tag, sw):
    png = os.path.join(out_dir, tag + "-chrome.png")
    profile = os.path.join(out_dir, "chrome-profile")
    cmd = [chrome, "--headless", "--hide-scrollbars", "--no-first-run", f"--user-data-dir={profile}",
           "--window-size=320,200", "--force-device-scale-factor=1", "--virtual-time-budget=3000",
           "--default-background-color=000000ff", f"--screenshot={png}"]
    cmd += ["--disable-gpu-compositing"] if sw else []
    cmd.append(pathlib.Path(page).as_uri())
    subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    return png if os.path.exists(png) else None


def psnr(a, b):
    x = np.asarray(Image.open(a).convert("RGB"), dtype=np.float64)
    y = np.asarray(Image.open(b).convert("RGB"), dtype=np.float64)
    if x.shape != y.shape:
        return None, f"shape {x.shape} vs {y.shape}"
    mse = np.mean((x - y) ** 2)
    diff = int(np.count_nonzero(np.any(x != y, axis=2)))
    value = math.inf if mse == 0 else 10 * math.log10(255 ** 2 / mse)
    return value, f"{diff} px differ, max {int(np.max(np.abs(x - y)))}"


rows = []
for i, eff in enumerate(effects):
    page = os.path.join(out_dir, f"p{i:02d}.html")
    harness(eff, page)
    for sw in (True, False):
        tag = f"p{i:02d}-{'sw' if sw else 'gpu'}"
        cef_png, rep = cef_frame(page, tag, sw)
        chrome_png = chrome_frame(page, tag, sw)
        val, note = (None, "missing frame") if not (cef_png and chrome_png) else psnr(cef_png, chrome_png)
        luma = float(np.mean(np.asarray(Image.open(cef_png).convert("L")))) if cef_png else None
        rows.append({"effect": eff, "mode": "sw" if sw else "gpu", "psnr_db": val, "note": note,
                     "cef_frames": rep.get("frames"), "cef_luma": luma, "console_errors": rep.get("console_errors"),
                     "console": rep.get("console", [])[:3]})
        shown = "inf" if val == math.inf else (f"{val:.1f}" if val is not None else "-")
        print(f"{tag} {os.path.basename(eff)[:26]:26} PSNR={shown:>5} dB  {note}  luma={luma}", flush=True)

with open(os.path.join(out_dir, "psnr.json"), "w", encoding="utf-8") as f:
    json.dump(rows, f, indent=1, default=str)
