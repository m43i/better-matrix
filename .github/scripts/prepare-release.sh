#!/usr/bin/env bash
# Called from release.yml on main. Use a PR so the version update obeys branch protection.
set -euo pipefail

: "${VERSION:?Version to release is required}"
: "${GITHUB_REPOSITORY:?Repository is required}"
: "${GITHUB_OUTPUT:?Actions output file is required}"
: "${GITHUB_RUN_ID:?Actions run ID is required}"
: "${GITHUB_RUN_ATTEMPT:?Actions run attempt is required}"

num='(0|[1-9][0-9]*)'
part='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
if ! [[ "$VERSION" =~ ^$num\.$num\.$num(-$part(\.$part)*)?$ ]]; then
  echo "::error::\"$VERSION\" isn't a version like 1.2.0 or 1.2.0-rc.1"
  exit 1
fi

tag="v$VERSION"
if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
  echo "::error::$tag exists already"
  exit 1
else
  # Exit 2 means no matching tag; authentication and network errors must stop the release.
  status=$?
  if [ "$status" -ne 2 ]; then exit "$status"; fi
fi
base=$(git rev-parse HEAD)
# A retry may start at a version-only release commit whose parent already passed CI.
if [[ "$(git log -1 --format=%s)" == "Release v"* ]] && [ "$(git diff --name-only HEAD~1 HEAD | sort | tr '\n' ' ')" = "Cargo.lock Cargo.toml " ]; then
  base=$(git rev-parse HEAD~1)
fi

sed -i '/^\[workspace.package\]/,/^\[/s/^version = ".*"/version = "'"$VERSION"'"/' Cargo.toml
for crate in better-matrix-desktop matrix-core; do
  sed -i '/^name = "'"$crate"'"$/{n;s/^version = ".*"/version = "'"$VERSION"'"/}' Cargo.lock
done
python3 - <<'PY'
import os, tomllib
version = os.environ["VERSION"]
with open("Cargo.toml", "rb") as file:
    manifest = tomllib.load(file)
assert manifest["workspace"]["package"]["version"] == version, "Cargo.toml"
with open("Cargo.lock", "rb") as file:
    lock = {p["name"]: p["version"] for p in tomllib.load(file)["package"]}
for crate in ("better-matrix-desktop", "matrix-core"):
    assert lock[crate] == version, f"Cargo.lock: {crate}"
PY

sha=$(git rev-parse HEAD)
if ! git diff --quiet; then
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  git commit -m "Release $tag" -- Cargo.toml Cargo.lock
  prepared=$(git rev-parse HEAD)
  branch="release/$tag-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
  git push origin "HEAD:refs/heads/$branch"
  pr=$(gh pr create --repo "$GITHUB_REPOSITORY" --base main --head "$branch" \
    --title "Release $tag" --label skip-changelog \
    --body "Set the workspace version to $VERSION for this release. The release workflow runs lint and tests unless the unchanged code already passed CI on main.")
  # No bypass or auto-merge: fail here if branch rules require more review or checks.
  gh pr merge "$pr" --repo "$GITHUB_REPOSITORY" --squash \
    --match-head-commit "$prepared" --delete-branch
  sha=$(gh pr view "$pr" --repo "$GITHUB_REPOSITORY" --json state,mergeCommit \
    --jq 'if .state == "MERGED" then .mergeCommit.oid else empty end')
  if [ -z "$sha" ]; then
    echo "::error::The version PR has not merged: $pr"
    exit 1
  fi
  git fetch --depth=2 origin "$sha"
  # main may have advanced while the PR was open. Reuse CI only for the exact prepared tree.
  if ! git diff --quiet "$prepared" "$sha"; then base="$sha"; fi
fi
{ echo "tag=$tag"; echo "sha=$sha"; echo "base=$base"; } >> "$GITHUB_OUTPUT"
