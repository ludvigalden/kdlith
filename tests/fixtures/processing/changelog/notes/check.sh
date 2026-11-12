#!/bin/sh
# Judge `kdlith changelog` on the build path the run declares:
# KDLITH_FX_CHANGELOG=absent for a binary built without relmap,
# anything else for one built with it. Every target is judged.
K="$KDLITH_FX_KDLITH"

# An unknown target is an argument fault on either build path.
"$K" changelog --target csv > out 2> err
status=$?
if [ "$status" -ne 2 ] || [ -s out ]; then
    echo "unknown target: exit $status, stdout not empty"
    exit 1
fi
grep -q 'invalid value' err || { echo "unknown target: no invalid-value error"; exit 1; }

if [ "${KDLITH_FX_CHANGELOG:-embedded}" = absent ]; then
    for target in markdown ndjson index metadata; do
        "$K" changelog --target "$target" > out 2> err
        status=$?
        test "$status" -eq 1 || { echo "target $target: exit $status, expected 1"; exit 1; }
        test ! -s out || { echo "target $target: stdout is not empty"; exit 1; }
        printf 'error[KDLITH-CHG001]: changelog data is unavailable: this kdlith was built without relmap\n  help: Rebuild kdlith with relmap on PATH, or with RELMAP_BIN naming it\n' > want
        cmp -s want err || { echo "target $target: stderr differs:"; cat err; exit 1; }
    done
    echo "changelog unavailable as declared"
    exit 0
fi

"$K" changelog > out 2> err
status=$?
test "$status" -eq 0 || { echo "exit $status, expected 0:"; cat err; exit 1; }
test ! -s err || { echo "stderr is not empty:"; cat err; exit 1; }
printf '# Changelog\n\n## ' > want
head -c 16 out | cmp -s want - || { echo "notes do not open with the title and a release heading"; exit 1; }
printf '## 0.1.0 (2026-11-07)\n\nBaseline of the first kdlith development line\n\nkdlith provides typed, streaming KDL processing in Rust: dialect and schema\ndefinitions describe families of KDL documents, which kdlith processes directly\nor turns into generated Rust processors. This pre-1.0 baseline is documented as\nunstable.\n\n### Added\n' > want
sed -n '/^## 0\.1\.0 (2026-11-07)$/,/^### Added$/p' out | cmp -s want - || { echo "the 0.1.0 notes do not open with its summary and paragraph"; exit 1; }

# --target markdown names the same page as the default.
"$K" changelog --target markdown > out2 2> err
status=$?
test "$status" -eq 0 || { echo "--target markdown: exit $status:"; cat err; exit 1; }
test ! -s err || { echo "--target markdown: stderr is not empty"; exit 1; }
cmp -s out out2 || { echo "--target markdown differs from the default page"; exit 1; }

# The NDJSON target is one compact JSON record per line, header first.
"$K" changelog --target ndjson > out 2> err
status=$?
test "$status" -eq 0 || { echo "--target ndjson: exit $status:"; cat err; exit 1; }
test ! -s err || { echo "--target ndjson: stderr is not empty"; exit 1; }
test -s out || { echo "--target ndjson: stdout is empty"; exit 1; }
grep -q '"kind":"header"' out || { echo "--target ndjson: no header record"; exit 1; }
test -z "$(tail -c 1 out)" || { echo "--target ndjson: no trailing newline"; exit 1; }

# The JSON targets are pretty-printed: they open with a brace and span
# one value per line. Their content is relmap's rendering and is not
# pinned here.
for target in index metadata; do
    "$K" changelog --target "$target" > out 2> err
    status=$?
    test "$status" -eq 0 || { echo "--target $target: exit $status:"; cat err; exit 1; }
    test ! -s err || { echo "--target $target: stderr is not empty"; exit 1; }
    test -s out || { echo "--target $target: stdout is empty"; exit 1; }
    head -c 1 out | grep -q '{' || { echo "--target $target: not JSON"; exit 1; }
    test "$(wc -l < out)" -gt 1 || { echo "--target $target: not one value per line"; exit 1; }
    test -z "$(tail -c 1 out)" || { echo "--target $target: no trailing newline"; exit 1; }
done
echo "changelog embedded as declared"
