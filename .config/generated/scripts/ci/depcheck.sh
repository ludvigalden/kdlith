#!/usr/bin/env bash
# Generated CI check script — one of the check-script family under
# <config>/generated/scripts/ci/. The configuration sync owns these
# bytes: hand edits are overwritten at the next sync. To take
# control, move the file to the mirrored override home
# <config>/scripts/ci/depcheck.sh (it wins over the generated variant).
set -euo pipefail
_CI_HOME=$(cd -- "$(dirname -- "$0")" && pwd -P)
_CI_CONF_DIR=$(cd -- "$_CI_HOME/../../.." && pwd -P)
_CI_ROOT=$(cd -- "$_CI_CONF_DIR/.." && pwd -P)
# shellcheck disable=SC2034
CI_SCRIPT=depcheck
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
_usage='usage: depcheck.sh [--json] [--for full|edits|ahead|depcheck]'
CI_FOR=""
# shellcheck disable=SC2034
while [ "$#" -gt 0 ]; do
    case $1 in
        --for)
            shift
            [ "$#" -gt 0 ] || ci_usage_exit "$_usage"
            CI_FOR=$1
            ;;
        --for=*) CI_FOR=${1#--for=} ;;
        --json) CI_JSON=1 ;;
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
if [ "$CI_TOOL_COUNT" -eq 0 ]; then
    printf 'EMPTY: no dependencies declared — nothing to verify\n' >&2
    ci_json "$CI_SCRIPT" "__summary__" "empty" 0 "no dependencies declared"
    exit "$CI_EX_OK"
fi
_fail=0
_rows=0
printf '%-18s %-9s %-16s %-16s %s\n' NAME REQUIRED FOUND STATUS INSTALL >&2
_i=1
while [ "$_i" -le "$CI_TOOL_COUNT" ]; do
    _nvar="CI_TOOL_${_i}_NAME"; _name=${!_nvar}
    _tvar="CI_TOOL_${_i}_TIER"; _tier=${!_tvar}
    _svar="CI_TOOL_${_i}_SCOPE"; _scope=${!_svar}
    _dvar="CI_TOOL_${_i}_DIR"; _pdir=${!_dvar}
    _vvar="CI_TOOL_${_i}_VERSION_CMD"; _vcmd=${!_vvar}
    _rvar="CI_TOOL_${_i}_VERSION_REGEX"; _regex=${!_rvar}
    _mvar="CI_TOOL_${_i}_MIN_VERSION"; _min=${!_mvar}
    _lvar="CI_TOOL_${_i}_INSTALL_LINUX"; _lins=${!_lvar}
    _dvar2="CI_TOOL_${_i}_INSTALL_DARWIN"; _dins=${!_dvar2}
    _uvar="CI_TOOL_${_i}_USEDBY"; _usedby=${!_uvar}
    _row=$_i
    _i=$((_i + 1))
    if [ -n "$CI_FOR" ] && ! ci_used_by "$_usedby" "$CI_FOR"; then
        continue
    fi
    case $(uname -s) in
        Darwin) _install=$_dins ;;
        *) _install=$_lins ;;
    esac
    [ -n "$_install" ] || _install="install $_name (see the repository dependency manifest)"
    _bin=$(command -v "$_name" 2>/dev/null || true)
    if [ -z "$_bin" ] && [ "$_scope" = "project" ]; then
        if ci_project_tool_present "$_row"; then
            _bin=present
        fi
    fi
    _required=yes
    case $_tier in
        lazy | optional) _required=$_tier ;;
    esac
    _found=""
    _status=""
    if [ -z "$_bin" ]; then
        if [ "$_scope" = "project" ]; then
            _status="missing (run bootstrap)"
        else
            _status="missing"
        fi
    else
        _argv=()
        IFS=$'\x1f' read -r -a _argv <<< "$_vcmd"
        _vout=""
        if [ "$_scope" = "project" ] && [ -n "$_pdir" ]; then
            _vpath=""
            [ -d "$_CI_ROOT/$_pdir/node_modules/.bin" ] && _vpath="$_CI_ROOT/$_pdir/node_modules/.bin"
            if [ -d "$_CI_ROOT/$_pdir/.venv/bin" ]; then
                if [ -n "$_vpath" ]; then
                    _vpath="$_vpath:$_CI_ROOT/$_pdir/.venv/bin"
                else
                    _vpath="$_CI_ROOT/$_pdir/.venv/bin"
                fi
            fi
            if [ -n "$_vpath" ]; then
                _vout=$(PATH="$_vpath:$PATH" "${_argv[@]}" 2>/dev/null || true)
            else
                _vout=$("${_argv[@]}" 2>/dev/null || true)
            fi
        else
            _vout=$("${_argv[@]}" 2>/dev/null || true)
        fi
        _vfirst=${_vout%%$'\n'*}
        _vm=$(printf '%s' "$_vfirst" | grep -E -o "$_regex" 2>/dev/null || true)
        _vhit=${_vm%%$'\n'*}
        if [ -n "$_vhit" ]; then
            _found=$_vhit
            _status=ok
            if [ -n "$_min" ]; then
                if ! ci_version_ge "$_found" "$_min"; then
                    _status="below-min (< $_min)"
                fi
            fi
        elif [ -n "$_vfirst" ]; then
            _found=${_vfirst:0:24}
            _status=unknown-version
            if [ -n "$_min" ]; then
                _status="unknown-version (needs >= $_min)"
            fi
        else
            _status="found (no version output)"
        fi
    fi
    case $_status in
        missing* | below-min* | unknown-version*)
            if [ "$_required" = "yes" ]; then
                _fail=$((_fail + 1))
            fi
            ;;
    esac
    printf '%-18s %-9s %-16s %-16s %s\n' "$_name" "$_required" "${_found:--}" "$_status" "$_install" >&2
    case $_status in
        ok) _jstatus=pass ;;
        missing* | below-min*) _jstatus=fail ;;
        *) _jstatus=warn ;;
    esac
    ci_json "$CI_SCRIPT" "$_name" "$_jstatus" 0 "required=$_required found=${_found:--}"
    _rows=$((_rows + 1))
done
printf 'SUMMARY depcheck: %s tool(s) checked, %s required problem(s)\n' "$_rows" "$_fail" >&2
if [ "$_fail" -gt 0 ]; then
    ci_json "$CI_SCRIPT" "__summary__" "findings" 1 "required problems=$_fail"
    exit "$CI_EX_FINDINGS"
fi
ci_json "$CI_SCRIPT" "__summary__" "ok" 0 "all present"
exit "$CI_EX_OK"
