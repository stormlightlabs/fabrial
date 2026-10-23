# Helpers shared by the storm scripts. Source it; it expects REPO and GH_TOKEN.
# The scripts run from a checkout of main, so the change under review cannot
# alter its own reviewer. HEAD_DIR is the checkout of the pull request's head.
# With DRY_RUN=1, writes to GitHub are printed instead of sent.

HEAD_DIR=${HEAD_DIR:-head}

BOT_LOGIN='stormlight-thunderbot[bot]'
BOT_EMAIL='340296662+stormlight-thunderbot[bot]@users.noreply.github.com'
CHECK_NAME='storm/review'
MAX_ROUNDS=5
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

# Starts the storm/review check on a commit and prints its id.
check_start() {
  local sha=$1
  if [[ ${DRY_RUN:-0} == 1 ]]; then
    echo 0
    return
  fi
  gh api -X POST "repos/$REPO/check-runs" -f name="$CHECK_NAME" -f head_sha="$sha" \
    -f status=in_progress --jq .id
}

# Completes a check: check_finish <id> <success|failure> <title> <summary>.
check_finish() {
  local id=$1 conclusion=$2 title=$3 summary=$4
  gh_write api -X PATCH "repos/$REPO/check-runs/$id" -f status=completed \
    -f conclusion="$conclusion" -f "output[title]=$title" -f "output[summary]=$summary" >/dev/null
}

# Sets the check on a commit in one step: check_set <sha> <conclusion> <title> <summary>.
check_set() {
  local sha=$1 conclusion=$2 title=$3 summary=$4
  gh_write api -X POST "repos/$REPO/check-runs" -f name="$CHECK_NAME" -f head_sha="$sha" \
    -f status=completed -f conclusion="$conclusion" \
    -f "output[title]=$title" -f "output[summary]=$summary" >/dev/null
}

# Prints the marker of each review the bot posted on a pull request, oldest
# first, one per line: "sha=<sha> patch=<patch-id> verdict=<conclusion>".
review_markers() {
  local pr=$1
  gh api --paginate "repos/$REPO/pulls/$pr/reviews" \
    --jq ".[] | select(.user.login == \"$BOT_LOGIN\") | .body
      | capture(\"<!-- storm-review (?<m>[^>]*) -->\").m"
}

# Prints the value of one field of a marker: marker_field <marker> <name>.
marker_field() {
  sed -n "s/.*\\b$2=\\([^ ]*\\).*/\\1/p" <<<"$1"
}

# Prints the patch id of the change between main and HEAD, or nothing when the
# change is empty.
change_patch_id() {
  git -C "$HEAD_DIR" diff "$(git -C "$HEAD_DIR" merge-base origin/main HEAD)" HEAD |
    git patch-id --stable | cut -d' ' -f1
}

# Runs one Claude Code agent with a JSON schema and prints its structured
# output: run_agent <agent> <schema-file> <tools> <prompt-file> <transcript>.
run_agent() {
  local agent=$1 schema=$2 tools=$3 prompt=$4 out=$5
  claude -p --agent "$agent" --model "${STORM_MODEL:-opus}" \
    --max-budget-usd "${STORM_BUDGET_USD:-5}" --no-session-persistence \
    --output-format json --json-schema "$(cat "$schema")" \
    --allowedTools "$tools" <"$prompt" >"$out"
  if ! jq -e '.is_error == false and .structured_output != null' "$out" >/dev/null; then
    echo "$agent returned no structured output:" >&2
    jq -r '.result // .subtype // "no result"' "$out" >&2 || cat "$out" >&2
    return 1
  fi
  jq -c .structured_output "$out"
}

# Prints the model that did most of the work in a transcript.
agent_model() {
  jq -r '.modelUsage | to_entries | max_by(.value.outputTokens) | .key' "$1"
}
