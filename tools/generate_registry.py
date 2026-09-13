#!/usr/bin/env python3
"""Generate crates/rctpower-protocol/data/registry.csv from the vendored python-rctclient registry.

This is the update path: `git submodule update`, re-run this script, check the
diff, run `cargo test`. Data-only changes should not require Rust code edits.

Usage: python3 tools/generate_registry.py [--vendor vendor/python-rctclient]
"""

import argparse
import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ENTRY_RE = re.compile(r"ObjectInfo\((.*?)\)(?=\s*,?\s*\n\s*ObjectInfo\(|\Z)", re.S)
FIELD_RE = re.compile(r"(\w+)\s*=\s*(\"(?:[^\"\\]|\\.)*\"|\'(?:[^\'\\]|\\.)*\'|[^,\n]+?)(?=\s*(?:,\s*\w+\s*=|\Z))", re.S)
ENUM_RE = re.compile(r"\{(.*)\}", re.S)
ENUM_PAIR_RE = re.compile(r"(\d+)\s*:\s*[\"']([^\"']*)[\"']")

REPO = Path(__file__).resolve().parent.parent


def parse(raw: str):
    raw = raw.strip().rstrip(",")
    fields = {}
    for m in FIELD_RE.finditer(raw):
        key, val = m.group(1), m.group(2).strip()
        fields[key] = val
    return fields


def unq(s: str) -> str:
    s = s.strip()
    if len(s) >= 2 and s[0] == s[-1] and s[0] in "\"'":
        return s[1:-1]
    return s


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--vendor", default=str(REPO / "vendor" / "python-rctclient"))
    args = ap.parse_args()

    reg_py = Path(args.vendor) / "src" / "rctclient" / "registry.py"
    text = reg_py.read_text(encoding="utf-8")

    out = []
    for m in ENTRY_RE.finditer(text):
        raw = m.group(1)
        f = parse(raw)
        if "name" not in f or "object_id" not in f:
            continue
        enum_map = ""
        # enum_map is a dict value containing commas/newlines: grab it from the
        # raw entry text, not via FIELD_RE
        em = ENUM_RE.search(raw)
        if em:
            enum_map = "|".join(
                f"{k}={v}" for k, v in ENUM_PAIR_RE.findall(em.group(1))
            )
        out.append(
            {
                "group": unq(f["group"]).replace("ObjectGroup.", ""),
                "object_id": unq(f["object_id"]),
                "index": unq(f.get("index", "-1")),
                "name": unq(f["name"]),
                "req_type": unq(f["request_data_type"]).replace("DataType.", ""),
                "resp_type": unq(f.get("response_data_type", f["request_data_type"])).replace("DataType.", ""),
                "unit": unq(f.get("unit", "")),
                "description": unq(f.get("description", "")),
                "enum_map": enum_map,
            }
        )

    out.sort(key=lambda r: int(r["object_id"], 16))

    try:
        commit = subprocess.check_output(
            ["git", "-C", args.vendor, "rev-parse", "--short", "HEAD"], text=True
        ).strip()
    except Exception:
        commit = "unknown"

    data_dir = REPO / "crates" / "rctpower-protocol" / "data"
    data_dir.mkdir(exist_ok=True)
    with open(data_dir / "registry.csv", "w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fieldnames=list(out[0].keys()), delimiter=";")
        w.writeheader()
        w.writerows(out)

    meta = {
        "source": "python-rctclient registry.py",
        "source_commit": commit,
        "count": len(out),
    }
    (data_dir / "registry_meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"wrote {len(out)} entries -> crates/rctpower-protocol/data/registry.csv (commit {commit})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
