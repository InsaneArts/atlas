"""Pure refusal checks, separate from the recorded real-desktop trials."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from understudy.client import SessionError, require_hidden_session, validate_helper_uid


class SessionGuardTests(unittest.TestCase):
    def setUp(self):
        self.status = {"ok": True, "user": {"uid": 502},
                       "session": {"onConsole": False, "managerName": "Aqua"},
                       "display": {"width": 2560, "height": 1440, "scale": 1}}

    def test_known_hidden_identity_is_accepted(self):
        self.assertIs(require_hidden_session(self.status, 502), self.status)

    def test_visible_or_unknown_console_state_is_refused(self):
        for value in [True, None, 0, "false"]:
            with self.subTest(value=value):
                self.status["session"]["onConsole"] = value
                with self.assertRaises(SessionError):
                    require_hidden_session(self.status, 502)

    def test_wrong_uid_is_refused(self):
        self.status["user"]["uid"] = 501
        with self.assertRaises(SessionError):
            require_hidden_session(self.status, 502)

    def test_non_aqua_context_is_refused(self):
        self.status["session"]["managerName"] = "Background"
        with self.assertRaises(SessionError):
            require_hidden_session(self.status, 502)

    def test_incomplete_reply_is_refused(self):
        for key in ["ok", "user", "session", "display"]:
            row = copy.deepcopy(self.status)
            del row[key]
            with self.subTest(key=key), self.assertRaises(SessionError):
                require_hidden_session(row, 502)

    def test_missing_or_zero_display_is_refused(self):
        for value in [None, 0, -1, "2560", True, float("nan"), float("inf")]:
            self.status["display"]["width"] = value
            with self.subTest(value=value), self.assertRaises(SessionError):
                require_hidden_session(self.status, 502)

    def test_malformed_sections_are_refused(self):
        for key in ["user", "session", "display"]:
            row = copy.deepcopy(self.status)
            row[key] = "invalid"
            with self.subTest(key=key), self.assertRaises(SessionError):
                require_hidden_session(row, 502)

    def test_primary_and_system_uids_are_refused(self):
        for uid in [0, 500, 501, "502", True]:
            with self.subTest(uid=uid), self.assertRaises(SessionError):
                validate_helper_uid(uid)


if __name__ == "__main__":
    unittest.main()
