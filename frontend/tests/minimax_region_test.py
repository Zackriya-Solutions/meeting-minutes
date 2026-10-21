"""Dependency-free checks of the production SQLite migration and save query.

Run: python3 -m unittest discover -s frontend/tests -p minimax_region_test.py
"""
import re
import sqlite3
import unittest
from pathlib import Path

TAURI = Path(__file__).resolve().parents[1] / "src-tauri"


class MiniMaxRegionPersistence(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        for migration in sorted((TAURI / "migrations").glob("*.sql")):
            self.db.executescript(migration.read_text())
        source = (TAURI / "src/database/repositories/setting.rs").read_text()
        save = source.split("pub async fn save_model_config(", 1)[1]
        self.query = re.search(r'r#"(.*?)"#', save, re.S).group(1)

    def tearDown(self):
        self.db.close()

    def save(self, provider, model, region=None):
        self.db.execute(self.query, (provider, model, "large-v3", None, region))
        self.db.commit()
        return self.db.execute(
            "SELECT provider, model, minimaxRegion FROM settings WHERE id = '1'"
        ).fetchone()

    def test_new_configuration_defaults_global(self):
        self.assertEqual(self.save("minimax", "MiniMax-M3.1-Flash"),
                         ("minimax", "MiniMax-M3.1-Flash", "global"))

    def test_region_round_trip_preserves_explicit_models(self):
        for model in ("MiniMax-M3.1-Flash", "MiniMax-M3", "MiniMax-M2.7", "custom-id"):
            self.assertEqual(self.save("minimax", model, "cn"), ("minimax", model, "cn"))
            self.assertEqual(self.save("minimax", model), ("minimax", model, "cn"))
            self.assertEqual(self.save("minimax", model, "global"),
                             ("minimax", model, "global"))

    def test_other_provider_save_preserves_minimax_region(self):
        self.save("minimax", "MiniMax-M3.1-Flash", "cn")
        self.assertEqual(self.save("ollama", "llama3.2:latest"),
                         ("ollama", "llama3.2:latest", "cn"))
        self.assertEqual(self.save("minimax", "MiniMax-M2.7"),
                         ("minimax", "MiniMax-M2.7", "cn"))

    def test_upgrade_preserves_legacy_model_and_key(self):
        with sqlite3.connect(":memory:") as db:
            for migration in sorted((TAURI / "migrations").glob("*.sql")):
                if "20261021000000" not in migration.name:
                    db.executescript(migration.read_text())
            db.execute("INSERT INTO settings (id, provider, model, whisperModel, minimaxApiKey) "
                       "VALUES ('1', 'minimax', 'MiniMax-M2.7', 'large-v3', 'test-key')")
            db.executescript((TAURI / "migrations/20261021000000_add_minimax_region.sql").read_text())
            self.assertEqual(db.execute("SELECT model, minimaxApiKey, minimaxRegion FROM settings").fetchone(),
                             ("MiniMax-M2.7", "test-key", "global"))


if __name__ == "__main__":
    unittest.main()
