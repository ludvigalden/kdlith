#!/usr/bin/env bash
# Generated CI check script — one of the check-script family under
# <config>/generated/scripts/ci/. The configuration sync owns these
# bytes: hand edits are overwritten at the next sync. To take
# control, move the file to the mirrored override home
# <config>/scripts/ci/ahead.sh (it wins over the generated variant).
set -euo pipefail
_CI_HOME=$(cd -- "$(dirname -- "$0")" && pwd -P)
_CI_CONF_DIR=$(cd -- "$_CI_HOME/../../.." && pwd -P)
_CI_ROOT=$(cd -- "$_CI_CONF_DIR/.." && pwd -P)
CI_SCRIPT=ahead
CI_JSON=0
CI_WARN_MISSING=0
CI_WARN_EMPTY=0
CI_SKIP_BOOTSTRAP=0
CI_ONLY=""
if [ ! -f "$_CI_HOME/_lib.sh" ] || [ ! -f "$_CI_HOME/manifest.sh" ]; then
    printf '%s\n' "ERROR: missing _lib.sh or manifest.sh beside $0 — torn sync; re-run the configuration sync" >&2
    exit 3
fi
. "$_CI_HOME/_lib.sh"
. "$_CI_HOME/manifest.sh"
ci_check_stamp
#
# Exit codes: 0 pass (a distinct EMPTY verdict for legitimately empty
# scope included), 1 findings, 2 usage, 3 environment/precondition
# (stale stamp, missing dep, failed bootstrap, bad topology, torn
# sync), 70 vacuous-should-match (--warn-empty downgrades to 0 with a
# warning banner).
_usage='usage: ahead.sh [--json] [--base <ref>] [--stages name[,name...]] [--warn-empty] [--warn-missing] [--skip-bootstrap]'
CI_BASE_ARG=""
while [ "$#" -gt 0 ]; do
    case $1 in
        --base)
            shift
            [ "$#" -gt 0 ] || ci_usage_exit "$_usage"
            CI_BASE_ARG=$1
            ;;
        --base=*) CI_BASE_ARG=${1#--base=} ;;
        --json) CI_JSON=1 ;;
        --warn-empty) CI_WARN_EMPTY=1 ;;
        --warn-missing) CI_WARN_MISSING=1 ;;
        --skip-bootstrap) CI_SKIP_BOOTSTRAP=1 ;;
        --stages)
            shift
            [ "$#" -gt 0 ] || ci_usage_exit "$_usage"
            CI_ONLY=$(printf '%s' "$1" | tr ',' ' ')
            ;;
        --stages=*) CI_ONLY=$(printf '%s' "${1#--stages=}" | tr ',' ' ') ;;
        -h | --help)
            printf '%s\n' "$_usage"
            exit 0
            ;;
        *)
            printf 'ERROR: unknown argument: %s\n' "$1" >&2
            ci_usage_exit "$_usage"
            ;;
    esac
    shift
done
_rb=$(git -C "$_CI_ROOT" rev-parse --git-path rebase-merge 2>/dev/null || true)
_ra=$(git -C "$_CI_ROOT" rev-parse --git-path rebase-apply 2>/dev/null || true)
if { [ -n "$_rb" ] && [ -e "$_rb" ]; } || { [ -n "$_ra" ] && [ -e "$_ra" ]; }; then
    ci_die "$CI_EX_ENV" "ERROR: an interactive rebase or cherry-pick is in progress — range semantics are unreliable; finish it first"
fi
_resolved=""
_trace=""
if [ -n "$CI_BASE_ARG" ]; then
    _resolved=$CI_BASE_ARG
    _trace=" explicit --base"
elif [ -n "$CI_BASE_DEFAULT" ] && git -C "$_CI_ROOT" rev-parse --verify --quiet "$CI_BASE_DEFAULT" >/dev/null 2>&1; then
    _resolved=$CI_BASE_DEFAULT
    _trace=" configured base"
