---
description: Review an open PR against this repo's stated rules and post the findings as inline comments
argument-hint: [PR number | sweep]
allowed-tools: Bash(./tools/gh-api.sh:*), Bash(git:*), Bash(jq:*), Bash(gh:*), Read, Grep, Glob, Write, mcp__github-reviewer__pull_request_review_write, mcp__github-reviewer__add_comment_to_pending_review, mcp__github-reviewer__pull_request_read
---

# PR review against this repo's rules

`$ARGUMENTS` is a PR number, or `sweep` (the default when it is empty) to do every open PR that has
not been reviewed at its current head.

A webhook run arrives with a `<github-trigger-context>` block naming the event and PR, and its
scope is **that PR alone**. A run with no trigger context (the daily cron, or `sweep` by hand)
reviews every open PR, and that sweep is the net for a webhook that never arrived. Sweeping on every
event is what used to post the same review several times: a push to one PR re-reviewed all the
others while their own events' runs were doing the same.

If the event is `pull_request.edited`, stop without reviewing anything. An edit to the title or body
carries no commits, and it usually lands seconds after a push whose own run is already reviewing
that head. The cost is that a title changed after its head was reviewed isn't looked at again until
the next push.

The trigger context gets two things wrong here:

- It offers `gh pr view`, which does not exist in this sandbox.
- Its checkout has **only the PR's own branch fetched**, so `git checkout main` fails until you
  `git fetch origin main` first. Whether that checkout is a local branch or a detached HEAD varies
  run to run, so do not write a procedure that depends on which.

Do not review a PR the event says is `closed`; the open-PR listing below already excludes it.

You are reviewing, not authoring. **Never** push a commit, amend a branch, merge, close, or approve.
You post exactly one review per PR head, `REQUEST_CHANGES` when it has a `bug` finding and `COMMENT`
otherwise. The one other write is dismissing our own earlier `REQUEST_CHANGES` once its bugs are
gone (step 5, "Dismiss our earlier blocking reviews"), and nothing else changes.

Repo: `aivanyuk/busy` (public, so a PR can come from anyone). The rubric is
`.github/review-rules.md`; it is an index into `CLAUDE.md` and `docs/`, which are the authority.

**Do not build or run anything.** busy is a Windows-only Rust app on the `windows` crate, and the
cloud sandbox is Linux: `cargo` there proves nothing about it. Whether it compiles, lints and passes
its tests is CI's job (`.github/workflows/ci.yml`, on `windows-latest`), not yours.

**Read through `tools/gh-api.sh`, not `gh`.** The cloud sandbox this runs in has git credentials and
no `gh` binary. The script is `gh api` reduced to what a review needs, over whatever token the host
has. Anything below written as `gh-api.sh <path>` is `./tools/gh-api.sh <path>` from the repository
root. It exits non-zero on an HTTP error and prints the status and body, so never treat a failed
call as "nothing found".

**Post through the `github-reviewer` MCP connector.** Reads are the same whoever makes them, but the
one write this procedure makes carries an identity, and `gh-api.sh` cannot carry one here. Step 5
says how.

## 1. Pick the PRs

A webhook run takes the PR its event names. Read it fresh from
`./tools/gh-api.sh "repos/aivanyuk/busy/pulls/<N>"`, because the event can be stale: the PR may be
closed, a draft, or at a newer head by now. A sweep lists them all:

```bash
./tools/gh-api.sh "repos/aivanyuk/busy/pulls?state=open&per_page=50" |
    jq -r '.[] | "\(.number)\t\(.head.sha)\tdraft=\(.draft)\t\(.title)"'
```

Labels and the body come from the same objects (`.labels[].name`, `.body`); read them from the JSON
rather than fetching again.

Skip a PR when it is a draft, when it carries the label `skip-review`, or when it has already been
reviewed at this exact head. The marker for that is a review comment of ours containing
`<!-- claude-pr-review sha=<headRefOid> -->`:

```bash
./tools/gh-api.sh "repos/aivanyuk/busy/pulls/<N>/reviews" |
    jq -r '.[].body' | grep -c "claude-pr-review sha=<head-sha>"
```

A PR that was reviewed at an older head **is** reviewed again, but only the commits added since
that review are in scope. Say so in the summary. Find that range with the SHA in the previous
marker: `git diff <old-sha>..<head-sha>`.

If nothing qualifies, say so in one line and stop. Do not post anything.

### Claim a PR before reading it

Two runs can be live at once: a push and the cron, or two pushes a minute apart. A review takes
minutes, so a marker check alone can't stop both runs from posting. Claim each PR through the
connector before step 2:

`mcp__github-reviewer__pull_request_review_write` with `method: "create"`, `owner: "aivanyuk"`,
`repo: "busy"`, `pullNumber: <N>`, `commitID: "<head-sha>"` and **no `event`**.

