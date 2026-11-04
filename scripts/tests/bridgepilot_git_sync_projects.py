"""Read-only project listing against the Windows paths emitted by BP bindings."""
import importlib.util
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "bp_projects", ROOT / "source/orchestrator/skills/bridgepilot-git-sync/scripts/list_projects.py"
)
projects = importlib.util.module_from_spec(spec)
spec.loader.exec_module(projects)


@unittest.skipUnless(os.name == "nt", "BP uses Windows installation bindings")
class ProjectListTests(unittest.TestCase):
    def test_extended_paths_read_existing_archive_and_keep_selection_read_only(self):
        base = ROOT / ".runtime/tests/bridgepilot-git-sync-projects"
        base.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=base) as temporary:
            root = Path(temporary)
            home, workspace = root / "home", root / "workspace"
            (home / ".runtime/config").mkdir(parents=True)
            (workspace / ".runtime/config").mkdir(parents=True)
            (home / ".runtime/config/bridgepilot.json").write_text(
                json.dumps({"workspace": "\\\\?\\" + str(workspace)}), encoding="utf-8"
            )
            (workspace / ".runtime/config/bridgepilot-machine.json").write_text(
                '{"id":"test-machine"}', encoding="utf-8"
            )
            binding = root / "bridgepilot-local.json"
            binding.write_text(json.dumps({"product": "BridgePilot", "home": "\\\\?\\" + str(home)}), encoding="utf-8")
            db = workspace / "doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3"
            # A missing archive must fail without creating any replacement.
            with self.assertRaises(sqlite3.OperationalError):
                projects.snapshot(binding)
            self.assertFalse(db.exists())
            db.parent.mkdir(parents=True)
            with closing(sqlite3.connect(db)) as connection:
                connection.executescript("""
                    CREATE TABLE archive_meta(singleton,archive_id,revision);
                    INSERT INTO archive_meta VALUES(1,'test-archive',42);
                    CREATE TABLE projects(id,name,repository,paused);
                    INSERT INTO projects VALUES('first','First','owner/first',0),('second','Second','owner/second',1);
                    CREATE TABLE bindings(id,project,machine,root,desktop_project_id,host);
                    CREATE TABLE tasks(id,project);
                    CREATE TABLE runs(id,state,machine,thread_id,task,ordinal);
                    CREATE TABLE operations(id,run,kind,state);
                    CREATE TABLE custody(singleton,epoch,owner,handoff_json);
                    INSERT INTO custody VALUES(1,0,'test-machine',NULL);
                """)
            before = db.read_bytes()
            result = projects.snapshot(binding, ("Second",))
            self.assertTrue(result["read_only"])
            self.assertEqual(result["archive"]["revision"], 42)
            self.assertEqual([p["id"] for p in result["projects"]], ["second"])
            self.assertEqual(len(projects.snapshot(binding)["projects"]), 2)
            with self.assertRaisesRegex(ValueError, "unknown or ambiguous"):
                projects.snapshot(binding, ("unknown",))
            self.assertEqual(db.read_bytes(), before)

    def test_extended_unc_keeps_real_network_host(self):
        self.assertEqual(str(projects.local_path("\\\\?\\UNC\\server\\share\\archive")), "\\\\server\\share\\archive")


if __name__ == "__main__":
    unittest.main()
