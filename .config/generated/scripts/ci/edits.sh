#!/usr/bin/env bash
# Generated CI check script — one of the check-script family under
# <config>/generated/scripts/ci/. The configuration sync owns these
# bytes: hand edits are overwritten at the next sync. To take
# control, move the file to the mirrored override home
# <config>/scripts/ci/edits.sh (it wins over the generated variant).
set -euo pipefail
_CI_HOME=$(cd -- "$(dirname -- "$0")" && pwd -P)
_CI_CONF_DIR=$(cd -- "$_CI_HOME/../../.." && pwd -P)
_CI_ROOT=$(cd -- "$_CI_CONF_DIR/.." && pwd -P)
# shellcheck disable=SC2034
CI_SCRIPT=edits
# shellcheck disable=SC2034
CI_JSON=0
# shellcheck disable=SC2034
CI_WARN_MISSING=0
# shellcheck disable=SC2034
CI_WARN_EMPTY=0
# shellcheck disable=SC2034
CI_SKIP_BOOTSTRAP=0
# shellcheck disable=SC2034
CI_ONLY=""
if [ ! -f "$_CI_HOME/_lib.sh" ] || [ ! -f "$_CI_HOME/manifest.sh" ]; then
    printf '%s\n' "ERROR: missing _lib.sh or manifest.sh beside $0 — torn sync; re-run the configuration sync" >&2
    exit 3
fi
# shellcheck source=./_lib.sh disable=SC1091
. "$_CI_HOME/_lib.sh"
# shellcheck source=./manifest.sh disable=SC1091
. "$_CI_HOME/manifest.sh"
ci_check_stamp
#
# Exit codes: 0 pass (a distinct EMPTY verdict for legitimately empty
# scope included), 1 findings, 2 usage, 3 environment/precondition
# (stale stamp, missing dep, failed bootstrap, bad topology, torn
# sync), 70 vacuous-should-match (--warn-empty downgrades to 0 with a
# warning banner).
_usage='usage: edits.sh [--json] [--stages name[,name...]] [--warn-empty] [--warn-missing] [--skip-bootstrap]'
# shellcheck disable=SC2034
while [ "$#" -gt 0 ]; do
    case $1 in
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
_conflicted=$(git -C "$_CI_ROOT" ls-files -u 2>/dev/null || true)
if [ -n "$_conflicted" ]; then
    ci_json "$CI_SCRIPT" "__collect__" "env" 3 "merge conflict in progress"
    ci_die "$CI_EX_ENV" "ERROR: unresolved merge conflicts present — resolve them first; conflicted-content lint is meaningless"
fi
_tmp_staged=$(mktemp)
_tmp_unstaged=$(mktemp)
_tmp_other=$(mktemp)
trap 'rm -f "$_tmp_staged" "$_tmp_unstaged" "$_tmp_other"' EXIT
_collect_to() {
    _dst=$1
    shift
    _crc=0
    git -C "$_CI_ROOT" "$@" > "$_dst" 2>/dev/null || _crc=$?
    if [ "$_crc" -ne 0 ]; then
        sleep 1
        _crc=0
        git -C "$_CI_ROOT" "$@" > "$_dst" 2>/dev/null || _crc=$?
    fi
    return "$_crc"
}
if ! _collect_to "$_tmp_staged" diff --cached --name-only --diff-filter=ACMR -z; then
    ci_die "$CI_EX_ENV" "ERROR: staged collection failed twice (index contention or repository error)"
fi
if ! _collect_to "$_tmp_unstaged" diff --name-only --diff-filter=ACMR -z; then
    ci_die "$CI_EX_ENV" "ERROR: unstaged collection failed twice (index contention or repository error)"
fi
if ! _collect_to "$_tmp_other" ls-files --others --exclude-standard -z; then
    ci_die "$CI_EX_ENV" "ERROR: untracked collection failed twice (index contention or repository error)"
fi
CI_FILES=()
CI_SEEN=()
CI_RAW_N=0
CI_STAGED_N=0
CI_UNSTAGED_N=0
CI_OTHER_N=0
CI_DIVERGED=0
_add() {
    _stream=$1
    _p=$2
    CI_RAW_N=$((CI_RAW_N + 1))
    if [ "${#CI_SEEN[@]}" -gt 0 ]; then
        for _seen in "${CI_SEEN[@]}"; do
            if [ "$_seen" = "$_p" ]; then
                if [ "$_stream" = "unstaged" ]; then
                    CI_DIVERGED=$((CI_DIVERGED + 1))
                fi
                return 0
            fi
        done
    fi
    CI_SEEN+=("$_p")
    _abs="$_CI_ROOT/$_p"
    if [ -L "$_abs" ]; then
        printf 'SKIP %s (symlink — content lives at the target)\n' "$_p" >&2
        return 0
    fi
    if [ ! -e "$_abs" ]; then
        printf 'SKIP %s (staged content not in the worktree; ahead/full cover it after commit)\n' "$_p" >&2
        return 0
    fi
    if [ -d "$_abs" ]; then
        printf 'SKIP %s (directory or submodule — run checks inside that repo)\n' "$_p" >&2
        return 0
    fi
    if [ -f "$_abs" ]; then
        CI_FILES+=("$_p")
        return 0
    fi
    printf 'NOTE %s (typechange — the tool sees the current worktree kind)\n' "$_p" >&2
    CI_FILES+=("$_p")
}
while IFS= read -r -d '' _p; do
    CI_STAGED_N=$((CI_STAGED_N + 1))
    _add staged "$_p"
done < "$_tmp_staged"
while IFS= read -r -d '' _p; do
    CI_UNSTAGED_N=$((CI_UNSTAGED_N + 1))
    _add unstaged "$_p"
done < "$_tmp_unstaged"
while IFS= read -r -d '' _p; do
    CI_OTHER_N=$((CI_OTHER_N + 1))
    _add untracked "$_p"
done < "$_tmp_other"
_porcelain=$(git -C "$_CI_ROOT" status --porcelain 2>/dev/null || true)
if [ "$CI_RAW_N" -eq 0 ]; then
    if [ -n "$_porcelain" ]; then
        ci_json "$CI_SCRIPT" "__collect__" "env" 3 "porcelain non-empty but collected empty"
        ci_die "$CI_EX_ENV" "ERROR: git status reports changes but the collector found no entry — deletion-only changes are the usual cause; run full.sh or commit (collector self-check failed; refusing a vacuous green)"
    fi
    printf 'EMPTY SCOPE (staged %s / unstaged %s / untracked %s) — nothing to check\n' "$CI_STAGED_N" "$CI_UNSTAGED_N" "$CI_OTHER_N" >&2
    ci_json "$CI_SCRIPT" "__summary__" "empty-scope" 0 "no edits collected"
    exit "$CI_EX_OK"
fi
if [ "$CI_DIVERGED" -gt 0 ]; then
    printf 'NOTE %s file(s) have staged content that diverges from the worktree; the worktree content is what gets checked\n' "$CI_DIVERGED" >&2
fi
ci_pre_flight edits system
ci_bootstrap
ci_pre_flight edits project
ci_run_stages edits
ci_verdict "staged $CI_STAGED_N / unstaged $CI_UNSTAGED_N / untracked $CI_OTHER_N"
