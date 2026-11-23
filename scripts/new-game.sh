#!/bin/bash
# Start a new game from this project.
#
#   scripts/new-game.sh <new-project-folder>
#
# For example:  scripts/new-game.sh ~/Projects/Rust/space-arena
#
# The folder's name becomes the game's name. It must be lowercase letters,
# digits and hyphens, starting with a letter (it is also the Rust package name).
#
# The copy is a complete, working game with its own name everywhere: window
# title, settings folder, LAN announcements and network protocol, so it can
# never join a game of the original by accident. It starts with a fresh git
# history and nothing committed.
set -eu
SOURCE=$(cd "$(dirname "$0")/.." && pwd)

[ $# -eq 1 ] || { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
DEST=$(realpath -m "$1")
NEW=$(basename "$DEST")
OLD=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' "$SOURCE/Cargo.toml" | head -1)

if ! [[ $NEW =~ ^[a-z][a-z0-9-]*$ ]]; then
    echo "new-game: '$NEW' won't work as a name: use lowercase letters, digits and hyphens, starting with a letter" >&2
    exit 2
fi
if [ -e "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ]; then
    echo "new-game: $DEST already exists and isn't empty" >&2
    exit 2
fi

mkdir -p "$DEST"
cd "$SOURCE"
# Everything git knows about or would add: no build output, no local settings.
git ls-files -co --exclude-standard -z | while IFS= read -r -d '' file; do
    [ -e "$file" ] || continue
    mkdir -p "$DEST/$(dirname "$file")"
    cp -p "$file" "$DEST/$file"
done

# Rename the package. The game reads its own name from Cargo.toml, so this is
# the only thing that has to change; the rest keeps docs and comments honest.
cd "$DEST"
grep -rlIZ -e "$OLD" -e "${OLD//-/_}" . 2>/dev/null | while IFS= read -r -d '' file; do
    sed -i "s/${OLD//-/_}/${NEW//-/_}/g; s/$OLD/$NEW/g" "$file"
done
git init -q

cat <<DONE
Created $NEW in $DEST

Next:
  cd $DEST
  cargo run                    # the first build compiles the engine and takes a while
  scripts/check.sh --quick     # confirm the copy works
  less AGENTS.md               # how the project is laid out and how to change it

Nothing is committed yet. When it runs:  git add -A && git commit -m "Start $NEW"
DONE
