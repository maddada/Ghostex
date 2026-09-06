#!/usr/bin/env python3
"""Bounded macOS process sampling: python3 tooling/profile-desktop.py --seconds 120 --output capture.jsonl.

RSS is resident memory, not physical footprint. Summed RSS can double-count shared pages.
CPU is calculated from cumulative process CPU time between samples. No command arguments are saved.
"""
import argparse
import datetime
import json
import pathlib
import subprocess
import time


def cpu_seconds(value):
    days, _, rest = value.rpartition('-')
    parts = [float(part) for part in rest.split(':')]
    return (int(days) * 86400 if days else 0) + sum(
        part * 60**index for index, part in enumerate(reversed(parts))
    )


def snapshot():
    output = subprocess.check_output(
        ['ps', '-axo', 'pid=,ppid=,rss=,time=,comm='], text=True
    )
    rows = []
    for line in output.splitlines():
        fields = line.split(None, 4)
        if len(fields) != 5:
            continue
        pid, ppid, rss, cpu, executable = fields
        rows.append(dict(pid=int(pid), ppid=int(ppid), rssKiB=int(rss),
                         cpuSeconds=cpu_seconds(cpu), executable=executable))
    roots = {r['pid'] for r in rows if r['executable'] == '/Applications/Ghostex.app/Contents/MacOS/Ghostex'}
    descendants = set(roots)
    while True:
        expanded = descendants | {r['pid'] for r in rows if r['ppid'] in descendants}
        if expanded == descendants:
            break
        descendants = expanded
    selected = []
    for row in rows:
        executable = row.pop('executable')
        if row['pid'] not in descendants and not executable.startswith('/Applications/Ghostex.app/'):
            continue
        name = pathlib.Path(executable).name
        row['group'] = ('gpui' if row['pid'] in roots else
                        'cef' if 'Helper' in name or 'cef-helper' in name else
                        'server' if name == 'gxserver' else
                        'zmx' if name == 'zmx' else
                        'editor' if name == 'GhostexEditor' else 'other')
        row['appDescendant'] = row['pid'] in descendants
        selected.append(row)
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=int, default=120, choices=range(1, 1801), metavar='1..1800')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--phase', type=pathlib.Path, help='Optional text file read as an interaction label each second')
    args = parser.parse_args()
    previous = {}
    start = last = time.monotonic()
    with args.output.open('x') as target:
        while time.monotonic() - start < args.seconds:
            now = time.monotonic()
            rows = snapshot()
            for row in rows:
                old = previous.get(row['pid'])
                row['cpuPercent'] = max(0, (row['cpuSeconds'] - old) / (now - last) * 100) if old is not None else None
            previous = {r['pid']: r['cpuSeconds'] for r in rows}
            label = args.phase.read_text().strip() if args.phase and args.phase.exists() else ''
            target.write(json.dumps(dict(timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                         elapsedSeconds=now-start, phase=label, processes=rows)) + '\n')
            target.flush()
            last = now
            time.sleep(max(0, 1 - (time.monotonic() - now)))


if __name__ == '__main__':
    main()
