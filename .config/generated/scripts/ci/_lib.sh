#!/usr/bin/env bash
# Check-script shared library, generated. Sourced by the entrypoints
# in this directory; never an entrypoint itself. The configuration
# sync owns these bytes — hand edits are overwritten at the next sync.
# Dialect: bash 3.2+ (macOS system bash safe). Scripts: bash <script>
# is the invocation convention; the exec bit is tracked in git.

CI_EX_OK=0
CI_EX_FINDINGS=1
CI_EX_USAGE=2
CI_EX_ENV=3
CI_EX_VACUOUS=70
CI_BATCH_SIZE=128


# ci_die prints the message to stderr and exits with the code.
ci_die() {
    _code=$1
    shift
    printf '%s\n' "$*" >&2
    exit "$_code"
}

# ci_json prints one NDJSON record on stdout when --json is active.
# Arguments: script stage status exit note.
ci_json() {
    [ "$CI_JSON" -eq 1 ] || return 0
    _ci_note=$(printf '%s' "$5" | tr '\n' ' ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')
    printf '{"script":"%s","stage":"%s","status":"%s","exit":%s,"note":"%s"}\n' \
        "$1" "$2" "$3" "$4" "$_ci_note"
}

# ci_sha256 prints the hex sha256 of $1 via the first available tool:
# GNU sha256sum, then BSD shasum, then openssl. macOS ships no
# sha256sum; the chain covers both hosts.
ci_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 -r "$1" | cut -d ' ' -f 1
    else
        return 1
    fi
}

