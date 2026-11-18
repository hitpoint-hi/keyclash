#!/bin/bash
# Deploy site/ to keyclash.app.
#
# The VPS serves /srv/keyclash.app/current (nginx container, behind a Cloudflare Tunnel).
# Each deploy goes into releases/<UTC timestamp>, then the `current` link moves to it.
# nginx follows the link, so the change is live at once, with no restart.
#
# Usage:
#   scripts/deploy-site.sh              deploy site/ from HEAD (committed files only)
#   scripts/deploy-site.sh v0.1.1       deploy site/ from a tag, branch or commit
#   scripts/deploy-site.sh --dry-run    show what changes, deploy nothing
#   scripts/deploy-site.sh --rollback   point `current` at the release before it
#
# Settings: KEYCLASH_SITE_HOST (ssh host, default food-list-vps), KEYCLASH_SITE_KEEP (releases to keep, default 5).
set -euo pipefail
cd "$(dirname "$0")/.."

host="${KEYCLASH_SITE_HOST:-food-list-vps}"
keep="${KEYCLASH_SITE_KEEP:-5}"
root=/srv/keyclash.app
url=https://keyclash.app/

mode=deploy
ref=HEAD
for arg in "$@"; do
  case "$arg" in
    --dry-run) mode=dry-run ;;
    --rollback) mode=rollback ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    -*) echo "Unknown option: $arg" >&2; exit 2 ;;
    *) ref="$arg" ;;
  esac
done

remote() { ssh -o BatchMode=yes "$host" "$@"; }

if [ "$mode" = rollback ]; then
  remote "set -e
    cd $root
    now=\$(readlink current | sed 's#releases/##')
    prev=\$(ls releases | sort | awk -v now=\"\$now\" '\$0 < now' | tail -1)
    [ -n \"\$prev\" ] || { echo 'No older release to roll back to.' >&2; exit 1; }
    sudo -n ln -sfn releases/\$prev current.tmp
    sudo -n mv -T current.tmp current
    echo \"Rolled back: \$now -> \$prev\""
  exit 0
fi

git rev-parse --verify --quiet "$ref^{commit}" >/dev/null || { echo "Unknown ref: $ref" >&2; exit 2; }
commit=$(git rev-parse --short "$ref^{commit}")
if [ "$ref" = HEAD ] && [ -n "$(git status --porcelain -- site)" ]; then
  echo "Note: site/ has uncommitted changes. They are not deployed (only committed files from $commit)." >&2
fi

# Committed site files only. READMEs and .gitignore stay out of the public folder.
archive() { git archive --format=tar "$ref:site" -- . ':(exclude,glob)**/README.md' ':(exclude).gitignore'; }

ts=$(date -u +%Y%m%dT%H%M%SZ)
stage="$root/releases/$ts"
echo "Deploying site/ from $commit to $host:$stage"

archive | remote "set -e
  sudo -n mkdir -p $stage
  sudo -n tar -x --no-same-owner -C $stage
  sudo -n chmod -R a+rX $stage
  echo '--- Changes compared with the live release:'
  sudo -n diff -rq $root/current/ $stage/ | sed 's#$root/##g' || true
  if [ '$mode' = dry-run ]; then
    sudo -n rm -rf $stage
    echo 'Dry run: nothing deployed.'
    exit 0
  fi
  sudo -n ln -sfn releases/$ts $root/current.tmp
  sudo -n mv -T $root/current.tmp $root/current
  echo \"Live: \$(readlink $root/current)\"
  # Keep the newest $keep releases. The live one is always the newest.
  cd $root/releases && ls | sort | head -n -$keep | xargs -r sudo -n rm -rf --
  echo \"Releases on disk: \$(ls | wc -l)\""

[ "$mode" = dry-run ] && exit 0

# Check that the live page is the new one.
code=$(curl -s -o /dev/null -w '%{http_code}' "$url?deploy=$ts")
modified=$(curl -sI "$url?deploy=$ts" | tr -d '\r' | sed -n 's/^last-modified: //Ip')
echo "Check: $url answers HTTP $code, last-modified $modified"
[ "$code" = 200 ] || { echo "The site does not answer 200. Run: scripts/deploy-site.sh --rollback" >&2; exit 1; }
