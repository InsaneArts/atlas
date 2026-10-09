"""Validate an entire bounded input batch before the native daemon sees it."""
import math
import string

MODIFIERS = {"cmd", "command", "ctrl", "control", "alt", "opt", "option", "shift", "fn"}
KEYS = set(string.ascii_lowercase + string.digits + "=-][;'\\,/`.") | {
    "return", "enter", "tab", "space", "backspace", "delete", "escape", "esc",
    "forwarddelete", "home", "pageup", "end", "pagedown", "left", "right", "down", "up",
} | {"f" + str(n) for n in range(1, 13)}
FIELDS = {
    "move": {"x", "y"}, "click": {"x", "y", "button", "count", "modifiers"},
    "drag": {"fromX", "fromY", "toX", "toY", "button"},
    "scroll": {"x", "y", "dx", "dy"}, "type": {"text"}, "key": {"key"}, "wait": {"ms"},
}


def validate_key(combo):
    if not isinstance(combo, str) or not combo or len(combo) > 80:
        raise ValueError("Key must be a supported key or modifier combination")
    parts = combo.lower().split("+")
    name = parts.pop()
    if not name and parts:
        name = "="
    if name not in KEYS or any(p not in MODIFIERS for p in parts):
        raise ValueError("Unsupported key combination")


def validate_actions(actions, display):
    if not isinstance(actions, list) or not 1 <= len(actions) <= 32:
        raise ValueError("Input must contain 1–32 actions")
    cost = 0

    def number(value, name, low, high, integer=False):
        if (type(value) not in (int, float) or not low <= value <= high
                or not math.isfinite(value) or (integer and type(value) is not int)):
            raise ValueError(name + " is outside its permitted range")

    def point(action, x="x", y="y"):
        number(action.get(x), x, 0, display["width"] - 0.001)
        number(action.get(y), y, 0, display["height"] - 0.001)

    for action in actions:
        if not isinstance(action, dict) or not isinstance(action.get("type"), str):
            raise ValueError("Every action must have a type")
        kind = action["type"]
        if kind not in FIELDS or set(action) - (FIELDS[kind] | {"type"}):
            raise ValueError("Unsupported action or fields")
        if kind in {"click", "move", "scroll"}:
            point(action)
        if kind in {"click", "drag"}:
            button = action.get("button", "left")
            if not isinstance(button, str) or button not in {"left", "right", "middle"}:
                raise ValueError("Unsupported mouse button")
        if kind == "click":
            count = action.get("count", 1)
            number(count, "Click count", 1, 5, integer=True)
            modifiers = action.get("modifiers", [])
            if not isinstance(modifiers, list) or any(not isinstance(m, str) or m not in MODIFIERS for m in modifiers):
                raise ValueError("Unsupported mouse modifiers")
            cost += count * 70
        elif kind == "drag":
            point(action, "fromX", "fromY")
            point(action, "toX", "toY")
            cost += 650
        elif kind == "scroll":
            number(action.get("dx", 0), "Horizontal scroll", -100, 100, integer=True)
            number(action.get("dy", 0), "Vertical scroll", -100, 100, integer=True)
            cost += 10
        elif kind == "type":
            text = action.get("text")
            if not isinstance(text, str) or not 1 <= len(text) <= 256:
                raise ValueError("Each text action must contain 1–256 characters")
            text.encode("utf-8")
            cost += len(text) * 3
        elif kind == "key":
            validate_key(action.get("key"))
            cost += 10
        elif kind == "wait":
            number(action.get("ms"), "Wait", 0, 250, integer=True)
            cost += action["ms"]
        if cost > 2000:
            raise ValueError("Input batch exceeds the two-second budget; send shorter batches after observing state")
    return actions
