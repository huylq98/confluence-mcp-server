#!/usr/bin/env bash
# Rebuild wrapper for git-bash / MSYS shells where cargo isn't on PATH.
# Forwards any args to build.ps1 (e.g. -UseUpx).
set -euo pipefail
ROOT_WIN="$(cygpath -w "$(cd "$(dirname "$0")/.." && pwd)")"
powershell.exe -ExecutionPolicy Bypass -Command \
  "\$env:PATH = \"\$env:USERPROFILE\\.cargo\\bin;\" + \$env:PATH; & \"$ROOT_WIN\\scripts\\build.ps1\" $*"