# ci_check_stamp is the freshness self-check, run before argument
# parsing so even --help reports staleness. Any mismatch exits 3: the
# configuration inputs or the manifest changed since the last sync, or
# the sync was torn. The stamp covers the configuration inputs and the
# manifest only — a hand-edited generated script is caught by the sync
# drift report instead, never by this check.
ci_check_stamp() {
    local _stamp="$_CI_HOME/.stamp" _line _want_manifest="" _have_manifest
    if [ ! -f "$_stamp" ]; then
        ci_die "$CI_EX_ENV" "ERROR: .stamp is missing under $_CI_HOME — torn sync; re-run the configuration sync"
    fi
    _have_manifest=$(ci_sha256 "$_CI_HOME/manifest.sh") || ci_die "$CI_EX_ENV" "ERROR: no sha256 tool available (need sha256sum, shasum, or openssl)"
    while IFS= read -r _line || [ -n "$_line" ]; do
        case $_line in
            manifest=*) _want_manifest=${_line#manifest=} ;;
        esac
    done < "$_stamp"
    if [ "$_have_manifest" != "$_want_manifest" ]; then
        ci_die "$CI_EX_ENV" "ERROR: manifest.sh diverged from the freshness stamp — torn sync; re-run the configuration sync"
    fi
    while IFS= read -r _line || [ -n "$_line" ]; do
        case $_line in
            input:*)
                local _pair _fname _fhash _f _got
                _pair=${_line#input:}
                _fname=${_pair%%=*}
                _fhash=${_pair#*=}
                _f="$_CI_CONF_DIR/$_fname"
                if [ -f "$_f" ]; then
                    _got=$(ci_sha256 "$_f") || ci_die "$CI_EX_ENV" "ERROR: cannot hash $_f"
                    if [ "$_got" != "$_fhash" ]; then
                        ci_die "$CI_EX_ENV" "ERROR: $_CI_CONF_DIR/$_fname changed since the last sync; re-run the configuration sync to regenerate"
                    fi
                fi
                ;;
        esac
    done < "$_stamp"
}

# ci_used_by USEDBY SCRIPT — true when USEDBY is empty (every script)
# or space-lists SCRIPT.
ci_used_by() {
    if [ -z "$1" ]; then
        return 0
    fi
    local _w
    for _w in $1; do
        if [ "$_w" = "$2" ]; then
            return 0
        fi
    done
    return 1
}

# ci_pre_flight SCRIPT SCOPE — refuse the run when a tool the script
# requires is absent. System tools must answer on PATH and are checked
# BEFORE the bootstrap (the bootstrap itself needs them); project
# tools are checked AFTER the bootstrap that installs them, via the
# package-manager bin directories or the baked version command.
# Missing tools exit 3 unless --warn-missing downgraded the run.
ci_pre_flight() {
    local _script=$1 _scope_filter=$2 _i=1 _missing="" _nvar _name _svar _scope _uvar _usedby _bin
    while [ "$_i" -le "$CI_TOOL_COUNT" ]; do
        _nvar="CI_TOOL_${_i}_NAME"; _name=${!_nvar}
        _svar="CI_TOOL_${_i}_SCOPE"; _scope=${!_svar}
        _uvar="CI_TOOL_${_i}_USEDBY"; _usedby=${!_uvar}
        if [ "$_scope" != "$_scope_filter" ]; then
            _i=$((_i + 1))
            continue
        fi
        if ci_used_by "$_usedby" "$_script"; then
            _bin=$(command -v "$_name" 2>/dev/null || true)
            if [ -z "$_bin" ] && [ "$_scope" = "project" ]; then
                if ci_project_tool_present "$_i"; then
                    _bin=present
                fi
            fi
            if [ -z "$_bin" ]; then
                _missing="$_missing $_name"
            fi
        fi
        _i=$((_i + 1))
    done
    if [ -n "$_missing" ]; then
        if [ "$CI_WARN_MISSING" -eq 1 ]; then
            printf 'WARN: missing %s tools for %s (continuing):%s\n' "$_scope_filter" "$_script" "$_missing" >&2
            return 0
        fi
        printf 'ERROR: missing required %s tools for %s:%s\n' "$_scope_filter" "$_script" "$_missing" >&2
        printf '       install them, or re-run with --warn-missing to downgrade to a warning\n' >&2
        exit "$CI_EX_ENV"
    fi
}

# ci_project_tool_present INDEX — the project-scope presence probe:
# the package-manager bin directories (node_modules/.bin, .venv/bin)
# carry the binary, or the baked version command succeeds under a PATH
# that includes them. Returns 0 when present.
ci_project_tool_present() {
    local _i=$1 _dvar _pdir _vvar
    local -a _argv
    _dvar="CI_TOOL_${_i}_DIR"; _pdir=${!_dvar}
    if [ -x "$_CI_ROOT/$_pdir/node_modules/.bin/$(ci_tool_name "$_i")" ] || [ -x "$_CI_ROOT/$_pdir/.venv/bin/$(ci_tool_name "$_i")" ]; then
        return 0
    fi
    _vvar="CI_TOOL_${_i}_VERSION_CMD"
    IFS=$'\x1f' read -r -a _argv <<< "${!_vvar}"
    if ci_tool_version_ok "$_i" "${_argv[@]}"; then
        return 0
    fi
    return 1
}

# ci_tool_name INDEX — the tool row's name.
ci_tool_name() {
    local _nvar="CI_TOOL_$1_NAME"
    printf '%s' "${!_nvar}"
}

# ci_tool_version_ok INDEX ARGV... — run the version command with the
# project PATH candidates prepended; success means the tool resolves.
ci_tool_version_ok() {
    local _i=$1 _pdir _vpath=""
    shift
    local _rc=0
    _pdir=$(ci_tool_project_dir "$_i")
    if [ -d "$_CI_ROOT/$_pdir/node_modules/.bin" ]; then
        _vpath="$_CI_ROOT/$_pdir/node_modules/.bin"
    fi
    if [ -d "$_CI_ROOT/$_pdir/.venv/bin" ]; then
        if [ -n "$_vpath" ]; then
            _vpath="$_vpath:$_CI_ROOT/$_pdir/.venv/bin"
        else
            _vpath="$_CI_ROOT/$_pdir/.venv/bin"
        fi
    fi
    if [ -n "$_vpath" ]; then
        ( PATH="$_vpath:$PATH" "$@" >/dev/null 2>&1 ) || _rc=$?
    else
        ( "$@" >/dev/null 2>&1 ) || _rc=$?
    fi
    return "$_rc"
}

# ci_tool_project_dir INDEX — the tool row's project directory.
ci_tool_project_dir() {
    local _dvar="CI_TOOL_$1_DIR"
    printf '%s' "${!_dvar}"
}

# ci_bootstrap runs the frozen project-local installs before any tool
# invocation — the same commands CI runs. --skip-bootstrap opts out
# for jobs that provisioned the tools themselves.
ci_bootstrap() {
    [ "${CI_BOOTSTRAP_COUNT:-0}" -gt 0 ] || return 0
    local _i=1 _tvar _tname _cvar _rc
    local -a _argv
    if [ "$CI_SKIP_BOOTSTRAP" -eq 1 ]; then
        printf 'SKIP bootstrap (--skip-bootstrap)\n' >&2
        ci_json "$CI_SCRIPT" "bootstrap" "skip" 0 "skipped by flag"
        return 0
    fi
    while [ "$_i" -le "$CI_BOOTSTRAP_COUNT" ]; do
        _tvar="CI_BOOTSTRAP_${_i}_TOOL"; _tname=${!_tvar}
        _cvar="CI_BOOTSTRAP_${_i}_CMD"
        IFS=$'\x1f' read -r -a _argv <<< "${!_cvar}"
        printf 'BOOTSTRAP %s\n' "$_tname" >&2
        _rc=0
        ( "${_argv[@]}" ) 1>&2 || _rc=$?
        if [ "$_rc" -ne 0 ]; then
            ci_json "$CI_SCRIPT" "bootstrap" "fail" "$_rc" "bootstrap failed"
            ci_die "$CI_EX_ENV" "ERROR: bootstrap failed for $_tname (exit $_rc) — the environment is not prepared; fix the install or re-run with --skip-bootstrap"
        fi
        _i=$((_i + 1))
    done
}

# ci_betterleaks_config resolves the scan configuration at run time:
# the mirrored override wins over the generated tree copy, matching
# the sync precedence.
ci_betterleaks_config() {
    if [ -f "$_CI_CONF_DIR/security/betterleaks.toml" ]; then
        printf '%s' "$_CI_CONF_DIR/security/betterleaks.toml"
    else
        printf '%s' "$_CI_CONF_DIR/generated/tools/security/betterleaks.toml"
    fi
}

# ci_run_builtin KIND NAME — the built-in scan stages. A missing or
# unrunnable scan binary is the environment class (exit 3), never a
# finding.
ci_run_builtin() {
    local _kind=$1 _name=$2 _cfg _rc=0
    _cfg=$(ci_betterleaks_config)
    case $_kind in
        betterleaks-fs)
            ( cd "$_CI_ROOT" && betterleaks fs . --config "$_cfg" ) 1>&2 || _rc=$?
            ;;
        betterleaks-git)
            ( cd "$_CI_ROOT" && betterleaks git . --config "$_cfg" ) 1>&2 || _rc=$?
            ;;
    esac
    if [ "$_rc" -eq 127 ] || [ "$_rc" -eq 126 ] || [ "$_rc" -eq 130 ]; then
        ci_json "$CI_SCRIPT" "$_name" "env" "$_rc" "scan binary unrunnable"
        ci_die "$CI_EX_ENV" "ERROR: stage $_name needs betterleaks, which is not runnable (exit $_rc); run depcheck.sh for the install hint"
    fi
    if [ "$_rc" -eq 0 ]; then
        printf 'PASS %s\n' "$_name" >&2
        ci_json "$CI_SCRIPT" "$_name" "pass" 0 ""
    else
        printf 'FAIL %s (exit %s)\n' "$_name" "$_rc" >&2
        ci_json "$CI_SCRIPT" "$_name" "fail" "$_rc" "scan findings"
        CI_FAILED=$((CI_FAILED + 1))
    fi
    CI_RAN=$((CI_RAN + 1))
}

