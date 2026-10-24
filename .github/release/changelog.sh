#!/usr/bin/env bash
# Writes a release's section of CHANGELOG.md, run by the Release workflow:
#
#   changelog.sh <version> <date>
#
# The section lists the lines under Unreleased, then one line per commit since
# the last `v` tag behind HEAD, newest first, from its subject. A squash
# subject ends with its pull request number, so each line names the PR. A
# commit whose number a line under Unreleased already names gets no line of
# its own, and the Release commits themselves are left out. Unreleased stays
# as an empty heading above the new section. The owner edits the lines in the
# release pull request. Exits 1, changing nothing, when there is nothing to
# list.
set -euo pipefail

: "${1:?version}" "${2:?date}"
version=$1 date=$2

grep -qx '## Unreleased' CHANGELOG.md || { echo "CHANGELOG.md has no '## Unreleased' heading." >&2; exit 1; }
last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
range=HEAD
[[ -z $last ]] || range="$last..HEAD"

# The lines under Unreleased, without blank lines at either end.
written=$(awk '
  /^## Unreleased$/ { on = 1; next }
  on && /^## / { exit }
  on { lines[++n] = $0 }
  END {
    first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
    last = n; while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
    for (i = first; i <= last; i++) print lines[i]
  }' CHANGELOG.md)

generated=
while IFS= read -r subject; do
  [[ -n $subject ]] || continue
  [[ $subject =~ ^Release\ [0-9]+\.[0-9]+\.[0-9]+(\ \(#[0-9]+\))?$ ]] && continue
  if [[ $subject =~ \(#([0-9]+)\)$ ]] && grep -qE "#${BASH_REMATCH[1]}([^0-9]|$)" <<<"$written"; then
    continue
  fi
  generated+="- $subject"$'\n\n'
done < <(git log --format=%s "$range")

section=$written
if [[ -n $generated ]]; then
  [[ -z $section ]] || section+=$'\n\n'
  section+=${generated%$'\n\n'}
fi
if [[ -z $section ]]; then
  echo "Nothing to list: no commits since ${last:-the start} and nothing under Unreleased." >&2
  exit 1
fi

file=$(mktemp)
trap 'rm -f "$file"' EXIT
printf '%s\n' "$section" >"$file"
awk -v heading="## $version - $date" -v file="$file" '
  /^## Unreleased$/ {
    print; print ""; print heading; print ""
    while ((getline line < file) > 0) print line
    print ""
    skip = 1; next
  }
  skip && /^## / { skip = 0 }
  !skip { print }
' CHANGELOG.md >CHANGELOG.md.new
mv CHANGELOG.md.new CHANGELOG.md
