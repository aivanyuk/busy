"""Writes THIRD-PARTY-LICENSES.txt: the MIT license text (each crate's own copy, with its authors) of every crate
linked into busy.exe, grouped by identical text. Every one of them offers MIT (some also Apache-2.0 or the
Unlicense); busy uses them under MIT.

Run from the repository root after a dependency changes, then commit the result:
    python tools/third-party-licenses.py
Needs the crates in the local cargo registry (any `cargo build` fetches them). CI only checks that every
crate is named (tools/check-licenses.sh); this script is how the file is made.
"""
import json
import os
import subprocess

tree = subprocess.run(
    ['cargo', 'tree', '-p', 'busy', '-e', 'normal,no-proc-macro', '--prefix', 'none', '--format', '{p}', '--locked'],
    capture_output=True, text=True, check=True).stdout
wanted = sorted({tuple(l.split()[:2]) for l in tree.splitlines() if l and not l.startswith('busy')})
meta = json.loads(subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked'],
                                 capture_output=True, text=True, check=True).stdout)
by_id = {(p['name'], 'v' + p['version']): p for p in meta['packages']}

groups = {}
for name, version in wanted:
    p = by_id[(name, version)]
    root = os.path.dirname(p['manifest_path'])
    mit = next(f for f in sorted(os.listdir(root)) if f.lower().startswith('license-mit') or f.lower() == 'license-mit.md')
    text = open(os.path.join(root, mit), encoding='utf-8').read().strip()
    text = '\n'.join(l.strip() for l in text.splitlines())
    authors = ', '.join(p.get('authors') or []) or 'its authors'
    groups.setdefault(text, []).append(f'- {name} {version[1:]} ({p["license"]}), by {authors}')

out = ['Third-party software in busy.exe',
       '',
       'busy.exe includes code from the crates below. Each is offered under the MIT license (some also under',
       'Apache-2.0 or the Unlicense); busy uses them under MIT. Their license texts follow, each once for the',
       'crates that ship it.',
       '']
for text, crates in groups.items():
    out += ['=' * 100, ''] + crates + ['', text, '']
open('THIRD-PARTY-LICENSES.txt', 'w', encoding='utf-8', newline='\n').write('\n'.join(out))
print(f'{len(wanted)} crates, {len(groups)} license texts')
