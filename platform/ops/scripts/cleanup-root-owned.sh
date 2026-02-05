#!/usr/bin/env bash
set -euo pipefail

ROOT_OWNED_DIRS=(
  .apps.root-owned
  .configs.root-owned
  .crates.root-owned
  .docs.root-owned
  .github.root-owned
  .migrations.root-owned
  .scripts.root-owned
  .serena.root-owned
  .target.root-owned
  .tests.root-owned
  .tools.root-owned
  .platform.root-owned
  apps.root-owned
  configs.root-owned
  crates.root-owned
  docs.root-owned
  migrations.root-owned
  platform.root-owned
  scripts.root-owned
  target.root-owned
  tests.root-owned
  tools.root-owned
)

existing=()
for d in "${ROOT_OWNED_DIRS[@]}"; do
  if [[ -e "$d" ]]; then
    existing+=("$d")
  fi
done

if [[ ${#existing[@]} -eq 0 ]]; then
  echo "No *.root-owned directories found."
  exit 0
fi

echo "Removing *.root-owned directories:"
printf ' - %s\n' "${existing[@]}"

if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
  sudo rm -rf -- "${existing[@]}"
elif [[ -x /mnt/c/Windows/System32/wsl.exe ]]; then
  # WSL fallback: run as root via wsl.exe (does not require sudo password).
  /mnt/c/Windows/System32/wsl.exe -u root -- bash -lc \
    "cd \"$(pwd)\" && rm -rf -- .apps.root-owned .configs.root-owned .crates.root-owned .docs.root-owned .github.root-owned .migrations.root-owned .scripts.root-owned .serena.root-owned .target.root-owned .tests.root-owned .tools.root-owned .platform.root-owned apps.root-owned configs.root-owned crates.root-owned docs.root-owned migrations.root-owned platform.root-owned scripts.root-owned target.root-owned tests.root-owned tools.root-owned"
elif command -v docker >/dev/null 2>&1 && docker version >/dev/null 2>&1; then
  # Use a root container to delete root-owned dirs when sudo isn't available.
  docker run --rm -v "$(pwd):/work" -w /work alpine:3.19 sh -lc \
    'rm -rf -- .apps.root-owned .configs.root-owned .crates.root-owned .docs.root-owned .github.root-owned .migrations.root-owned .scripts.root-owned .serena.root-owned .target.root-owned .tests.root-owned .tools.root-owned .platform.root-owned apps.root-owned configs.root-owned crates.root-owned docs.root-owned migrations.root-owned platform.root-owned scripts.root-owned target.root-owned tests.root-owned tools.root-owned'
else
  echo "Error: need passwordless sudo, WSL root (wsl.exe), or docker to remove root-owned directories." >&2
  exit 1
fi

echo "Done."
