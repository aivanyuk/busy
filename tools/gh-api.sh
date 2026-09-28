#!/usr/bin/env bash
# A minimum `gh api`, for hosts that have no gh. The automated PR reviewer runs both on a dev
# machine (gh installed and logged in) and in a cloud sandbox that has git credentials and no gh.
# The review procedure calls this instead of gh, so there is one code path, not two that drift.
#
#   tools/gh-api.sh repos/aivanyuk/busy/pulls                      # GET
#   tools/gh-api.sh --method POST repos/…/pulls/92/reviews --input review.json
#   tools/gh-api.sh --raw repos/…/pulls/92 --accept diff            # the PR's diff
#
# Token, first source that answers: $PR_REVIEWER_TOKEN, then $GH_TOKEN / $GITHUB_TOKEN, then
# `gh auth token`, then git's credential helper for github.com (what a cloud checkout is
# authenticated with). The token is never echoed, never written to disk and never passed on the
# command line: curl reads the header from a file descriptor, so it stays out of `ps` too.
#
# None of that decides who a review is posted as in the cloud routine. That sandbox reaches
# api.github.com through an egress proxy that replaces the Authorization header on every request:
# a call sending no header comes back authenticated anyway, and one sending a deliberately invalid
# token comes back 200 rather than 401. The identity is the routine environment's own GitHub
# connection, and no token this script picks can change it. PR_REVIEWER_TOKEN is worth setting only
# on a dev machine running /pr-review by hand, where it beats a personal `gh auth token`.
#
# Exits non-zero on an HTTP error, printing the status and body, because a reviewer that silently
# fails to post looks exactly like a reviewer that found nothing.
set -euo pipefail

method=GET
accept='application/vnd.github+json'
input=''
path=''

while [ $# -gt 0 ]; do
    case "$1" in
        --method | -X) method=$2; shift 2 ;;
        --input) input=$2; shift 2 ;;
        --accept) accept="application/vnd.github.$2" ; shift 2 ;;
        --raw) accept='application/vnd.github.raw'; shift ;;
        -*) echo "unknown flag: $1" >&2; exit 2 ;;
        *) path=$1; shift ;;
    esac
done
[ -n "$path" ] || { echo "usage: $0 [--method M] [--input FILE] [--accept TYPE] <api-path>" >&2; exit 2; }

token=''
if [ -n "${PR_REVIEWER_TOKEN:-}" ]; then
    token=$PR_REVIEWER_TOKEN
elif [ -n "${GH_TOKEN:-}" ]; then
    token=$GH_TOKEN
elif [ -n "${GITHUB_TOKEN:-}" ]; then
    token=$GITHUB_TOKEN
elif command -v gh >/dev/null 2>&1 && token=$(gh auth token 2>/dev/null); then
    :
else
    # git's own helper: whatever authenticated the clone. Not all helpers answer a bare query, so a
    # failure here is normal and falls through to the error below.
    token=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null |
        sed -n 's/^password=//p' || true)
fi
[ -n "$token" ] || {
    echo "$0: no GitHub token (tried PR_REVIEWER_TOKEN, GH_TOKEN, GITHUB_TOKEN, gh auth token, git credential fill)" >&2
    exit 3
}

set -- --silent --show-error \
    --request "$method" \
    --header "Accept: $accept" \
    --header 'X-GitHub-Api-Version: 2022-11-28' \
    --header "@/dev/fd/3" \
    --write-out '\n%{http_code}'
[ -n "$input" ] && set -- "$@" --data-binary "@$input" --header 'Content-Type: application/json'

# The header arrives on fd 3 so the token never appears in the process's arguments.
response=$(curl "$@" "https://api.github.com/${path#/}" 3<<<"Authorization: Bearer $token")
status=${response##*$'\n'}
body=${response%$'\n'*}

case "$status" in
    2*) [ -n "$body" ] && printf '%s\n' "$body"; exit 0 ;;
    *) printf 'HTTP %s\n%s\n' "$status" "$body" >&2; exit 1 ;;
esac