That opens a pending review, and GitHub lets one account hold only one pending review per PR, so the
call is a lock: while another run holds it, `create` is refused with "User can only have one pending
review per pull request". A pending review has no timestamp, and the connector drops a pending
review's body, so the only thing a claim records is the commit it was made for. On a refusal, read
that with `mcp__github-reviewer__pull_request_read`, `method: "get_reviews"`: the connector's account
sees its own pending review, `state: "PENDING"`, with its `commit_id`. `gh-api.sh` reads as a
different account and doesn't see it.

- **A webhook run** skips the PR when the claim's `commit_id` is the current head, because another
  run is reviewing it. A claim on an older head belongs to a run that will drop it at step 5, or
  that died holding it. Either way, clear it with `method: "delete_pending"` and claim once more. A
  second refusal means skip.
- **A sweep** (no trigger context) takes over any claim the same way. That is how a claim left by a
  run that died on the current head gets cleared, at worst a day later.

**Record your claim's `id`.** Right after a successful `create`, call `get_reviews` and note the
`id` of the `PENDING` review; `create` itself doesn't return it. Every run posts as the same account,
so the pending slot doesn't say whose it is, and the `id` is the only way to tell. Before each later
`delete_pending` or `submit_pending`, read `get_reviews` again. If the pending review's `id` isn't
yours, or nothing is pending, another run has taken your claim over. Stop without touching the slot,
and report the PR as skipped. A run that ignored this could delete or submit the new holder's claim.

The pending review you opened is the one step 5 fills and submits. **Never leave it behind:** if you
stop before submitting, for whatever reason (nothing to post, the head moved, an error), release it
with `delete_pending` after the `id` check above. Until the next sweep, a leftover claim on the current head keeps that head
from being reviewed.

## 2. Read the change

```bash
git fetch --no-tags origin "pull/<N>/head" && git checkout --detach FETCH_HEAD
./tools/gh-api.sh --accept diff "repos/aivanyuk/busy/pulls/<N>"
```

(`git diff main...FETCH_HEAD` says the same thing and is cheaper if the API diff is truncated.)

Read the diff first, then open the **whole** files it touches, because a rule is nearly always
broken by what the diff does *to* its surroundings, and a hunk alone cannot show that. Read the PR
body: it states the intent the change is judged against, and what its author claims to have
verified.

Read the commits too (`git log --reverse --format='%h %s%n%b' main..FETCH_HEAD`). PRs here are
rebase-merged, so every commit lands on `main` as it stands and each is meant to build and do one
reviewable thing (`docs/commits.md`); a commit that only makes sense with the next one is a finding
even when the PR as a whole is fine.

## 3. Collect the rules that apply

Read `.github/review-rules.md` and pick the rule IDs whose *Trigger* column the diff matches. Then
**open the authority passage behind every rule you picked** (`CLAUDE.md`, `docs/*.md`,
`docs/areas/*.md`, the script headers in `tools/`). This step decides whether the
review is worth reading: those passages carry the exceptions, and most of these rules have one. A
finding against a rule whose exception the diff is using is worse than no finding.

Never flag anything under the rubric's "Already enforced" section. A script decides those exactly.

## 4. Judge

For each finding: the rule ID (or `bug` for something wrong regardless of any rule), the file and
line **in the new file**, what the code does, and the smallest fix. Then check it against these,
because a reviewer nobody trusts gets muted:

- Is it in the diff? Pre-existing code is not this PR's problem, unless the diff makes it worse.
- Can you point at the line that is wrong, and at the passage that says so? If either is missing,
  it is a `nit` or nothing.
- Would the author's own reason survive your objection? If the diff or a comment already explains
  the departure, that satisfies P3 and there is no finding.
- Is it about the code, or about how you would have written it? The second is a `nit`, and a review
  carries at most one or two of those.

Cap at **10 findings**, ordered `bug`, then `rule`, then `question`, then `nit`. If a change breaks
more rules than that, say so in the summary rather than listing 40 comments.

## 5. Post one review

Inline comments anchor to lines the diff touches; anything you cannot anchor goes in the body
instead.

**Pick the event first.** `REQUEST_CHANGES` if and only if step 4 left at least one `bug` finding:
something wrong regardless of any rule, that you can point at and that would ship broken. Rule
findings, questions and nits alone are `COMMENT`, because a rule departure is the author's call and
a bug is not. Below, `<event>` is that choice.

