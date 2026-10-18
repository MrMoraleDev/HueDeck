# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarise a Lightloom HID trace: operations in order, grouped by packet "shape".

Usage: python -I shapes.py <trace.json> [head_bytes]
A shape is (op, path suffix, first `head_bytes` bytes). For each shape: count, first and
last event index, and the byte positions that vary across its packets.
Prints the first 40 events verbatim (init) and the last 20 (shutdown).
"""
import json
import sys

trace = json.load(open(sys.argv[1], encoding="utf-8"))
head = int(sys.argv[2]) if len(sys.argv) > 2 else 3
events = trace["events"]


def data(e):
    for k in ("data", "sent", "bytes", "reply"):
        if isinstance(e.get(k), str):
            return bytes.fromhex(e[k])
    return b""


def path_tag(e):
    p = e.get("path", "")
    for part in p.split("#"):
        if part.startswith("VID_") or "MI_" in part:
            return part[-24:]
    return p[-30:]


def describe(i, e):
    d = data(e)
    extra = {k: v for k, v in e.items() if k not in ("t_us", "path", "data", "op")}
    return f"{i:5d} {e.get('t_us', 0) / 1000:9.1f}ms {e.get('op'):<14} {path_tag(e):<26} len={len(d):<3} {d.hex()[:96]}{'...' if len(d) > 48 else ''} {extra if extra else ''}"


print(f"{len(events)} events; devices: {[d['key'] for d in trace['devices']]}")
print("--- first 40 ---")
for i, e in enumerate(events[:40]):
    print(describe(i, e))
print("--- last 20 ---")
for i, e in enumerate(events[-20:], start=len(events) - 20):
    print(describe(i, e))

shapes = {}
for i, e in enumerate(events):
    d = data(e)
    key = (e.get("op"), path_tag(e), d[:head].hex(), len(d))
    s = shapes.setdefault(key, {"count": 0, "first": i, "last": i, "packets": []})
    s["count"] += 1
    s["last"] = i
    if len(s["packets"]) < 400:
        s["packets"].append(d)

print("--- shapes ---")
for key, s in sorted(shapes.items(), key=lambda kv: kv[1]["first"]):
    pk = s["packets"]
    varying = []
    if len(pk) > 1:
        n = min(len(p) for p in pk)
        varying = [j for j in range(n) if len({p[j] for p in pk}) > 1]
    print(f"{key[0]:<14} {key[1]:<26} head={key[2]:<10} len={key[3]:<4} x{s['count']:<6} events {s['first']}..{s['last']} varying={varying[:40]}")
