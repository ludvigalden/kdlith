#!/usr/bin/env bash
# Generated CI check script — one of the check-script family under
# <config>/generated/scripts/ci/. The configuration sync owns these
# bytes: hand edits are overwritten at the next sync. To take
# control, move the file to the mirrored override home
# <config>/scripts/ci/full.sh (it wins over the generated variant).
set -euo pipefail
_CI_HOME=$(cd -- "$(dirname -- "$0")" && pwd -P)
_CI_CONF_DIR=$(cd -- "$_CI_HOME/../../.." && pwd -P)
_CI_ROOT=$(cd -- "$_CI_CONF_DIR/.." && pwd -P)
CI_SCRIPT=full
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
_usage='usage: full.sh [--json] [--stages name[,name...]] [--warn-empty] [--warn-missing] [--skip-bootstrap]'
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
ci_pre_flight full system
ci_bootstrap
ci_pre_flight full project
ci_run_stages full
if [ "$CI_AUTHOR_COUNT" -gt 0 ]; then
    ci_check_authors --all
    CI_RAN=$((CI_RAN + 1))
fi
ci_verdict "declared stages"