**Post through the connector, not `gh-api.sh`.** In the cloud sandbox an egress proxy replaces the
`Authorization` header on every `api.github.com` request, so a review posted with `gh-api.sh` is
attributed to whichever account the routine's environment is connected as, whatever token the script
found (`tools/gh-api.sh`'s header has the measurements). The connector is a separate route carrying
its own GitHub identity, which is how a review comes from the reviewer's account rather than a
maintainer's.

**Check again right before posting.** Re-read the PR's head with `gh-api.sh` and its reviews for
the marker. If the head has moved since your claim, release your claim (`delete_pending`, after the `id` check
in step 1) and skip: the push that moved it fired its own run, and your review would be attached to
a commit you never read. If the marker for your head is already there, release your claim and stop.

Then confirm the pending review is still yours (its `id`, step 1), and fill and submit it. It is
already pinned to the head through `commitID`:

1. `mcp__github-reviewer__add_comment_to_pending_review` once per finding, with `path`, `line` (the
   line **in the new file**), `side: "RIGHT"`, `subjectType: "LINE"` and `body`.
2. `mcp__github-reviewer__pull_request_review_write` with `method: "submit_pending"`,
   `event: "<event>"`, and `body` set to the summary ending in the marker. The claim was only a
   placeholder in the body, and this replaces it.

A review with no inline comments is step 2 alone. Don't `create` a second review: the claim holds
the one pending slot, so `create` would be refused.

Running `/pr-review` by hand on a dev machine, where the connector is not attached, skip the claim
(you are the only run) and post with `gh-api.sh` instead. There is no proxy there, so it carries whatever token the script finds:

```bash
cat > /tmp/review.json <<'JSON'
{
  "event": "<event>",
  "body": "…summary…\n\n<!-- claude-pr-review sha=<headRefOid> -->",
  "comments": [
    {"path": "crates/sensors/src/…/wmi.rs", "line": 42, "side": "RIGHT",
     "body": "**bug** — this WMI query runs on the taskbar window's thread…"}
  ]
}
JSON
./tools/gh-api.sh --method POST "repos/aivanyuk/busy/pulls/<N>/reviews" --input /tmp/review.json
```

Rules for the review, whichever route posted it:

- `event` is `REQUEST_CHANGES` when there is a `bug` finding and `COMMENT` otherwise. Never
  `APPROVE`: a model reading a diff is not a second gate, and CI plus the manual checks in
  `docs/testing.md` still say a change is done.
- Our earlier `REQUEST_CHANGES` on an older head stays in force after a `COMMENT` re-review, because
  GitHub counts each reviewer's latest non-comment review. When this review resolves it, open the
  summary by saying so, then dismiss it as the next section says.
- The body ends with the marker, which is what makes a re-run idempotent. Without it the next sweep
  reviews the same head again. The marker covers runs that come after; the claim covers runs that
  overlap.
- Each comment body opens with the rule ID in bold, then one or two sentences and, where it helps, a
  two-line suggested diff. No preamble, no restating the code.
- The summary body is short: a first line with the decision (`Requesting changes: N bug findings`
  or `Comments only`), what the PR does, whether it holds against the rules that apply, and the
  counts by severity. Name a rule you checked and found satisfied only when the PR is the kind
  of change that usually breaks it. That is information; a checklist of everything is not.
- A clean PR still gets a review: one line saying what was checked and that nothing came up, because
  silence is indistinguishable from a broken pipeline.

If a comment's position is rejected (the line is outside the diff), move that one into the body
under "Not anchorable" and carry on with the rest; the pending review is still open, so nothing
needs redoing. If the submit itself is rejected, report the error rather than retrying blind.

### Dismiss our earlier blocking reviews

Only after the submit succeeded, and only when `<event>` was `COMMENT`. A new `REQUEST_CHANGES`
already supersedes the old one. List our blocking reviews: state `CHANGES_REQUESTED`, posted by
the reviewer's account and carrying the marker. The author check matters, because the marker is a
literal anyone can paste into a review. `<reviewer>` is the `user.login` on your claim in
`get_reviews` (`tmikx`), or, by hand, `./tools/gh-api.sh user | jq -r .login`:

```bash
./tools/gh-api.sh "repos/aivanyuk/busy/pulls/<N>/reviews" |
    jq -r '.[] | select(.state == "CHANGES_REQUESTED" and .user.login == "<reviewer>"
        and (.body | contains("claude-pr-review sha="))) | .id'
```

There can be more than one: a round that fixed one bug and brought in another posts a second
`REQUEST_CHANGES` without the first being dismissed. Do the following **for each id** the query
returns.

A re-review reads only the commits since the last one, so this is the moment to check that
review's own findings: read its `bug` comments
(`./tools/gh-api.sh "repos/aivanyuk/busy/pulls/<N>/reviews/<id>/comments"`) and the author's
replies, and hold each against the head. Dismiss it only when every one of them is fixed, or the
author has shown it was not a bug. If one still stands, leave that review in place and name the
finding in your report. Otherwise:

```bash
printf '{"message": "Resolved at %s: %s", "event": "DISMISS"}' "<head-sha>" "<new review URL>" > /tmp/dismiss.json
./tools/gh-api.sh --method PUT "repos/aivanyuk/busy/pulls/<N>/reviews/<id>/dismissals" --input /tmp/dismiss.json
```

This goes through `gh-api.sh`, not the connector, because the connector has no dismiss call. In the
cloud the proxy makes it the routine environment's account that dismisses, not the reviewer's, and
that is fine: GitHub lets any account with write access dismiss a review, and what has to come from
the reviewer's account is the review, not its dismissal. If a dismissal is refused, report its
status and body; the summary already tells the author the findings are resolved, so they can
dismiss it by hand.

## 6. Report

One line per PR: number, head SHA, the event posted, findings by severity, the review URL, and each
earlier blocking review you dismissed or left standing. If a PR was skipped, say which and why.