# ci_stage_exec DIR ARGV... — run one stage argv in its directory as a
# subshell; tool output streams to stderr untouched (findings are
# tool-native; stdout stays reserved for the script's own payloads)
# and only the exit code is captured. 127/126/130 (missing binary,
# exec failure, interrupt) re-class to the environment code — they are
# never findings.
ci_stage_exec() {
    local _dir=$1 _rc=0
    shift
    if [ -n "$_dir" ]; then
        ( cd "$_CI_ROOT/$_dir" && "$@" ) 1>&2 || _rc=$?
    else
        ( cd "$_CI_ROOT" && "$@" ) 1>&2 || _rc=$?
    fi
    if [ "$_rc" -eq 127 ] || [ "$_rc" -eq 126 ] || [ "$_rc" -eq 130 ]; then
        return "$CI_EX_ENV"
    fi
    return "$_rc"
}

# ci_run_stages MODE — dispatch the baked stage table. MODE "full"
# never appends files; MODE "edits" hands pass_files stages the
# CI_FILES list in CI_BATCH_SIZE chunks (no xargs: BSD and GNU differ
# on empty input). Files that vanish mid-run are loud per-file skips.
# Sets CI_RAN and CI_FAILED; a stage landing in the environment class
# exits 3 immediately with the depcheck pointer.
ci_run_stages() {
    local _mode=$1 _i=1 _name _dir _pf _builtin _cvar _rc _total _off _n _f _skip=""
    local -a _argv _batch
    CI_RAN=0
    CI_FAILED=0
    CI_BATCHES=0
    if [ -n "$CI_ONLY" ]; then
        _i=1
        while [ "$_i" -le "$CI_STAGE_COUNT" ]; do
            _nvar="CI_STAGE_${_i}_NAME"; _name=${!_nvar}
            case " $CI_ONLY " in
                *" $_name "*) ;;
                *) _skip=$_skip" $_name" ;;
            esac
            _i=$((_i + 1))
        done
        if [ -n "$_skip" ]; then
            ci_die "$CI_EX_USAGE" "ERROR: unknown stage(s):$_skip — known stages are in manifest.sh; --stages names must match"
        fi
    fi
    _i=1
    while [ "$_i" -le "$CI_STAGE_COUNT" ]; do
        _nvar="CI_STAGE_${_i}_NAME"; _name=${!_nvar}
        if [ -n "$CI_ONLY" ]; then
            case " $CI_ONLY " in
                *" $_name "*) ;;
                *) _i=$((_i + 1)); continue ;;
            esac
        fi
        _dvar="CI_STAGE_${_i}_DIR"; _dir=${!_dvar}
        _pvar="CI_STAGE_${_i}_PASSFILES"; _pf=${!_pvar}
        _bvar="CI_STAGE_${_i}_BUILTIN"; _builtin=${!_bvar}
        _cvar="CI_STAGE_${_i}_CMD"
        if [ -n "$_builtin" ]; then
            if [ "$_mode" = "edits" ] && [ "$_builtin" = "betterleaks-git" ]; then
                _i=$((_i + 1))
                continue
            fi
            ci_run_builtin "$_builtin" "$_name"
            _i=$((_i + 1))
            continue
        fi
        IFS=$'\x1f' read -r -a _argv <<< "${!_cvar}"
        _rc=0
        if [ "$_mode" = "edits" ] && [ "$_pf" -eq 1 ]; then
            # CI_FILES is assigned in edits.sh before the dispatch call;
            # a standalone analysis of this library cannot see that
            # assignment and misreads the array as a CI_FAILED misspelling.
            # shellcheck disable=SC2153
            _total=${#CI_FILES[@]}
            if [ "$_total" -eq 0 ]; then
                printf 'SKIP %s (no files)\n' "$_name" >&2
                ci_json "$CI_SCRIPT" "$_name" "skip" 0 "empty file set"
                _i=$((_i + 1))
                continue
            fi
            _off=0
            while [ "$_off" -lt "$_total" ]; do
                _batch=()
                _n=0
                while [ "$_n" -lt "$CI_BATCH_SIZE" ] && [ "$_off" -lt "$_total" ]; do
                    _f="${CI_FILES[$_off]}"
                    if [ -f "$_f" ]; then
                        _batch+=("$_f")
                        _n=$((_n + 1))
                    else
                        printf 'SKIP %s (file removed mid-run: %s)\n' "$_name" "$_f" >&2
                    fi
                    _off=$((_off + 1))
                done
                if [ "$_n" -eq 0 ]; then
                    _off=$_total
                    continue
                fi
                CI_BATCHES=$((CI_BATCHES + 1))
                ci_stage_exec "$_dir" "${_argv[@]}" "${_batch[@]}" || _rc=$?
                if [ "$_rc" -ne 0 ]; then
                    break
                fi
            done
        else
            ci_stage_exec "$_dir" "${_argv[@]}" || _rc=$?
        fi
        if [ "$_rc" -eq "$CI_EX_ENV" ]; then
            ci_json "$CI_SCRIPT" "$_name" "env" "$_rc" "environment class"
            ci_die "$CI_EX_ENV" "ERROR: stage $_name landed in the environment class (exit $_rc); the tool behind it looks missing or unrunnable — run depcheck.sh"
        fi
        if [ "$_rc" -eq 0 ]; then
            printf 'PASS %s\n' "$_name" >&2
            ci_json "$CI_SCRIPT" "$_name" "pass" 0 ""
        else
            printf 'FAIL %s (exit %s)\n' "$_name" "$_rc" >&2
            ci_json "$CI_SCRIPT" "$_name" "fail" "$_rc" "stage findings"
            CI_FAILED=$((CI_FAILED + 1))
        fi
        CI_RAN=$((CI_RAN + 1))
        _i=$((_i + 1))
    done
}

