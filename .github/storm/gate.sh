#!/usr/bin/env bash
# Sets the storm/review check that the ruleset on main requires. No model runs
# here: the agent that opened the pull request reviews it in its own session
# (the `storm` skill) and ends by commenting `Review finished at <sha>`.
#
#   EVENT=push     a new head: pending, unless it is the change last reviewed
#                  with only the base moved (BEFORE is the previous head).
#   EVENT=comment  BODY from AUTHOR: `Review finished at <sha>` naming the head,
#                  or `/storm:ready`, sets it to success; `/storm:stop` labels
#                  the pull request and turns off auto-merge.
#
# Runs from a checkout of main with the pull request's head in HEAD_DIR and
# origin/main fetched there. Needs REPO, PR, GH_TOKEN and EVENT. With
# DRY_RUN=1, writes to GitHub are printed instead of sent.
set -euo pipefail

: "${REPO:?}" "${PR:?}" "${EVENT:?}"
HEAD_DIR=${HEAD_DIR:-head}
CHECK_NAME='storm/review'
STOP_LABEL='storm:stopped'

# Runs gh for a write, or prints it under DRY_RUN.
gh_write() {
  if [[ ${DRY_RUN:-0} == 1 ]]; then
    printf 'DRY_RUN gh' >&2
    printf ' %q' "$@" >&2
    printf '\n' >&2
    return 0
  fi
  gh "$@"
}

# Records the check on a commit: check <sha> <in_progress|success> <title> <summary>.
check() {
  local sha=$1 state=$2 title=$3 summary=$4
  local args=(-f name="$CHECK_NAME" -f head_sha="$sha" -f "output[title]=$title" -f "output[summary]=$summary")
  if [[ $state == in_progress ]]; then
    args+=(-f status=in_progress)
  else
    args+=(-f status=completed -f conclusion="$state")
  fi
  gh_write api -X POST "repos/$REPO/check-runs" "${args[@]}" >/dev/null
}

# Prints the patch id of a commit's change against main, or nothing when the
# commit is not in the clone or adds nothing.
patch_id() {
  local sha=$1 base
  git -C "$HEAD_DIR" cat-file -e "$sha^{commit}" 2>/dev/null ||
    git -C "$HEAD_DIR" fetch -q origin "$sha" 2>/dev/null || return 0
  base=$(git -C "$HEAD_DIR" merge-base origin/main "$sha") || return 0
  git -C "$HEAD_DIR" diff "$base" "$sha" | git patch-id --stable | cut -d' ' -f1
}

# The marker a success leaves in its summary, so a later push can tell whether
# only the base moved.
marker() {
  printf '\n\n<!-- storm-review patch=%s -->' "$(patch_id "$1")"
}

head=$(gh pr view "$PR" --repo "$REPO" --json headRefOid --jq .headRefOid)
short=${head:0:7}

if [[ $EVENT == push ]]; then
  before=${BEFORE:-}
  if [[ -n $before && ! $before =~ ^0+$ ]]; then
    last=$(gh api "repos/$REPO/commits/$before/check-runs?check_name=$CHECK_NAME" \
      --jq '[.check_runs[] | select(.status == "completed")] | sort_by(.completed_at) | last
        | select(.conclusion == "success") | .output.summary' || true)
    reviewed=$(sed -n 's/.*<!-- storm-review patch=\([0-9a-f]*\) -->.*/\1/p' <<<"$last")
    if [[ -n $reviewed && $reviewed == "$(patch_id "$head")" ]]; then
      check "$head" success "Same change as ${before:0:7}" \
        "Only the base moved since ${before:0:7}, so its review stands.$(marker "$head")"
      exit 0
    fi
  fi
  check "$head" in_progress "Waiting for review" \
    "The agent that opened this pull request reviews it and comments \`Review finished at $short\`. A person can accept it as it is with \`/storm:ready\`."
  exit 0
fi

line=$(head -n 1 <<<"${BODY:-}" | tr -d '\r')
author=${AUTHOR:-someone}
case $line in
  'Review finished at '*)
    named=$(awk '{print $4}' <<<"$line" | tr -d '`.')
    if [[ ${#named} -ge 7 && $head == "$named"* ]]; then
      check "$head" success "Reviewed at $short" \
        "@$author finished the review of $short.$(marker "$head")"
    else
      gh_write pr comment "$PR" --repo "$REPO" \
        --body "\`Review finished\` names \`$named\`, but the head is \`$short\`. Review the head and post it again."
    fi
    ;;
  /storm:ready*)
    check "$head" success "Marked ready by @$author" \
      "@$author accepted $short with \`/storm:ready\`.$(marker "$head")"
    ;;
  /storm:stop*)
    gh_write label create "$STOP_LABEL" --repo "$REPO" --force \
      --description 'No auto-merge or agent work until a person removes this label' >/dev/null
    gh_write pr edit "$PR" --repo "$REPO" --add-label "$STOP_LABEL" >/dev/null
    gh_write pr merge "$PR" --repo "$REPO" --disable-auto >/dev/null 2>&1 || true
    ;;
esac
