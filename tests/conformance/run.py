#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Lightloom conformance runner (stdlib only; run with an isolated interpreter).

    python -I tests/conformance/run.py --lightloom PATH [options]

Three stages, each one selectable with --only (default: all three):

dryrun
    ``lightloom compat dry-run DIR --initialize --json`` over every plugin corpus
    directory given with --corpus (repeatable), in $LIGHTLOOM_CONFORMANCE_CORPUS
    (os.pathsep-separated) or in the git-ignored ``tests/conformance/corpus/``.
    Each corpus is judged on its own: at least --parse-threshold percent of its
    files must parse (default 100) and at least --initialize-threshold percent
    of the Initialize() calls must succeed (default 95). The API gaps, the
    Initialize error classes and the top errors are reported, never judged.
    The corpus is the user's own copy (e.g. a SignalRGB plugin cache): nothing
    of it is copied into the report beyond file names and error lines.

replay
    ``lightloom compat replay TRACE --plugins DIR`` for every recorded HID trace:
    every ``*.json`` under ``plugins/*/fixtures/`` whose top level is a Lightloom
    trace (``version``, ``endpoints``, ``events``; recorded with
    ``lightloom light <colour> --record FILE``), plus those under each --traces
    DIR. Packet expectations and other fixtures are listed as skipped. A trace
    that diverges, stalls or fails a session fails the stage. --require-traces
    fails it when no trace was found.

psnr
    ``tools/effects-bench/psnr.py`` (needs numpy and Pillow, the
    ``lightloom-renderer`` with the ``cef`` feature and a stock Chrome) over the
    --effect files (default: ``effects/*.html``); every comparison must reach
    --psnr-threshold dB (default 40). Skipped unless --renderer and --chrome
    are given.

A stage without its inputs (no corpus, no renderer) is skipped, not failed,
unless --strict. Results go to OUT/conformance.json and OUT/conformance.md
(--out, default a fresh directory under the system temp dir); the Markdown is
appended to $GITHUB_STEP_SUMMARY when set. Exit status: 0 when every stage that
ran passed, 1 when one failed, 2 on a usage error.

Nothing here touches hardware: the dry run uses the recording stub host and the
replay plays the trace back through an in-memory HID backend. LIGHTLOOM_HAL is
set to ``mock`` for the child processes all the same.
"""

from __future__ import annotations

import argparse
import datetime
import glob
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, os.pardir, os.pardir))
DEFAULT_CORPUS = os.path.join(HERE, "corpus")
PSNR_SCRIPT = os.path.join(REPO, "tools", "effects-bench", "psnr.py")
STAGES = ("dryrun", "replay", "psnr")

PASS, FAIL, SKIP = "pass", "fail", "skip"


# --- pure helpers (unit-tested in test_run.py) --------------------------------


def pct(n: int, d: int) -> float | None:
    """`n` of `d` in percent; None for an empty denominator."""
    return None if d == 0 else 100.0 * n / d


def judge_dryrun(summary: dict, parse_threshold: float, initialize_threshold: float) -> dict:
    """Verdict on one `compat dry-run --initialize` summary."""
    files = int(summary.get("files", 0))
    parsed = int(summary.get("parsed", 0))
    called = int(summary.get("initialize_called", 0))
    ok = int(summary.get("initialize_ok", 0))
    parse_pct, init_pct = pct(parsed, files), pct(ok, called)
    problems = []
    if files == 0:
        problems.append("the corpus has no plugin files")
    elif parse_pct < parse_threshold:
        problems.append(f"{parsed}/{files} files parse ({parse_pct:.1f} % < {parse_threshold:g} %)")
    if called == 0 and files:
        problems.append("Initialize() was called for no plugin")
    elif called and init_pct < initialize_threshold:
        problems.append(
            f"{ok}/{called} Initialize() calls succeed ({init_pct:.1f} % < {initialize_threshold:g} %)"
        )
    return {
        "status": FAIL if problems else PASS,
        "problems": problems,
        "files": files,
        "parsed": parsed,
        "parse_pct": parse_pct,
        "plugins": int(summary.get("plugins", 0)),
        "initialize_called": called,
        "initialize_ok": ok,
        "initialize_pct": init_pct,
        "api_gaps": [list(g) for g in summary.get("top_api_gaps", [])],
        "initialize_error_classes": [list(c) for c in summary.get("initialize_error_classes", [])],
        "top_errors": [list(e) for e in summary.get("top_errors", [])][:10],
    }


def is_trace(doc) -> bool:
    """Whether a parsed JSON document is a Lightloom HID trace (`lightloom_hal::trace::HidTrace`)."""
    return (
        isinstance(doc, dict)
        and isinstance(doc.get("version"), int)
        and not isinstance(doc.get("version"), bool)
        and isinstance(doc.get("endpoints"), list)
        and isinstance(doc.get("events"), list)
    )


def find_traces(roots: list[str]) -> tuple[list[str], list[str]]:
    """(trace files, other JSON files) under `roots`, each sorted."""
    traces, others = [], []
    for root in roots:
        for path in sorted(glob.glob(os.path.join(os.path.normpath(root), "**", "*.json"), recursive=True)):
            try:
                with open(path, encoding="utf-8") as f:
                    doc = json.load(f)
            except (OSError, ValueError):
                others.append(path)
                continue
            (traces if is_trace(doc) else others).append(path)
    return traces, others


def judge_psnr(rows: list[dict], threshold: float) -> dict:
    """Verdict on the rows of psnr.py's psnr.json."""
    bad = []
    for r in rows:
        v = r.get("psnr_db")
        if isinstance(v, str):  # psnr.py writes non-floats with default=str
            try:
                v = float(v)
            except ValueError:
                v = None
        if v is None or (not math.isinf(v) and v < threshold):
            bad.append(f"{os.path.basename(str(r.get('effect')))} ({r.get('mode')}): "
                       f"{'no frame' if v is None else f'{v:.1f} dB'} {r.get('note', '')}".rstrip())
    problems = [f"{len(bad)} of {len(rows)} comparisons below {threshold:g} dB"] if bad else []
    if not rows:
        problems.append("psnr.py produced no comparison")
    return {"status": FAIL if problems else PASS, "problems": problems, "below": bad, "comparisons": len(rows)}


def overall(stages: dict, strict: bool) -> str:
    """Pass when nothing failed (and, with `strict`, nothing was skipped)."""
    statuses = [s["status"] for s in stages.values()]
    if FAIL in statuses or (strict and SKIP in statuses):
        return FAIL
    return PASS if PASS in statuses else SKIP


# --- running ------------------------------------------------------------------


def child_env() -> dict:
    env = dict(os.environ)
    env["LIGHTLOOM_HAL"] = "mock"
    return env


def run(cmd: list[str], timeout: float) -> tuple[int, str, str]:
    """Exit code, stdout and stderr of `cmd` (124 on timeout)."""
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                           timeout=timeout, env=child_env())
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        out = e.stdout.decode("utf-8", "replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
        err = e.stderr.decode("utf-8", "replace") if isinstance(e.stderr, bytes) else (e.stderr or "")
        return 124, out, err + f"\n(timed out after {timeout:g} s)"


def tail(text: str, n: int = 12) -> list[str]:
    return [line for line in text.strip().splitlines()[-n:] if line.strip()]


def corpus_dirs(args) -> list[str]:
    dirs = list(args.corpus)
    env = os.environ.get("LIGHTLOOM_CONFORMANCE_CORPUS", "")
    dirs += [d for d in env.split(os.pathsep) if d]
    if not dirs and os.path.isdir(DEFAULT_CORPUS):
        # tests/conformance/corpus/ itself, or one corpus per subdirectory.
        subs = sorted(os.path.join(DEFAULT_CORPUS, d) for d in os.listdir(DEFAULT_CORPUS)
                      if os.path.isdir(os.path.join(DEFAULT_CORPUS, d)))
        dirs = subs or [DEFAULT_CORPUS]
    return dirs


def stage_dryrun(args, out: str) -> dict:
    dirs = corpus_dirs(args)
    if not dirs:
        return {"status": SKIP, "reason": "no plugin corpus (--corpus, LIGHTLOOM_CONFORMANCE_CORPUS "
                                          "or tests/conformance/corpus/)", "corpora": []}
    corpora = []
    for i, d in enumerate(dirs):
        entry = {"dir": d}
        if not os.path.isdir(d):
            entry.update(status=FAIL, problems=[f"{d} is not a directory"])
            corpora.append(entry)
            continue
        report = os.path.join(out, f"dryrun-{i}.json")
        cmd = [args.lightloom, "compat", "dry-run", d, "--initialize", "--json", report]
        if args.jobs:
            cmd += ["--jobs", str(args.jobs)]
        code, stdout, stderr = run(cmd, args.timeout)
        entry["command"] = cmd
        entry["report"] = report
        if code != 0 or not os.path.isfile(report):
            entry.update(status=FAIL, problems=[f"lightloom compat dry-run exited {code}"],
                         output=tail(stdout + "\n" + stderr))
        else:
            with open(report, encoding="utf-8") as f:
                summary = json.load(f)["summary"]
            entry.update(judge_dryrun(summary, args.parse_threshold, args.initialize_threshold))
        corpora.append(entry)
    status = FAIL if any(c["status"] == FAIL for c in corpora) else PASS
    return {"status": status, "corpora": corpora,
            "thresholds": {"parse_pct": args.parse_threshold, "initialize_pct": args.initialize_threshold}}


def stage_replay(args, out: str) -> dict:
    roots = sorted(glob.glob(os.path.join(REPO, "plugins", "*", "fixtures"))) + list(args.traces)
    for t in args.traces:
        if not os.path.isdir(t):
            return {"status": FAIL, "problems": [f"--traces {t} is not a directory"], "traces": []}
    traces, others = find_traces(roots)
    results = []
    for i, path in enumerate(traces):
        replayed = os.path.join(out, f"replay-{i}.json")
        cmd = [args.lightloom, "compat", "replay", path, "--no-plugin-cache", "--out", replayed]
        for d in args.plugins or [os.path.join(REPO, "plugins")]:
            cmd += ["--plugins", d]
        if args.signalrgb:
            cmd.append("--signalrgb")
        code, stdout, stderr = run(cmd, args.timeout)
        results.append({
            "trace": os.path.relpath(path, REPO) if path.startswith(REPO) else path,
            "status": PASS if code == 0 else FAIL,
            "exit": code,
            "output": tail(stdout + "\n" + stderr, 20 if code else 6),
        })
    problems = [f"{r['trace']}: exit {r['exit']}" for r in results if r["status"] == FAIL]
    if not traces and args.require_traces:
        problems.append("no recorded trace found")
    if not traces and not problems:
        status = SKIP
    else:
        status = FAIL if problems else PASS
    return {
        "status": status,
        "reason": "no recorded trace under plugins/*/fixtures or --traces" if status == SKIP else None,
        "problems": problems,
        "traces": results,
        "skipped_fixtures": len(others),
    }


