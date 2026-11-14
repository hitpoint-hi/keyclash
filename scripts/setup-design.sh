#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

shared_skill="$HOME/.codex/skills/impeccable"
if [ ! -x "$shared_skill/scripts/impeccable" ]; then
    echo 'Install the shared Impeccable skill in ~/.codex/skills/impeccable through Skills Manager.' >&2
    exit 1
fi

project_skill='.agents/skills/impeccable'
if [ ! -e "$project_skill" ] && [ ! -L "$project_skill" ]; then
    mkdir -p .agents/skills
    ln -s "$shared_skill" "$project_skill"
fi

if [ ! -x "$project_skill/scripts/impeccable" ]; then
    echo 'The existing .agents/skills/impeccable path does not contain an executable launcher. Inspect that path before retrying.' >&2
    exit 1
fi

"$project_skill/scripts/impeccable" hooks on
"$project_skill/scripts/impeccable" doctor --json
