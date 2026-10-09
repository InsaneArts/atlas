import argparse
import json
import os
from pathlib import Path
import sys

from .client import Client, SessionError


def main(argv=None):
    parser.add_argument("--config", default=os.environ.get("UNDERSTUDY_CONFIG", "understudy.local.json"))
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("status", help="Read identity and permission status; never prompts")
    commands.add_parser("doctor", help="Check readiness and list only the missing setup steps")
    monitor = commands.add_parser("monitor", help="Serve a local read-only live view")
    monitor.add_argument("--port", type=int, default=8765)
    capture = commands.add_parser("capture", help="Save a real helper PNG and return its path")
    capture.add_argument("--max-dimension", type=int)
    commands.add_parser("apps", help="List applications inside the helper session")
    action = commands.add_parser("input", help="Send a JSON action array, or @path/to/actions.json")
    action.add_argument("actions")
    run = commands.add_parser("run", help="Launch a command in the helper session")
    run.add_argument("argv", nargs=argparse.REMAINDER)
    commands.add_parser("restart", help="Restart the helper; may interrupt in-flight work")
    args = parser.parse_args(argv)
    try:
        config_path = Path(args.config).expanduser().resolve()
        if not config_path.is_file():
            raise ValueError("Missing config: copy understudy.example.json to understudy.local.json and set the intended helper UID/socket")
        config = json.loads(config_path.read_text())
        output = Path(config.get("output_dir", "runs")).expanduser()
        if not output.is_absolute():
            output = config_path.parent / output
        os.umask(0o077)
        client = Client(config["helper_uid"], config["socket_path"], output)
        if args.command == "status":
            result = client.status()
        elif args.command == "doctor":
            from .doctor import diagnose
            result = diagnose(client.status(), client.uid)
            print(json.dumps(result, indent=2, ensure_ascii=False))
            return 0 if result["ready"] else 1
        elif args.command == "monitor":
            from .monitor import serve
            return serve(client, args.port)
        elif args.command == "capture":
            result = client.capture(args.max_dimension)
        elif args.command == "input":
            raw = Path(args.actions[1:]).read_text() if args.actions.startswith("@") else args.actions
            result = client.input(json.loads(raw))
        elif args.command == "run":
            command = args.argv[1:] if args.argv[:1] == ["--"] else args.argv
            result = client.run(command)
        elif args.command == "apps":
            result = client.apps()
        else:
            result = client.restart()
        print(json.dumps(result, indent=2, ensure_ascii=False))
        if result.get("ok") is not True or result.get("timedOut") or result.get("exitCode", 0) != 0:
            return 1
        return 0
    except (OSError, ValueError, KeyError, SessionError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}), file=sys.stderr)
        return 1
