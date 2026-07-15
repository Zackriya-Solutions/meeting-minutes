import sqlite3
import unittest

import prepare_standup_corpus as corpus


class StandupCorpusExporterTests(unittest.TestCase):
    def test_reviewed_edits_seed_references_but_never_hypotheses(self) -> None:
        db = sqlite3.connect(":memory:")
        db.execute(
            "CREATE TABLE standup_records("
            "id INTEGER PRIMARY KEY, meeting_id TEXT, kind TEXT, payload TEXT, "
            "reviewed_payload TEXT, review_status TEXT)"
        )
        db.executemany(
            "INSERT INTO standup_records VALUES(?, 'm1', 'action', ?, ?, ?)",
            [
                (1, '{"task":"raw"}', '{"task":"human correction"}', "accepted"),
                (2, '{"task":"bad model claim"}', None, "rejected"),
                (3, '{"task":"unreviewed"}', None, "pending"),
            ],
        )

        references = corpus.reviewed_references(db, "m1")
        self.assertEqual(
            references,
            [
                {
                    "id": "review-1",
                    "kind": "action",
                    "text": "human correction",
                    "owner": None,
                    "due_date": None,
                }
            ],
        )

        hypotheses = corpus.flatten_standup(
            {"action_items": [{"task": "current provider output", "evidence": []}]}
        )
        self.assertEqual(hypotheses[0]["text"], "current provider output")
        self.assertIsNone(hypotheses[0]["match_id"])

    def test_generation_identity_excludes_connection_details(self) -> None:
        result = {
            "summary_generation": {
                "source": {
                    "model_provider": "deepseek",
                    "model_name": "deepseek-chat",
                    "template_fingerprint": "abc:3",
                    "custom_openai_endpoint": "https://secret.invalid",
                }
            }
        }
        identity = corpus.generation_identity(result)
        self.assertEqual(identity, ("deepseek", "deepseek-chat", "abc:3"))
        self.assertNotIn("secret", repr(identity))


if __name__ == "__main__":
    unittest.main()
