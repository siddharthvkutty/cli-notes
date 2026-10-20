#!/usr/bin/env bash
#
# Build the `notes` CLI and install it on your PATH.
#
# Usage:
#   ./install.sh              install to ~/.local/bin and ensure it is on PATH
#   ./install.sh --prefix DIR install to DIR instead
#   ./install.sh --no-path    install the binary only, do not edit shell rc files
#   ./install.sh --uninstall  remove the installed binary (and the PATH line we added)
#
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${NOTES_PREFIX:-$HOME/.local/bin}"
MANAGE_PATH=1
DO_INSTALL=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="$2"; shift 2 ;;
    --no-path)
      MANAGE_PATH=0; shift ;;
    --uninstall)
      DO_INSTALL=0; shift ;;
    -h|--help)
      sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)
      echo "unknown option: $1 (see --help)" >&2; exit 1 ;;
  esac
done

MARK_BEGIN="# >>> notes CLI >>>"
MARK_END="# <<< notes CLI <<<"
PATH_LINE="export PATH=\"$PREFIX:\$PATH\""
# Guarded so running it in nested shells never duplicates the PATH entry.
PATH_BLOCK="case \":\$PATH:\" in *\":$PREFIX:\"*) ;; *) export PATH=\"$PREFIX:\$PATH\" ;; esac"

# Shell rc files that may exist; we append to every one of them so the
# binary is reachable from login shells, interactive shells and scripts.
rc_files=()
for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.bash_profile" "$HOME/.profile"; do
  [[ -f "$rc" ]] && rc_files+=("$rc")
done
[[ ${#rc_files[@]} -eq 0 ]] && rc_files=("$HOME/.bashrc")

remove_path_block() {
  local rc
  for rc in "${rc_files[@]}"; do
    [[ -f "$rc" ]] || continue
    if grep -qF "$MARK_BEGIN" "$rc"; then
      local tmp
      tmp="$(mktemp)"
      awk -v b="$MARK_BEGIN" -v e="$MARK_END" '
        $0 == b {skip=1; next}
        $0 == e {skip=0; next}
        !skip {lines[++n]=$0}
        END {
          while (n > 0 && lines[n] ~ /^[[:space:]]*$/) n--
          for (i = 1; i <= n; i++) print lines[i]
        }
      ' "$rc" > "$tmp"
      # Overwrite in place so the file keeps its original permissions.
      cat "$tmp" > "$rc"
      rm -f "$tmp"
      echo "Removed PATH entry from $rc"
    fi
  done
}

if [[ $DO_INSTALL -eq 0 ]]; then
  rm -f "$PREFIX/notes"
  echo "Removed $PREFIX/notes"
  [[ $MANAGE_PATH -eq 1 ]] && remove_path_block
  exit 0
fi

echo "==> Building release binary..."
(cd "$REPO_DIR" && cargo build --release --quiet)

echo "==> Installing to $PREFIX/notes"
mkdir -p "$PREFIX"
install -m 755 "$REPO_DIR/target/release/notes" "$PREFIX/notes"

# Always write the rc block, even if PREFIX already happens to be on PATH in
# *this* shell (e.g. right after --uninstall): only new shells read the rc files.
if [[ $MANAGE_PATH -eq 1 ]]; then
  echo "==> Ensuring $PREFIX is on PATH in ${rc_files[*]}"
  remove_path_block
  for rc in "${rc_files[@]}"; do
    {
      echo ""
      echo "$MARK_BEGIN"
      echo "$PATH_BLOCK"
      echo "$MARK_END"
    } >> "$rc"
  done
  case ":$PATH:" in
    *":$PREFIX:"*) ;;
    *) export PATH="$PREFIX:$PATH" ;;
  esac
else
  case ":$PATH:" in
    *":$PREFIX:"*) echo "==> $PREFIX is already on PATH" ;;
    *)
      echo "==> $PREFIX is not on PATH. Add this line to your shell rc:"
      echo "    $PATH_LINE"
      ;;
  esac
fi

echo "==> Done. Open a new terminal (or run: source ${rc_files[0]}) and try: notes --help"
