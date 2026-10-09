"""One validated configuration, independent of the working directory."""
from dataclasses import dataclass
from pathlib import Path

from .session import validate_helper_uid
from .transport import decode_json


@dataclass(frozen=True)
class Config:
    helper_uid: int
    socket_path: Path
    output_dir: Path

    @classmethod
    def load(cls, filename):
        path = Path(filename).expanduser().resolve()
        if not path.is_file():
            raise ValueError("Missing config: copy understudy.example.json to understudy.local.json and set the helper UID/socket")
        if path.stat().st_size > 16384:
            raise ValueError("Configuration exceeds 16 KiB")
        raw = decode_json(path.read_bytes())
        if not isinstance(raw, dict):
            raise ValueError("Configuration must be a JSON object")
        if set(raw) - {"helper_uid", "socket_path", "output_dir"}:
            raise ValueError("Unknown configuration keys: " + ", ".join(sorted(set(raw) - {"helper_uid", "socket_path", "output_dir"})))
        uid = raw.get("helper_uid")
        validate_helper_uid(uid)

        def location(name, default=None):
            value = raw.get(name, default)
            if not isinstance(value, str) or not value.strip() or "\0" in value:
                raise ValueError(name + " must be a nonempty path string")
            target = Path(value).expanduser()
            return (target if target.is_absolute() else path.parent / target).resolve()

        return cls(uid, location("socket_path"), location("output_dir", "runs"))