def stage_psnr(args, out: str) -> dict:
    if not (args.renderer and args.chrome):
        return {"status": SKIP, "reason": "--renderer and --chrome not given"}
    effects = args.effect or sorted(glob.glob(os.path.join(REPO, "effects", "*.html")))
    if not effects:
        return {"status": FAIL, "problems": ["no effect to compare"]}
    psnr_out = os.path.join(out, "psnr")
    cmd = [sys.executable, "-I", PSNR_SCRIPT, args.renderer, args.chrome, psnr_out, *effects]
    code, stdout, stderr = run(cmd, args.timeout + 240 * len(effects))
    result_file = os.path.join(psnr_out, "psnr.json")
    if code != 0 or not os.path.isfile(result_file):
        return {"status": FAIL, "problems": [f"psnr.py exited {code}"], "output": tail(stdout + "\n" + stderr)}
    with open(result_file, encoding="utf-8") as f:
        rows = json.load(f)
    verdict = judge_psnr(rows, args.psnr_threshold)
    verdict.update(threshold_db=args.psnr_threshold, report=result_file, effects=len(effects))
    return verdict


# --- reporting ----------------------------------------------------------------


def fmt_pct(v) -> str:
    return "-" if v is None else f"{v:.1f} %"


def markdown(report: dict) -> str:
    lines = [f"# Lightloom conformance: {report['status'].upper()}", "",
             f"{report['started']}, `{report['lightloom']}`", ""]
    st = report["stages"]
    if "dryrun" in st:
        d = st["dryrun"]
        lines += [f"## Corpus dry-run: {d['status']}", ""]
        if d["status"] == SKIP:
            lines += [d["reason"], ""]
        for c in d.get("corpora", []):
            lines += [f"### `{c['dir']}`: {c['status']}", ""]
            if "files" in c:
                lines += [
                    "| files | parsed | plugins | Initialize ok |",
                    "|---|---|---|---|",
                    f"| {c['files']} | {c['parsed']} ({fmt_pct(c['parse_pct'])}) | {c['plugins']} | "
                    f"{c['initialize_ok']}/{c['initialize_called']} ({fmt_pct(c['initialize_pct'])}) |",
                    "",
                ]
            lines += [f"- {p}" for p in c.get("problems", [])]
            if c.get("api_gaps"):
                lines += ["", "API gaps (files hitting them):", ""]
                lines += [f"- `{g}`: {n}" for g, n in c["api_gaps"]]
            if c.get("initialize_error_classes"):
                lines += ["", "Initialize() failures by class:", ""]
                lines += [f"- {cls}: {n}" for cls, n in c["initialize_error_classes"]]
            lines += [f"    {o}" for o in c.get("output", [])]
            lines.append("")
    if "replay" in st:
        r = st["replay"]
        lines += [f"## Trace replay: {r['status']}", ""]
        if r.get("reason"):
            lines += [r["reason"], ""]
        for t in r.get("traces", []):
            lines.append(f"- `{t['trace']}`: {t['status']}")
            if t["status"] == FAIL:
                lines += [f"    {o}" for o in t["output"]]
        if not r.get("traces") and r.get("problems"):
            lines += [f"- {p}" for p in r["problems"]]
        lines += ["", f"{r.get('skipped_fixtures', 0)} other fixture files (packet expectations) skipped.", ""]
    if "psnr" in st:
        p = st["psnr"]
        lines += [f"## Effects PSNR: {p['status']}", ""]
        if p.get("reason"):
            lines += [p["reason"], ""]
        if "comparisons" in p:
            lines += [f"{p['comparisons']} comparisons, threshold {p['threshold_db']:g} dB.", ""]
        lines += [f"- {x}" for x in p.get("problems", []) + p.get("below", [])]
        lines += [f"    {o}" for o in p.get("output", [])]
        lines.append("")
    text = "\n".join(lines).rstrip() + "\n"
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    return text


