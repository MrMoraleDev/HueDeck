# SPDX-License-Identifier: GPL-3.0-or-later
"""Unit tests of the conformance runner's verdicts (no lightloom binary needed).

    python -I -m unittest discover -s tests/conformance -p "test_*.py"
"""

import json
import math
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run  # noqa: E402

SUMMARY = {
    "files": 200,
    "parsed": 200,
    "plugins": 180,
    "initialize_called": 180,
    "initialize_ok": 175,
    "top_api_gaps": [["device.fancy", 3]],
    "initialize_error_classes": [["TypeError", 5]],
    "top_errors": [["boom", 2]],
}


class DryRunVerdict(unittest.TestCase):
    def test_passes_at_the_thresholds_and_reports_gaps(self):
        v = run.judge_dryrun(SUMMARY, 100, 95)
        self.assertEqual(v["status"], run.PASS, v)
        self.assertAlmostEqual(v["initialize_pct"], 97.22, places=2)
        self.assertEqual(v["api_gaps"], [["device.fancy", 3]])

    def test_one_unparsed_file_fails_the_100_percent_parse(self):
        v = run.judge_dryrun(dict(SUMMARY, parsed=199), 100, 95)
        self.assertEqual(v["status"], run.FAIL)
        self.assertIn("parse", v["problems"][0])

    def test_initialize_below_95_percent_fails(self):
        v = run.judge_dryrun(dict(SUMMARY, initialize_ok=170), 100, 95)
        self.assertEqual(v["status"], run.FAIL)
        self.assertIn("Initialize", v["problems"][0])
        # Exactly 95 % passes.
        self.assertEqual(run.judge_dryrun(dict(SUMMARY, initialize_ok=171), 100, 95)["status"], run.PASS)

    def test_an_empty_corpus_fails(self):
        self.assertEqual(run.judge_dryrun({"files": 0}, 100, 95)["status"], run.FAIL)


class Traces(unittest.TestCase):
    def test_only_hid_traces_are_replayed(self):
        trace = {"version": 1, "endpoints": [], "events": [], "canvas": "#ff0000"}
        self.assertTrue(run.is_trace(trace))
        self.assertFalse(run.is_trace({"source": "x", "frame": []}))
        self.assertFalse(run.is_trace({"version": True, "endpoints": [], "events": []}))
        self.assertFalse(run.is_trace([1, 2]))
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "fixtures", "sub"))
            for name, doc in (("a.json", trace), ("sub/b.json", {"frame": []})):
                with open(os.path.join(d, "fixtures", name), "w", encoding="utf-8") as f:
                    json.dump(doc, f)
            with open(os.path.join(d, "fixtures", "broken.json"), "w", encoding="utf-8") as f:
                f.write("{")
            traces, others = run.find_traces([os.path.join(d, "fixtures")])
        self.assertEqual([os.path.basename(t) for t in traces], ["a.json"])
        self.assertEqual(sorted(os.path.basename(o) for o in others), ["b.json", "broken.json"])


class Psnr(unittest.TestCase):
    def test_threshold_and_missing_frames(self):
        rows = [
            {"effect": "a.html", "mode": "sw", "psnr_db": math.inf},
            {"effect": "a.html", "mode": "gpu", "psnr_db": 41.0},
        ]
        self.assertEqual(run.judge_psnr(rows, 40)["status"], run.PASS)
        rows.append({"effect": "b.html", "mode": "gpu", "psnr_db": 39.9, "note": "12 px differ"})
        rows.append({"effect": "c.html", "mode": "sw", "psnr_db": None, "note": "missing frame"})
        v = run.judge_psnr(rows, 40)
        self.assertEqual(v["status"], run.FAIL)
        self.assertEqual(len(v["below"]), 2)
        self.assertEqual(run.judge_psnr([], 40)["status"], run.FAIL)


class Overall(unittest.TestCase):
    def test_skips_fail_only_when_strict(self):
        stages = {"dryrun": {"status": run.PASS}, "psnr": {"status": run.SKIP}}
        self.assertEqual(run.overall(stages, False), run.PASS)
        self.assertEqual(run.overall(stages, True), run.FAIL)
        self.assertEqual(run.overall({"psnr": {"status": run.SKIP}}, False), run.SKIP)
        self.assertEqual(run.overall({"a": {"status": run.PASS}, "b": {"status": run.FAIL}}, False), run.FAIL)


if __name__ == "__main__":
    unittest.main()