else
    if [ -n "$CI_BASE_DEFAULT" ]; then
        _trace=" configured-base(unresolved)"
    fi
    if git -C "$_CI_ROOT" rev-parse --verify --quiet '@{u}' >/dev/null 2>&1; then
        _resolved='@{u}'
        _trace="$_trace upstream-tracking"
    elif _oh=$(git -C "$_CI_ROOT" symbolic-ref -q refs/remotes/origin/HEAD 2>/dev/null) && [ -n "$_oh" ]; then
        _resolved=$_oh
        _trace="$_trace origin/HEAD"
    elif git -C "$_CI_ROOT" rev-parse --verify --quiet refs/remotes/origin/main >/dev/null 2>&1; then
        _resolved=refs/remotes/origin/main
        _trace="$_trace origin/main"
    elif git -C "$_CI_ROOT" rev-parse --verify --quiet refs/remotes/origin/master >/dev/null 2>&1; then
        _resolved=refs/remotes/origin/master
        _trace="$_trace origin/master"
    fi
fi
if [ -z "$_resolved" ]; then
    ci_die "$CI_EX_ENV" "ERROR: no base branch resolved (tried:$_trace); pass --base <ref> or fetch the base branch"
fi
_shallow=$(git -C "$_CI_ROOT" rev-parse --is-shallow-repository 2>/dev/null || true)
if [ "$_shallow" = "true" ]; then
    case $_resolved in
        refs/remotes/*) _remote=$(printf '%s' "$_resolved" | cut -d / -f 3) ;;
        '@'*) _remote=origin ;;
        */*) _remote=${_resolved%%/*} ;;
        *) _remote=origin ;;
    esac
    if ! git -C "$_CI_ROOT" fetch --unshallow --quiet "$_remote" 2>/dev/null; then
        ci_die "$CI_EX_ENV" "ERROR: shallow clone cannot resolve merge-bases and the unshallow fetch from $_remote failed — use fetch-depth: 0 in CI, or run on a full checkout"
    fi
fi
_bases=$(git -C "$_CI_ROOT" merge-base --all HEAD "$_resolved" 2>/dev/null || true)
if [ -z "$_bases" ]; then
    ci_die "$CI_EX_ENV" "ERROR: no merge-base between HEAD and $_resolved — unrelated histories or a force-push; identify the correct base and pass it via --base"
fi
_tmp_commits=$(mktemp)
trap 'rm -f "$_tmp_commits"' EXIT
for _b in $_bases; do
    git -C "$_CI_ROOT" rev-list "$_b..HEAD" >> "$_tmp_commits" || ci_die "$CI_EX_ENV" "ERROR: rev-list failed for range $_b..HEAD"
done
sort -u -o "$_tmp_commits" "$_tmp_commits"
_count=$(wc -l < "$_tmp_commits" | tr -d ' ')
if [ "$_count" -eq 0 ]; then
    printf 'EMPTY RANGE (base %s..HEAD has no commits ahead) — nothing to check\n' "$_resolved" >&2
    ci_json "$CI_SCRIPT" "__summary__" "empty-range" 0 "everything merged"
    exit "$CI_EX_OK"
fi
_first=${_bases%%$'\n'*}
ci_pre_flight ahead system
ci_bootstrap
ci_pre_flight ahead project
CI_RAN=0
CI_FAILED=0
if [ "$CI_RANGE_SCAN" = "1" ]; then
    _cfg=$(ci_betterleaks_config)
    _rc=0
    ( cd "$_CI_ROOT" && betterleaks git . --config "$_cfg" --log-opts "$_first..HEAD" ) 1>&2 || _rc=$?
    if [ "$_rc" -eq 127 ] || [ "$_rc" -eq 126 ] || [ "$_rc" -eq 130 ]; then
        ci_json "$CI_SCRIPT" "history-scan" "env" "$_rc" "scan binary unrunnable"
        ci_die "$CI_EX_ENV" "ERROR: the range scan needs betterleaks, which is not runnable (exit $_rc); run depcheck.sh"
    fi
    if [ "$_rc" -eq 0 ]; then
        printf 'PASS history-scan (%s..%s)\n' "$_first" "HEAD" >&2
        ci_json "$CI_SCRIPT" "history-scan" "pass" 0 ""
    else
        printf 'FAIL history-scan (exit %s)\n' "$_rc" >&2
        ci_json "$CI_SCRIPT" "history-scan" "fail" "$_rc" "range scan findings"
        CI_FAILED=$((CI_FAILED + 1))
    fi
    CI_RAN=$((CI_RAN + 1))
fi
if [ "$CI_AUTHOR_COUNT" -gt 0 ]; then
    ci_check_authors "$_first..HEAD"
    CI_RAN=$((CI_RAN + 1))
fi
ci_verdict "range $_first..HEAD"