def find_lightloom(explicit: str | None) -> str | None:
    if explicit:
        return explicit
    if os.environ.get("LIGHTLOOM_BIN"):
        return os.environ["LIGHTLOOM_BIN"]
    exe = "lightloom.exe" if os.name == "nt" else "lightloom"
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
    for profile in ("release", "debug"):
        candidate = os.path.join(target, profile, exe)
        if os.path.isfile(candidate):
            return candidate
    return shutil.which("lightloom")


def parse_args(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lightloom", help="the lightloom CLI (default: $LIGHTLOOM_BIN, "
                                      "$CARGO_TARGET_DIR or ./target release/debug, PATH)")
    ap.add_argument("--only", default=",".join(STAGES),
                    help=f"comma-separated stages to run ({', '.join(STAGES)})")
    ap.add_argument("--out", help="report directory (default: a new temp dir)")
    ap.add_argument("--strict", action="store_true", help="a skipped stage fails the run")
    ap.add_argument("--timeout", type=float, default=1800, help="seconds per lightloom invocation")
    g = ap.add_argument_group("dryrun")
    g.add_argument("--corpus", action="append", default=[], metavar="DIR",
                   help="plugin corpus directory (repeatable)")
    g.add_argument("--parse-threshold", type=float, default=100.0, metavar="PCT")
    g.add_argument("--initialize-threshold", type=float, default=95.0, metavar="PCT")
    g.add_argument("--jobs", type=int, help="dry-run worker threads")
    g = ap.add_argument_group("replay")
    g.add_argument("--traces", action="append", default=[], metavar="DIR",
                   help="more directories of recorded traces (repeatable)")
    g.add_argument("--plugins", action="append", default=[], metavar="DIR",
                   help="plugin directories for the replay (default: ./plugins)")
    g.add_argument("--signalrgb", action="store_true",
                   help="replay with the local SignalRGB plugins too (read in place)")
    g.add_argument("--require-traces", action="store_true", help="fail when no trace is found")
    g = ap.add_argument_group("psnr")
    g.add_argument("--renderer", help="lightloom-renderer built with the cef feature")
    g.add_argument("--chrome", help="stock Chrome / Chromium to compare with")
    g.add_argument("--effect", action="append", default=[], metavar="HTML",
                   help="effect to compare (repeatable; default effects/*.html)")
    g.add_argument("--psnr-threshold", type=float, default=40.0, metavar="DB")
    args = ap.parse_args(argv)
    args.stages = [s.strip() for s in args.only.split(",") if s.strip()]
    unknown = [s for s in args.stages if s not in STAGES]
    if unknown:
        ap.error(f"unknown stage(s): {', '.join(unknown)}")
    return args


def main(argv=None) -> int:
    args = parse_args(argv)
    needs_cli = any(s in args.stages for s in ("dryrun", "replay"))
    args.lightloom = find_lightloom(args.lightloom)
    if args.lightloom and not os.path.isfile(args.lightloom):
        args.lightloom = shutil.which(args.lightloom)
    if needs_cli and not args.lightloom:
        print("conformance: the lightloom CLI was not found (--lightloom or LIGHTLOOM_BIN)", file=sys.stderr)
        return 2
    out = args.out or tempfile.mkdtemp(prefix="lightloom-conformance-")
    os.makedirs(out, exist_ok=True)
    report = {
        "started": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
        "lightloom": args.lightloom,
        "stages": {},
    }
    runners = {"dryrun": stage_dryrun, "replay": stage_replay, "psnr": stage_psnr}
    for name in STAGES:
        if name in args.stages:
            print(f"conformance: {name} ...", flush=True)
            report["stages"][name] = runners[name](args, out)
            print(f"conformance: {name}: {report['stages'][name]['status']}", flush=True)
    report["status"] = overall(report["stages"], args.strict)
    with open(os.path.join(out, "conformance.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(report, f, indent=1, default=str)
        f.write("\n")
    md = markdown(report)
    with open(os.path.join(out, "conformance.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write(md)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8", newline="\n") as f:
            f.write(md)
    print(md)
    print(f"conformance: {report['status']} (report in {out})")
    return 1 if report["status"] == FAIL else 0


if __name__ == "__main__":
    sys.exit(main())