# ci_check_authors RANGE — every commit author in RANGE must carry an
# allowlisted identity ("Name <email>"); violations are findings.
# Adds to CI_FAILED and reports one record per violator.
ci_check_authors() {
    local _range=$1 _who _ok _i _avar _a
    local -a _allowed
    _allowed=()
    _i=1
    while [ "$_i" -le "$CI_AUTHOR_COUNT" ]; do
        _avar="CI_AUTHOR_${_i}"
        _allowed+=("${!_avar}")
        _i=$((_i + 1))
    done
    while IFS= read -r _who; do
        if [ -z "$_who" ]; then
            continue
        fi
        _ok=0
        for _a in "${_allowed[@]}"; do
            if [ "$_who" = "$_a" ]; then
                _ok=1
                break
            fi
        done
        if [ "$_ok" -ne 1 ]; then
            printf 'FAIL identity: %s is not an allowed commit author\n' "$_who" >&2
            ci_json "$CI_SCRIPT" "identity" "fail" 1 "unauthorized commit author"
            CI_FAILED=$((CI_FAILED + 1))
        fi
    done < <(git log --format='%an <%ae>' "$_range" 2>/dev/null | sort -u)
}

# ci_verdict SCOPE_NOTE — the end-of-run roll-up. Findings exit 1.
# Zero executed stages over a should-match surface is the vacuous
# class: exit 70, or 0 with a warning banner when --warn-empty was
# given (the empty scope itself is never vacuous — its script prints
# a distinct EMPTY verdict before reaching here).
ci_verdict() {
    local _scope_note=$1
    if [ "$CI_FAILED" -gt 0 ]; then
        printf 'SUMMARY %s: %s check(s) ran, %s failed\n' "$CI_SCRIPT" "$CI_RAN" "$CI_FAILED" >&2
        ci_json "$CI_SCRIPT" "__summary__" "findings" 1 "ran=$CI_RAN failed=$CI_FAILED"
        exit "$CI_EX_FINDINGS"
    fi
    if [ "$CI_RAN" -eq 0 ]; then
        if [ "$CI_WARN_EMPTY" -eq 1 ]; then
            printf 'WARN: nothing to check (%s) — vacuous run downgraded by --warn-empty\n' "$_scope_note" >&2
            ci_json "$CI_SCRIPT" "__summary__" "empty" 0 "warn-empty downgrade"
            exit "$CI_EX_OK"
        fi
        printf 'ERROR: nothing to check (%s) — a run that checks nothing is vacuous; declare stages in the configuration or re-run with --warn-empty\n' "$_scope_note" >&2
        ci_json "$CI_SCRIPT" "__summary__" "vacuous" 70 "no stages matched"
        exit "$CI_EX_VACUOUS"
    fi
    printf 'SUMMARY %s: %s check(s) ran, 0 failed\n' "$CI_SCRIPT" "$CI_RAN" >&2
    ci_json "$CI_SCRIPT" "__summary__" "ok" 0 ""
    exit "$CI_EX_OK"
}

