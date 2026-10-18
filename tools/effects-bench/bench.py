# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Effects bench: every effect through lightloom-renderer, GPU and software compositing.

Each effect paints 300 frames at 30 fps in each mode; the table has the paint-interval
percentiles, dropped frames, console errors, the non-black fraction of frames 10/100/290 and
the renderer process tree's peak working set and private bytes (sampled with PowerShell, so
the memory columns are Windows-only).

Usage: python -I bench.py <lightloom-renderer> <out_dir> <effects_dir> [more effect files...]
Writes <out_dir>/bench.json and prints one line per run. Effects from a SignalRGB
installation are passed as paths and read in place.
"""
import glob
import json
import os
import subprocess
import sys
import threading
import time

exe, out_dir, effects_dir = sys.argv[1:4]
extra = sys.argv[4:]
os.makedirs(out_dir, exist_ok=True)
effects = sorted(glob.glob(os.path.join(effects_dir, "*.html"))) + extra
MODES = [("gpu", []), ("sw", ["--software-compositing"])]


def tree_memory():
    """Working set and private bytes (MB) summed over every lightloom-renderer process."""
    ps = ("$p = Get-Process lightloom-renderer -ErrorAction SilentlyContinue; "
          "if ($p) { '{0} {1} {2}' -f ($p | Measure-Object WorkingSet64 -Sum).Sum, "
          "($p | Measure-Object PrivateMemorySize64 -Sum).Sum, @($p).Count }")
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True,
                             text=True, timeout=10).stdout.split()
        if len(out) == 3:
            return int(out[0]) / 2**20, int(out[1]) / 2**20, int(out[2])
    except Exception:
        pass
    return None


results = []
for i, eff in enumerate(effects):
    for mode, flags in MODES:
        tag = f"e{i:02d}-{mode}"
        ring = os.path.join(out_dir, tag + ".shm")
        cmd = [exe, "--effect", eff, "--out", ring, "--frames", "300", "--fps", "30",
               "--snapshot-frames", "10,100,290", "--png", os.path.join(out_dir, tag + ".png")] + flags
        samples = []
        stop = threading.Event()

        def sampler():
            time.sleep(3)
            while not stop.is_set():
                m = tree_memory()
                if m:
                    samples.append(m)
                stop.wait(1.5)

        th = threading.Thread(target=sampler, daemon=True)
        th.start()
        t0 = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        stop.set()
        th.join()
        try:
            r = json.loads(p.stdout)
        except Exception:
            r = {"parse_error": p.stdout[:300], "stderr": p.stderr[-500:]}
        iv = r.get("paint_interval") or {}
        snaps = r.get("snapshots") or []
        row = {
            "effect": eff, "mode": mode, "exit": p.returncode, "wall_s": round(time.time() - t0, 1),
            "frames": r.get("frames"), "fps": r.get("effective_fps"),
            "p50": iv.get("p50_ms"), "p99": iv.get("p99_ms"), "max": iv.get("max_ms"), "dropped": iv.get("dropped"),
            "first_paint_ms": (r.get("timing") or {}).get("first_paint_ms"),
            "console_errors": r.get("console_errors"), "console": (r.get("console") or [])[:5],
            "nonblack": [s.get("nonblack_fraction") for s in snaps],
            "luma": [s.get("mean_luma") for s in snaps],
            "hashes": [s.get("sha256", "")[:12] for s in snaps],
            "mem_ws_max": round(max((s[0] for s in samples), default=0), 1),
            "mem_priv_max": round(max((s[1] for s in samples), default=0), 1),
            "procs": max((s[2] for s in samples), default=0),
        }
        results.append(row)
        print(f"{tag} {os.path.basename(eff)[:28]:28} exit={row['exit']} frames={row['frames']} fps={row['fps']} "
              f"p99={row['p99']} drop={row['dropped']} cerr={row['console_errors']} nb={row['nonblack']} "
              f"ws={row['mem_ws_max']} priv={row['mem_priv_max']}", flush=True)
        for f in glob.glob(ring + "*"):
            if os.path.isfile(f) and f.endswith(".shm"):
                try:
                    os.remove(f)
                except OSError:
                    pass

with open(os.path.join(out_dir, "bench.json"), "w", encoding="utf-8") as f:
    json.dump(results, f, indent=1)
print("done", len(results))
