"""Pure identity checks and readiness diagnosis. Never requests permissions."""
import math
import os

from .errors import SessionError


def validate_helper_uid(uid):
    if type(uid) is not int or uid <= 501 or uid == os.getuid():
        raise SessionError("Target must be a distinct helper account with UID >= 502", "identity")


def require_hidden_session(status, uid):
    validate_helper_uid(uid)
    if not isinstance(status, dict) or status.get("ok") is not True:
        raise SessionError("Helper connection is unavailable", "connection")
    user, session, display = (status.get(key) for key in ("user", "session", "display"))
    if not all(isinstance(value, dict) for value in (user, session, display)):
        raise SessionError("Helper returned an incomplete session identity", "identity")
    geometry = [display.get("width"), display.get("height"), display.get("scale")]
    if (type(user.get("uid")) is not int or user["uid"] != uid
            or session.get("onConsole") is not False or session.get("managerName") != "Aqua"
            or not all(type(v) in (int, float) and 0 < v <= limit and math.isfinite(v)
                       for v, limit in zip(geometry, (32768, 32768, 8)))):
        raise SessionError("Expected the configured helper in a hidden Aqua desktop with a valid display", "identity")
    return status


def epoch(status):
    """Identity attached to a frame; a daemon restart invalidates old input."""
    daemon = status.get("daemon")
    if not isinstance(daemon, dict) or type(daemon.get("pid")) is not int or daemon["pid"] <= 0:
        raise SessionError("Helper did not identify its process", "identity")
    display = status["display"]
    return (status["user"]["uid"], daemon["pid"], display["width"], display["height"], display["scale"])


def diagnose(status, uid):
    checks = []

    def check(name, passed, fix):
        checks.append({"name": name, "ok": passed, "nextStep": None if passed else fix})

    reachable = isinstance(status, dict) and status.get("ok") is True
    check("Helper connection", reachable,
          "Check the configured socket. Log into the helper account if needed and start its installed LaunchAgent.")
    hidden, permissions = False, {}
    if reachable:
        try:
            require_hidden_session(status, uid)
            hidden = True
        except SessionError:
            pass
        check("Hidden desktop", hidden,
              "The configured helper must have an Aqua desktop. Switch back to your main account without logging the helper out.")
        permissions = status.get("permissions")
        if not isinstance(permissions, dict):
            permissions = {}
        check("Screen recording", permissions.get("screenCapture") is True,
              "In the helper account, enable Screen & System Audio Recording for the installed helper, then restart it.")
        check("Mouse and keyboard", permissions.get("accessibility") is True,
              "In the helper account, enable Accessibility / Device Control and Data Access for the installed helper, then restart it.")
    return {"ok": True, "ready": all(c["ok"] for c in checks),
            "canObserve": reachable and hidden and permissions.get("screenCapture") is True,
            "checks": checks, "nextSteps": [c["nextStep"] for c in checks if not c["ok"]],
            "session": status if reachable else None}
