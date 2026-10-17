#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Validate HueDeck JSON Schemas and the user's local SignalRGB component corpus.

Run with an isolated interpreter (stdlib only, no third-party validator):

    python -I tests/schemas/validate_corpus.py [--skip-if-missing-corpus] [--verbose] [ROOT ...]

What it does
------------
1. Parses every ``schemas/*.schema.json`` as JSON and checks the draft 2020-12
   boilerplate (``$schema``, ``$id``, SPDX ``$comment``) plus that every
   ``pattern`` compiles.
2. Mirrors ``schemas/component.schema.json`` with explicit structural checks
   (required keys, value types, bounds) and cross-checks that the hard-coded
   mirror still matches the schema's ``required``/``properties`` lists, so the
   two cannot silently drift apart.
3. Runs the mirror against built-in fixtures (one valid placeholder strip,
   several invalid documents) so a broken checker fails loudly.
4. Walks the corpus roots recursively, picks every ``*.json`` whose top level is
   an object with a ``ProductName`` key, validates it, and prints pass/fail
   counts plus the distinct keys, ``Type`` and ``Brand`` values seen.

The real corpus is the ground truth: hard schema violations are FAILURES;
invariants that JSON Schema cannot express (equal array lengths, coordinates
inside Width x Height, LedMapping being a permutation, duplicate ProductName
between files, whitespace padding) are reported as WARNINGS because shipped
files violate some of them and the loader must tolerate them.

Exit status: 0 when every corpus file passes (or the corpus is absent and
``--skip-if-missing-corpus`` was given), 1 otherwise.
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, os.pardir, os.pardir))
SCHEMAS_DIR = os.path.join(REPO, "schemas")
COMPONENT_SCHEMA = os.path.join(SCHEMAS_DIR, "component.schema.json")

DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema"
SPDX_COMMENT = "SPDX-License-Identifier: Apache-2.0 OR MIT"

LOCALAPPDATA = os.environ.get("LOCALAPPDATA") or os.path.join(
    os.path.expanduser("~"), "AppData", "Local"
)
DEFAULT_ROOTS = [
    os.path.join(LOCALAPPDATA, "VortxEngine", "app-2.5.80", "Signal-x64", "Components"),
    os.path.join(LOCALAPPDATA, "WhirlwindFX", "SignalRgb", "cache", "plugin_cdn", "main", "Components"),
    os.path.join(LOCALAPPDATA, "WhirlwindFX", "SignalRgb", "cache", "addons"),
]

# --- mirror of schemas/component.schema.json ---------------------------------

REQUIRED_KEYS = (
    "ProductName",
    "DisplayName",
    "Brand",
    "Type",
    "LedCount",
    "Width",
    "Height",
    "LedMapping",
    "LedCoordinates",
    "LedNames",
)
OPTIONAL_KEYS = ("ImageUrl", "Image", "ComponentId", "Custom")
KNOWN_KEYS = REQUIRED_KEYS + OPTIONAL_KEYS

NON_BLANK_STRINGS = ("ProductName", "DisplayName", "Brand")  # pattern "\S"
POSITIVE_INTS = ("LedCount", "Width", "Height")  # integer, minimum 1

IMAGE_URL_RE = re.compile(r"^(https?://|file://|/|\./|[A-Za-z]:[\\/]).+")
IMAGE_B64_RE = re.compile(r"^[A-Za-z0-9+/=\s]+$")
COMPONENT_ID_RE = re.compile(
    r"^\{?[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\}?$"
)


def is_int(value) -> bool:
    """JSON integer: Python int but never bool."""
    return isinstance(value, int) and not isinstance(value, bool)


def is_number(value) -> bool:
    return (isinstance(value, (int, float))) and not isinstance(value, bool)


def check_component(doc) -> tuple[list[str], list[str]]:
    """Return (errors, warnings) for one component document.

    ``errors`` mirror what the JSON Schema rejects; ``warnings`` are the
    documented invariants the schema cannot express.
    """
    errors: list[str] = []
    warnings: list[str] = []

    if not isinstance(doc, dict):
        return ["top level is not an object"], warnings

    for key in REQUIRED_KEYS:
        if key not in doc:
            errors.append(f"missing required key {key!r}")

    for key in NON_BLANK_STRINGS:
        if key in doc:
            value = doc[key]
            if not isinstance(value, str):
                errors.append(f"{key} is not a string")
            elif len(value) < 1 or not re.search(r"\S", value):
                errors.append(f"{key} is blank")
            elif value != value.strip():
                warnings.append(f"{key} has leading/trailing whitespace: {value!r}")

    if "Type" in doc:
        value = doc["Type"]
        if not isinstance(value, str):
            errors.append("Type is not a string")
        elif len(value) < 1:
            errors.append("Type is empty")
        elif value != value.strip():
            warnings.append(f"Type has leading/trailing whitespace: {value!r}")

    for key in POSITIVE_INTS:
        if key in doc:
            value = doc[key]
            if not is_int(value):
                errors.append(f"{key} is not an integer (got {type(value).__name__})")
            elif value < 1:
                errors.append(f"{key} must be >= 1 (got {value})")

    mapping = doc.get("LedMapping")
    if "LedMapping" in doc:
        if not isinstance(mapping, list):
            errors.append("LedMapping is not an array")
        elif len(mapping) < 1:
            errors.append("LedMapping is empty")
        else:
            for i, slot in enumerate(mapping):
                if not is_int(slot):
                    errors.append(f"LedMapping[{i}] is not an integer")
                    break
                if slot < 0:
                    errors.append(f"LedMapping[{i}] is negative")
                    break

    coords = doc.get("LedCoordinates")
    if "LedCoordinates" in doc:
        if not isinstance(coords, list):
            errors.append("LedCoordinates is not an array")
        elif len(coords) < 1:
            errors.append("LedCoordinates is empty")
        else:
            for i, pair in enumerate(coords):
                if not (isinstance(pair, list) and len(pair) == 2):
                    errors.append(f"LedCoordinates[{i}] is not a 2-element array")
                    break
                if not all(is_int(v) for v in pair):
                    errors.append(f"LedCoordinates[{i}] has a non-integer element")
                    break
                if pair[0] < 0 or pair[1] < 0:
                    errors.append(f"LedCoordinates[{i}] has a negative element")
                    break

    names = doc.get("LedNames")
    if "LedNames" in doc:
        if not isinstance(names, list):
            errors.append("LedNames is not an array")
        elif len(names) < 1:
            errors.append("LedNames is empty")
        else:
            for i, name in enumerate(names):
                if not isinstance(name, str):
                    errors.append(f"LedNames[{i}] is not a string")
                    break
                if len(name) < 1:
                    errors.append(f"LedNames[{i}] is empty")
                    break

    if "ImageUrl" in doc:
        value = doc["ImageUrl"]
        if not isinstance(value, str):
            errors.append("ImageUrl is not a string")
        elif not IMAGE_URL_RE.match(value):
            errors.append(f"ImageUrl does not look like a URL or path: {value[:60]!r}")

    if "Image" in doc:
        value = doc["Image"]
        if not isinstance(value, str):
            errors.append("Image is not a string")
        elif len(value) < 1:
            errors.append("Image is empty")
        elif not IMAGE_B64_RE.match(value):
            errors.append("Image is not base64 text (data: prefix or illegal characters)")

    if "ComponentId" in doc:
        value = doc["ComponentId"]
        if not isinstance(value, str) or not COMPONENT_ID_RE.match(value):
            errors.append("ComponentId is not a UUID string")

    if "Custom" in doc and not isinstance(doc["Custom"], bool):
        errors.append("Custom is not a boolean")

    # --- invariants the schema cannot express (warnings) ---
    unknown = sorted(k for k in doc if k not in KNOWN_KEYS)
    if unknown:
        warnings.append(f"unknown keys (ignored by the loader): {unknown}")

    if "ImageUrl" not in doc and "Image" not in doc:
        warnings.append("neither ImageUrl nor Image present (placeholder image will be used)")

    led_count = doc.get("LedCount")
    if is_int(led_count) and led_count >= 1:
        lengths = {
            "LedMapping": len(mapping) if isinstance(mapping, list) else None,
            "LedCoordinates": len(coords) if isinstance(coords, list) else None,
            "LedNames": len(names) if isinstance(names, list) else None,
        }
        mismatched = {k: v for k, v in lengths.items() if v is not None and v != led_count}
        if mismatched:
            warnings.append(f"array lengths differ from LedCount={led_count}: {mismatched}")

    width, height = doc.get("Width"), doc.get("Height")
    if is_int(width) and is_int(height) and isinstance(coords, list):
        for i, pair in enumerate(coords):
            if isinstance(pair, list) and len(pair) == 2 and all(is_int(v) for v in pair):
                if not (0 <= pair[0] < width and 0 <= pair[1] < height):
                    warnings.append(
                        f"LedCoordinates[{i}]={pair} outside Width x Height = {width}x{height}"
                    )
                    break

    if isinstance(mapping, list) and all(is_int(v) for v in mapping) and mapping:
        expected = list(range(len(mapping)))
        if sorted(mapping) != expected:
            dupes = sorted({v for v in mapping if mapping.count(v) > 1})
            out_of_range = sorted({v for v in mapping if v >= len(mapping)})
            warnings.append(
                "LedMapping is not a permutation of 0..N-1"
                f" (duplicates {dupes}, out of range {out_of_range})"
            )

    return errors, warnings


# --- schema files ------------------------------------------------------------


def iter_patterns(node, path="$"):
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "pattern" and isinstance(value, str):
                yield f"{path}.pattern", value
            else:
                yield from iter_patterns(value, f"{path}.{key}")
    elif isinstance(node, list):
        for i, value in enumerate(node):
            yield from iter_patterns(value, f"{path}[{i}]")


def check_schema_files(verbose: bool) -> tuple[int, list[str]]:
    """Parse every schema and return (count, problems)."""
    problems: list[str] = []
    files = sorted(
        f for f in os.listdir(SCHEMAS_DIR) if f.endswith(".schema.json")
    )
    if not files:
        problems.append(f"no *.schema.json files found in {SCHEMAS_DIR}")
    for name in files:
        path = os.path.join(SCHEMAS_DIR, name)
        with open(path, "rb") as fh:
            raw = fh.read()
        if raw.startswith(b"\xef\xbb\xbf"):
            problems.append(f"{name}: has a UTF-8 BOM")
        if b"\r\n" in raw:
            problems.append(f"{name}: has CRLF line endings")
        try:
            schema = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            problems.append(f"{name}: not valid JSON: {exc}")
            continue
        if not isinstance(schema, dict):
            problems.append(f"{name}: top level is not an object")
            continue
        if schema.get("$schema") != DRAFT_2020_12:
            problems.append(f"{name}: $schema is not {DRAFT_2020_12}")
        if not isinstance(schema.get("$id"), str) or not schema["$id"]:
            problems.append(f"{name}: missing $id")
        if schema.get("$comment") != SPDX_COMMENT:
            problems.append(f"{name}: $comment is not the SPDX header")
        if "title" not in schema or "description" not in schema:
            problems.append(f"{name}: missing title/description")
        for where, pattern in iter_patterns(schema):
            try:
                re.compile(pattern)
            except re.error as exc:
                problems.append(f"{name}: {where} does not compile under Python re: {exc}")
        if verbose:
            print(f"  schema ok: {name}")
    return len(files), problems


def check_mirror_matches_schema() -> list[str]:
    """Ensure the hard-coded mirror still agrees with component.schema.json."""
    problems: list[str] = []
    with open(COMPONENT_SCHEMA, "r", encoding="utf-8") as fh:
        schema = json.load(fh)
    required = tuple(schema.get("required", []))
    if required != REQUIRED_KEYS:
        problems.append(
            f"component schema 'required' {list(required)} != mirror {list(REQUIRED_KEYS)}"
        )
    props = set(schema.get("properties", {}))
    if props != set(KNOWN_KEYS):
        problems.append(
            f"component schema properties {sorted(props)} != mirror {sorted(KNOWN_KEYS)}"
        )
    for key in POSITIVE_INTS:
        spec = schema["properties"].get(key, {})
        if spec.get("type") != "integer" or spec.get("minimum") != 1:
            problems.append(f"component schema {key} is not integer/minimum 1 as the mirror expects")
    for key in NON_BLANK_STRINGS:
        spec = schema["properties"].get(key, {})
        if spec.get("pattern") != "\\S":
            problems.append(f"component schema {key} pattern is not \\S as the mirror expects")
    if schema["properties"]["ImageUrl"].get("pattern") != IMAGE_URL_RE.pattern:
        problems.append("component schema ImageUrl pattern differs from mirror")
    if schema["properties"]["Image"].get("pattern") != IMAGE_B64_RE.pattern:
        problems.append("component schema Image pattern differs from mirror")
    if schema["properties"]["ComponentId"].get("pattern") != COMPONENT_ID_RE.pattern:
        problems.append("component schema ComponentId pattern differs from mirror")
    for example in schema.get("examples", []):
        errors, _ = check_component(example)
        if errors:
            problems.append(f"component schema example fails the mirror: {errors}")
    return problems


# --- fixtures ----------------------------------------------------------------


def placeholder_strip(n: int = 4) -> dict:
    return {
        "ProductName": f"Default Strip - {n}",
        "DisplayName": f"Default Strip - {n}",
        "Brand": "Custom",
        "Type": "Strip",
        "LedCount": n,
        "Width": n,
        "Height": 1,
        "LedMapping": list(range(n)),
        "LedCoordinates": [[i, 0] for i in range(n)],
        "LedNames": [f"Led {i + 1}" for i in range(n)],
        "ImageUrl": "https://assets.signalrgb.com/devices/default/misc/led-strip.png",
    }


def self_test() -> list[str]:
    problems: list[str] = []

    def expect(name, doc, *, errors: bool, warn_substr: str | None = None):
        errs, warns = check_component(doc)
        if bool(errs) != errors:
            problems.append(f"fixture {name}: expected errors={errors}, got {errs}")
        if warn_substr is not None and not any(warn_substr in w for w in warns):
            problems.append(f"fixture {name}: expected warning containing {warn_substr!r}, got {warns}")

    expect("valid placeholder", placeholder_strip(), errors=False)
    inst = dict(placeholder_strip(), ComponentId="8d3978fc-1b2a-4c7e-9f00-123456789abc")
    expect("valid instance", inst, errors=False)
    img = dict(placeholder_strip())
    del img["ImageUrl"]
    img["Image"] = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
    expect("valid inline image", img, errors=False)

    expect("not an object", [], errors=True)
    missing = dict(placeholder_strip())
    del missing["LedNames"]
    expect("missing LedNames", missing, errors=True)
    expect("LedCount zero", dict(placeholder_strip(), LedCount=0), errors=True)
    expect("LedCount string", dict(placeholder_strip(), LedCount="4"), errors=True)
    expect("LedCount bool", dict(placeholder_strip(), LedCount=True), errors=True)
    expect("blank ProductName", dict(placeholder_strip(), ProductName="   "), errors=True)
    expect("bad coordinate tuple", dict(placeholder_strip(), LedCoordinates=[[0], [1, 0], [2, 0], [3, 0]]), errors=True)
    expect("negative mapping", dict(placeholder_strip(), LedMapping=[-1, 1, 2, 3]), errors=True)
    expect("empty LedName", dict(placeholder_strip(), LedNames=["", "b", "c", "d"]), errors=True)
    expect("bad ImageUrl", dict(placeholder_strip(), ImageUrl="not a url"), errors=True)
    expect("data: Image", dict(placeholder_strip(), Image="data:image/png;base64,AAAA"), errors=True)
    expect("bad ComponentId", dict(placeholder_strip(), ComponentId="nope"), errors=True)

    expect("length mismatch", dict(placeholder_strip(), LedNames=["a", "b"]), errors=False, warn_substr="array lengths differ")
    expect("coordinate out of bounds", dict(placeholder_strip(), Width=2), errors=False, warn_substr="outside Width x Height")
    expect("mapping duplicates", dict(placeholder_strip(), LedMapping=[0, 1, 3, 3]), errors=False, warn_substr="not a permutation")
    expect("padded ProductName", dict(placeholder_strip(), ProductName="Strip "), errors=False, warn_substr="whitespace")
    expect("unknown key", dict(placeholder_strip(), Extra=1), errors=False, warn_substr="unknown keys")
    return problems


# --- corpus walk -------------------------------------------------------------


def iter_component_files(roots):
    for root in roots:
        if not os.path.isdir(root):
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames.sort()
            for filename in sorted(filenames):
                if filename.lower().endswith(".json"):
                    yield root, os.path.join(dirpath, filename)


def load_json_file(path):
    with open(path, "rb") as fh:
        raw = fh.read()
    bom = raw.startswith(b"\xef\xbb\xbf")
    return json.loads(raw.decode("utf-8-sig")), bom


def run_corpus(roots, verbose: bool):
    passed = failed = skipped = unparsable = 0
    keys = collections.Counter()
    types = collections.Counter()
    brands = collections.Counter()
    per_root = collections.Counter()
    warnings_total = 0
    failures: list[tuple[str, list[str]]] = []
    warned: list[tuple[str, list[str]]] = []
    product_names: dict[str, list[str]] = collections.defaultdict(list)

    for root, path in iter_component_files(roots):
        rel = os.path.relpath(path, root)
        try:
            doc, bom = load_json_file(path)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            unparsable += 1
            failures.append((path, [f"not valid JSON: {exc}"]))
            continue
        if not (isinstance(doc, dict) and "ProductName" in doc):
            skipped += 1
            continue
        errors, warnings = check_component(doc)
        if bom:
            warnings.append("file starts with a UTF-8 BOM")
        for key in doc:
            keys[key] += 1
        types[str(doc.get("Type"))] += 1
        brands[str(doc.get("Brand"))] += 1
        per_root[root] += 1
        if isinstance(doc.get("ProductName"), str):
            product_names[doc["ProductName"].strip()].append(path)
        if errors:
            failed += 1
            failures.append((path, errors))
        else:
            passed += 1
        if warnings:
            warnings_total += len(warnings)
            warned.append((path, warnings))
        if verbose:
            status = "FAIL" if errors else "ok"
            print(f"  [{status}] {rel}" + (f"  warnings={len(warnings)}" if warnings else ""))

    duplicates = {name: paths for name, paths in product_names.items() if len(paths) > 1}
    return {
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "unparsable": unparsable,
        "keys": keys,
        "types": types,
        "brands": brands,
        "per_root": per_root,
        "warnings_total": warnings_total,
        "failures": failures,
        "warned": warned,
        "duplicates": duplicates,
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("roots", nargs="*", help="corpus roots (default: the local SignalRGB component directories)")
    parser.add_argument("--skip-if-missing-corpus", action="store_true",
                        help="exit 0 (after the schema checks) when none of the corpus roots exist")
    parser.add_argument("--verbose", "-v", action="store_true", help="print one line per file")
    args = parser.parse_args(argv)

    ok = True

    print("== schema files")
    count, problems = check_schema_files(args.verbose)
    print(f"  parsed {count} schema file(s), {len(problems)} problem(s)")
    for p in problems:
        print(f"  ERROR {p}")
    ok &= not problems

    print("== component mirror vs schema")
    problems = check_mirror_matches_schema()
    print(f"  {len(problems)} problem(s)")
    for p in problems:
        print(f"  ERROR {p}")
    ok &= not problems

    print("== self-test fixtures")
    problems = self_test()
    print(f"  {len(problems)} problem(s)")
    for p in problems:
        print(f"  ERROR {p}")
    ok &= not problems

    roots = args.roots or DEFAULT_ROOTS
    present = [r for r in roots if os.path.isdir(r)]
    print("== corpus roots")
    for r in roots:
        print(f"  {'found  ' if os.path.isdir(r) else 'missing'} {r}")
    if not present:
        if args.skip_if_missing_corpus:
            print("  no corpus present; skipping corpus validation (--skip-if-missing-corpus)")
            return 0 if ok else 1
        print("  ERROR no corpus root exists (pass roots on the command line or --skip-if-missing-corpus)")
        return 1

    result = run_corpus(present, args.verbose)
    print("== corpus results")
    print(f"  passed={result['passed']} failed={result['failed']} unparsable={result['unparsable']} "
          f"skipped(no ProductName)={result['skipped']} warnings={result['warnings_total']}")
    for root, n in result["per_root"].items():
        print(f"  {n:4d} component file(s) under {root}")
    print(f"  distinct keys ({len(result['keys'])}): "
          + ", ".join(f"{k}={v}" for k, v in result["keys"].most_common()))
    print(f"  distinct Type values ({len(result['types'])}): "
          + ", ".join(f"{k!r}={v}" for k, v in result["types"].most_common()))
    print(f"  distinct Brand values ({len(result['brands'])}): "
          + ", ".join(f"{k!r}={v}" for k, v in result["brands"].most_common()))
    if result["duplicates"]:
        print(f"  WARNING {len(result['duplicates'])} ProductName(s) appear in more than one file "
              "(last scanned / user copy wins):")
        for name, paths in sorted(result["duplicates"].items()):
            print(f"    {name!r}:")
            for p in paths:
                print(f"      {p}")
    if result["warned"]:
        print(f"  WARNINGS in {len(result['warned'])} file(s):")
        for path, warnings in result["warned"]:
            for w in warnings:
                print(f"    {path}: {w}")
    if result["failures"]:
        print(f"  FAILURES in {len(result['failures'])} file(s):")
        for path, errors in result["failures"]:
            for e in errors:
                print(f"    {path}: {e}")
        ok = False

    print("== summary:", "OK" if ok else "FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