# ci_usage_exit prints the usage block and exits 2.
ci_usage_exit() {
    printf '%s\n' "$1" >&2
    exit "$CI_EX_USAGE"
}

# ci_version_ge A B — pure-shell field comparator (no sort -V: GNU
# only). Numeric fields compare left to right; equal numerics order a
# pre-release suffix BELOW the plain release, and two suffixes compare
# lexically (documented approximation).
ci_version_ge() {
    local a=${1#v} b=${2#v} ra="" rb="" oldIFS
    local a1=0 a2=0 a3=0 b1=0 b2=0 b3=0
    case $a in *-*) ra=${a#*-}; a=${a%%-*} ;; esac
    case $b in *-*) rb=${b#*-}; b=${b%%-*} ;; esac
    oldIFS=$IFS
    IFS=.
    # Version fields split on the dot IFS — the unquoted expansions are
    # the intended word split; quoting would compare the whole string.
    # shellcheck disable=SC2086
    set -- $a
    a1=${1:-0}; a2=${2:-0}; a3=${3:-0}
    # shellcheck disable=SC2086
    set -- $b
    b1=${1:-0}; b2=${2:-0}; b3=${3:-0}
    IFS=$oldIFS
    ci_num a1; ci_num a2; ci_num a3
    ci_num b1; ci_num b2; ci_num b3
    if [ "$a1" -gt "$b1" ]; then return 0; fi
    if [ "$a1" -lt "$b1" ]; then return 1; fi
    if [ "$a2" -gt "$b2" ]; then return 0; fi
    if [ "$a2" -lt "$b2" ]; then return 1; fi
    if [ "$a3" -gt "$b3" ]; then return 0; fi
    if [ "$a3" -lt "$b3" ]; then return 1; fi
    if [ -z "$ra" ] && [ -n "$rb" ]; then return 0; fi
    if [ -n "$ra" ] && [ -z "$rb" ]; then return 1; fi
    if [ "$ra" = "$rb" ]; then return 0; fi
    if [[ "$ra" > "$rb" ]]; then return 0; fi
    return 1
}

# ci_num VAR — force base-10 numeric; non-numeric or empty becomes 0.
ci_num() {
    local _v=${!1}
    case $_v in
        '' | *[!0-9]*) _v=0 ;;
        *) _v=$((10#$_v)) ;;
    esac
    printf -v "$1" '%s' "$_v"
}
