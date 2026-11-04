"""Resolve BP's local binding and list a consistent, read-only project snapshot."""
import argparse
import json
import sqlite3
import sys
from pathlib import Path


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def local_path(value):
    # BP canonicalizes Windows bindings with extended-length prefixes. SQLite
    # file URIs require a drive path or a real UNC host, never the '?' device.
    if value.startswith("\\\\?\\UNC\\"):
        value = "\\\\" + value[8:]
    elif value.startswith("\\\\?\\"):
        value = value[4:]
    return Path(value)


def snapshot(binding_path, selectors=()):
    binding = read_json(Path(binding_path))
    if binding.get("product") != "BridgePilot":
        raise ValueError("binding is not owned by BridgePilot")
    home = local_path(binding["home"])
    workspace = local_path(read_json(home / ".runtime/config/bridgepilot.json")["workspace"])
    if not home.is_absolute() or not workspace.is_absolute():
        raise ValueError("home and workspace must be absolute")
    machine = read_json(workspace / ".runtime/config/bridgepilot-machine.json")["id"]
    db = workspace / "doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3"
    # mode=ro rejects a missing archive instead of creating an empty replacement.
    connection = sqlite3.connect(db.resolve().as_uri() + "?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        connection.execute("PRAGMA query_only=ON")
        connection.execute("BEGIN")
        meta = connection.execute("SELECT archive_id,revision FROM archive_meta WHERE singleton=1").fetchone()
        if meta is None:
            raise ValueError("archive metadata missing")
        projects = [dict(row) for row in connection.execute("SELECT id,name,repository,paused FROM projects ORDER BY name,id")]
        selected = []
        for selector in selectors:
            matches = [p for p in projects if p["id"] == selector or p["name"] == selector]
            if len(matches) != 1:
                raise ValueError("unknown or ambiguous project: " + selector)
            if matches[0] not in selected:
                selected.append(matches[0])
        if not selectors:
            selected = projects
        for project in selected:
            project["local_bindings"] = [dict(r) for r in connection.execute(
                "SELECT id,root,desktop_project_id,host FROM bindings WHERE project=? AND machine=? ORDER BY id",
                (project["id"], machine))]
            project["runs"] = [dict(r) for r in connection.execute(
                "SELECT r.id,r.state,r.machine,r.thread_id FROM runs r JOIN tasks t ON t.id=r.task WHERE t.project=? ORDER BY r.ordinal",
                (project["id"],))]
            project["operations"] = [dict(r) for r in connection.execute(
                "SELECT o.id,o.run,o.kind,o.state FROM operations o JOIN runs r ON r.id=o.run JOIN tasks t ON t.id=r.task WHERE t.project=? ORDER BY o.id",
                (project["id"],))]
        custody = connection.execute("SELECT epoch,owner,handoff_json FROM custody WHERE singleton=1").fetchone()
        return {"schema": 1, "read_only": True, "home": str(home), "workspace": str(workspace),
                "machine": machine, "archive": dict(meta), "custody": dict(custody) if custody else None,
                "projects": selected}
    finally:
        connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binding", type=Path, default=Path(__file__).resolve().parents[2] / "bridgepilot/bridgepilot-local.json")
    parser.add_argument("--project", action="append", default=[])
    args = parser.parse_args()
    try:
        result = snapshot(args.binding, args.project)
    except (OSError, ValueError, KeyError, sqlite3.Error) as error:
        print(json.dumps({"status": "blocked", "read_only": True, "error": str(error)}, ensure_ascii=False))
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    raise SystemExit(main())
