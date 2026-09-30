#!/usr/bin/env bash
# Fails when a crate linked into busy.exe isn't named in THIRD-PARTY-LICENSES.txt at its version (the release
# zip ships that file). Fix: `python tools/third-party-licenses.py`, then commit the file.
set -euo pipefail
cd "$(dirname "$0")/.."
missing=0
while read -r name version; do
  if ! grep -q -- "^- $name ${version#v} " THIRD-PARTY-LICENSES.txt; then
    echo "THIRD-PARTY-LICENSES.txt doesn't name $name ${version#v}"
    missing=1
  fi
done < <(cargo tree -p busy -e normal,no-proc-macro --prefix none --format '{p}' --locked \
  | awk '$1 !~ /^busy/ { print $1, $2 }' | sort -u)
if [ "$missing" -ne 0 ]; then
  echo "Regenerate it: python tools/third-party-licenses.py"
  exit 1
fi
echo "THIRD-PARTY-LICENSES.txt names every crate in busy.exe"
