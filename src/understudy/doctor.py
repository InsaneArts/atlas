"""Readiness diagnosis only; never changes accounts, permissions or login state."""
from .client import SessionError, require_hidden_session


def diagnose(status, uid):
    checks = []

    def check(name, passed, fix):
        checks.append({"name": name, "ok": passed, "nextStep": None if passed else fix})

    reachable = isinstance(status, dict) and status.get("ok") is True
    check("Helper connection", reachable,
          "Check the configured socket. Log into the helper account if needed and start its installed LaunchAgent.")
    hidden = False
    permissions = {}
    if reachable:
        try:
            require_hidden_session(status, uid)
            hidden = True
        except SessionError:
            hidden = False
        check("Hidden graphical session", hidden,
              "The configured UID must have an Aqua desktop. Switch back to your main account without logging the helper out.")
        permissions = status.get("permissions") or {}
        if not isinstance(permissions, dict):
            permissions = {}
        check("Screen recording", permissions.get("screenCapture") is True,
              "In the helper account, enable Screen & System Audio Recording for the installed helper, then restart that helper.")
        check("Input permission", permissions.get("accessibility") is True,
              "In the helper account, enable Accessibility / Device Control and Data Access for the installed helper, then restart it.")
    return {"ok": True, "ready": all(c["ok"] for c in checks),
            "canObserve": reachable and hidden and permissions.get("screenCapture") is True,
            "checks": checks,
            "nextSteps": [c["nextStep"] for c in checks if not c["ok"]],
            "session": status if reachable else None}
