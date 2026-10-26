#!/usr/bin/env bash
# Writes a release's section of CHANGELOG.md, run by the Release PR workflow:
#
#   changelog.sh <version> <date>
#
# The section lists one line per commit since the last release tag behind
# HEAD (`v` and a version, without a pre-release part), newest first, from its
# subject. A squash subject ends with its pull request number, so each line
# names the PR. Release commits are left out. The section goes above the
# newest release's, or at the end when there is none. The owner edits the
# lines in the release pull request. Exits 1, changing nothing, when there is
# nothing to list.
set -euo pipefail

: "${1:?version}" "${2:?date}"
version=$1 date=$2

last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' --exclude 'v*-*' 2>/dev/null || true)
range=HEAD
[[ -z $last ]] || range="$last..HEAD"

section=
while IFS= read -r subject; do
  [[ -n $subject ]] || continue
  [[ $subject =~ ^Release\ [0-9]+\.[0-9]+\.[0-9]+(\ \(#[0-9]+\))?$ ]] && continue
  section+="- $subject"$'\n\n'
done < <(git log --format=%s "$range")
if [[ -z $section ]]; then
  echo "Nothing to list: no commits since ${last:-the start}." >&2
  exit 1
fi

file=$(mktemp)
trap 'rm -f "$file"' EXIT
printf '## %s - %s\n\n%s' "$version" "$date" "$section" >"$file"
awk -v file="$file" '
  function insert() { while ((getline line < file) > 0) print line; done = 1 }
  !done && /^## / { insert() }
  { print }
  END { if (!done) { print ""; insert() } }
' CHANGELOG.md >CHANGELOG.md.new
mv CHANGELOG.md.new CHANGELOG.md
