#!/usr/bin/env python3
"""Preflight: overlay every design sheet, list unfilled/unverified cells and unresolved references.

Usage: python3 tools/preflight.py [--milestone N]
Exit code 0 only when nothing is open for the chosen milestone (default: all).
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHEETS = ROOT / "sheets"

OPEN_MARKERS = ("TBD", "TODO", "?")


def load():
    out = {}
    for p in sorted(SHEETS.glob("*.json")):
        d = json.loads(p.read_text())
        out[d["sheet"]] = d
    return out


def is_open(v):
    if v is None or v == "" or v == []:
        return True
    if v is False:
        return True
    if isinstance(v, str) and any(v.startswith(m) or f" {m}" in v for m in OPEN_MARKERS):
        return True
    return False


def main():
    milestone = None
    if "--milestone" in sys.argv:
        milestone = int(sys.argv[sys.argv.index("--milestone") + 1])
    sheets = load()
    ids = {name: {r["id"] for r in s["rows"]} for name, s in sheets.items()}
    problems = []

    # Which systems are in scope
    systems = sheets["systems"]["rows"]
    in_scope = {r["id"] for r in systems if milestone is None or r["milestone"] <= milestone}

    # Hooks/protocol rows only matter if a system in scope uses them
    used_hooks = {h for r in systems if r["id"] in in_scope for h in r["hooks"]}
    used_proto = {p for r in systems if r["id"] in in_scope for p in r["protocol"]}

    for name, s in sheets.items():
        for row in s["rows"]:
            if name == "systems" and row["id"] not in in_scope:
                continue
            if name == "hooks" and row["id"] not in used_hooks:
                continue
            if name == "protocol" and row["id"] not in used_proto:
                continue
            if name == "weapons" and milestone is not None and milestone < 3:
                continue
            for col in s["columns"]:
                if col not in row:
                    problems.append(f"{name}.{row['id']}.{col}: missing column")
                    continue
                v = row[col]
                if col in ("notes", "reads", "writes", "hooks", "file", "installs_to", "license", "unit") and v in ("", []):
                    continue  # legitimately empty
                if is_open(v):
                    problems.append(f"{name}.{row['id']}.{col}: open ({v!r})")

    # Cross-sheet references
    for r in systems:
        if r["id"] not in in_scope:
            continue
        for h in r["hooks"]:
            if h not in ids["hooks"]:
                problems.append(f"systems.{r['id']}.hooks: '{h}' not in hooks sheet")
        for p in r["protocol"]:
            if p not in ids["protocol"]:
                problems.append(f"systems.{r['id']}.protocol: '{p}' not in protocol sheet")
    for p in sheets["protocol"]["rows"]:
        for u in p["used_by"]:
            if u not in ids["systems"]:
                problems.append(f"protocol.{p['id']}.used_by: '{u}' not in systems sheet")

    scope = f"milestone <= {milestone}" if milestone else "all milestones"
    print(f"Preflight ({scope}): {len(problems)} open")
    for line in problems:
        print("  -", line)
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main())
