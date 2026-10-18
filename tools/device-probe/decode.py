# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Decode the coordinate-probe trace: which canvas cell each LED byte triple sampled.

The probe effect paints R = round(x*255/319), G = round(y*255/199), B in {0x40,0x80,0xC0};
every device is placed at (0,0) with scale 10, so an LED of grid cell (lx, ly) samples the
pixel (lx*10+5, ly*10+5) -> R = round((lx*10+5)*255/319), G = round((ly*10+5)*255/199).

Usage: python -I decode.py <trace.json> <device-key-prefix>
Prints, per distinct packet shape of that device, the decoded cells of RGB/GRB triples found
at each offset (only triples whose third component is one of the probe's blues).
"""
import json
import sys

trace = json.load(open(sys.argv[1], encoding="utf-8"))
prefix = sys.argv[2].lower()
BLUES = {0x40, 0x80, 0xC0}

cells_r = {round((lx * 10 + 5) * 255 / 319): lx for lx in range(32)}
cells_g = {round((ly * 10 + 5) * 255 / 199): ly for ly in range(20)}

paths = set()
for d in trace["devices"]:
    if d["key"].lower().startswith(prefix):
        paths.update(d["paths"])


def decode(r, g, b):
    if b not in BLUES:
        return None
    if r in cells_r and g in cells_g:
        return (cells_r[r], cells_g[g], hex(b))
    return None


seen = {}
for i, e in enumerate(trace["events"]):
    if e.get("path") not in paths or not e.get("data"):
        continue
    data = bytes.fromhex(e["data"])
    key = (e["op"], data[:4].hex())
    found = []
    for off in range(len(data) - 2):
        for order, (a, b_, c) in (("RGB", (0, 1, 2)), ("GRB", (1, 0, 2))):
            r, g, blue = data[off + a], data[off + b_], data[off + c]
            cell = decode(r, g, blue)
            if cell:
                found.append((off, order, cell))
    if found and key not in seen:
        seen[key] = (i, found)
for key, (i, found) in seen.items():
    print(f"event {i} {key}:")
    for off, order, cell in found:
        print(f"   offset {off:3d} {order} cell={cell}")
