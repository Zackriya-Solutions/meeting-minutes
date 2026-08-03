import importlib.util
import pathlib
import unittest
from unittest import mock


MODULE_PATH = pathlib.Path(__file__).with_name("memento_pr_review.py")
SPEC = importlib.util.spec_from_file_location("memento_pr_review", MODULE_PATH)
reviewer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reviewer)


class MementoPrReviewTests(unittest.TestCase):
    def test_bound_diff_reports_truncation(self):
        bounded, truncated = reviewer.bound_diff("x" * (reviewer.MAX_DIFF_CHARS + 1))
        self.assertEqual(len(bounded), reviewer.MAX_DIFF_CHARS)
        self.assertTrue(truncated)

    def test_bound_diff_prioritizes_sensitive_sections(self):
        ordinary = (
            "diff --git a/docs/large.md b/docs/large.md\n"
            "--- a/docs/large.md\n+++ b/docs/large.md\n"
            + "x" * reviewer.MAX_DIFF_CHARS
        )
        sensitive = (
            "diff --git a/stats/server.py b/stats/server.py\n"
            "--- a/stats/server.py\n+++ b/stats/server.py\n+safe change\n"
        )
        bounded, truncated = reviewer.bound_diff(ordinary + "\n" + sensitive)
        self.assertTrue(truncated)
        self.assertIn("stats/server.py", bounded)
        self.assertNotIn("docs/large.md", bounded)

    def test_payload_forces_schema_and_privacy(self):
        payload = reviewer.messages_payload("policy", "85", "title", "diff", False)
        self.assertEqual(payload["tool_choice"], {"type": "tool", "name": reviewer.TOOL_NAME})
        self.assertEqual(payload["reasoning"], {"effort": "high", "exclude": True})
        self.assertEqual(payload["provider"], {"data_collection": "deny", "zdr": True})
        self.assertTrue(payload["turn_off_message_logging"])

    def test_extract_review_requires_tool_result(self):
        expected = {"summary": "Safe", "findings": [], "test_gaps": [], "residual_risks": []}
        response = {"content": [{"type": "tool_use", "name": reviewer.TOOL_NAME, "input": expected}]}
        self.assertEqual(reviewer.extract_review(response), expected)
        with self.assertRaises(RuntimeError):
            reviewer.extract_review({"content": [{"type": "text", "text": "no tool"}]})

    def test_blocking_verdict_only_for_p0_or_p1(self):
        self.assertFalse(reviewer.has_blockers({"findings": [{"severity": "P2"}]}))
        self.assertTrue(reviewer.has_blockers({"findings": [{"severity": "P1"}]}))

    def test_render_comment_escapes_model_content(self):
        body = reviewer.render_comment(
            "a" * 40,
            {
                "summary": "<script>bad</script>",
                "findings": [],
                "test_gaps": [],
                "residual_risks": [],
            },
            {},
            False,
        )
        self.assertNotIn("<script>", body)
        self.assertIn("&lt;script&gt;", body)
        self.assertIn("No blocking findings", body)

    def test_already_reviewed_accepts_only_actions_bot_marker(self):
        marker = reviewer.review_marker("a" * 40)
        with mock.patch.object(
            reviewer,
            "get_json",
            return_value=[{"body": marker, "user": {"login": "collaborator"}}],
        ):
            self.assertFalse(reviewer.already_reviewed("owner/repo", "85", "token", "a" * 40))
        with mock.patch.object(
            reviewer,
            "get_json",
            return_value=[{"body": marker, "user": {"login": "github-actions[bot]"}}],
        ):
            self.assertTrue(reviewer.already_reviewed("owner/repo", "85", "token", "a" * 40))


if __name__ == "__main__":
    unittest.main()
