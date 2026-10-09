#!/usr/bin/env python3
"""Prepare a fresh native fixture in an already consented hidden session."""
import argparse
import base64
import json
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'src'))
from understudy.client import Client
from understudy.config import Config
from understudy.session import diagnose


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', default='understudy.local.json')
    args = parser.parse_args()
    config = Config.load(args.config)
    client = Client(config.helper_uid, config.socket_path, config.output_dir)
    readiness = diagnose(client.status(), client.uid)
    if not readiness['ready']:
        raise RuntimeError('; '.join(readiness['nextSteps']))
    if readiness['session']['display'] != {'width': 2560, 'height': 1440, 'scale': 1}:
        raise RuntimeError('This coordinate fixture is qualified only at 2560×1440, scale 1')
    target = Path('/Users/Shared/UnderstudyTests')
    target.mkdir(mode=0o755, exist_ok=True)
    binary = target / 'DesktopFixture'
    subprocess.run(['swiftc', str(ROOT / 'tests/live/DesktopFixture.swift'), '-o', str(binary),
                    '-framework', 'AppKit', '-framework', 'CoreGraphics'], check=True)

    def run(argv):
        result = client.run(argv)
        if not result.get('ok') or result.get('exitCode') != 0:
            raise RuntimeError(result)
        return ''.join(base64.b64decode(event['data']).decode() for event in result.get('events', [])
                       if event.get('event') == 'output').strip()

    directory = run(['/usr/bin/mktemp', '-d', '/tmp/understudy-viewer.XXXXXX'])
    run(['/bin/chmod', '755', directory])
    command = ('nohup ' + shlex.quote(str(binary)) + ' ' + shlex.quote(directory)
               + ' >' + shlex.quote(directory + '/fixture.log') + ' 2>&1 & echo $!')
    pid = int(run(['/bin/sh', '-c', command]))
    record = {'fixturePID': pid, 'logDirectory': directory, 'session': client.identity()}
    client.artifacts.write('control-live-setup.json', (json.dumps(record, indent=2) + '\n').encode())
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()
