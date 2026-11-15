#!/usr/bin/env bash
# Runs the line-coverage gate for one crate and reports the number as an annotation, so it is
# visible on the run page and through the API and not only deep in the log. cargo-llvm-cov
# fails a run that is under the threshold with exit code 1 and no message of its own.
#
# usage: bash .github/scripts/coverage-gate.sh <crate> <minimum line coverage in percent>
set -u

crate="${1:?usage: coverage-gate.sh <crate> <minimum percent>}"
minimum="${2:?usage: coverage-gate.sh <crate> <minimum percent>}"

status=0
cargo llvm-cov -p "$crate" --fail-under-lines "$minimum" > coverage.log 2>&1 || status=$?
cat coverage.log

# The TOTAL row: regions, missed, cover, functions, missed, cover, lines, missed, cover, ...
total="$(grep -E '^TOTAL' coverage.log | awk '{print $10}')"

echo "::notice title=Coverage::${crate} line coverage is ${total:-unknown} (gate ${minimum}%)"
if [ "$status" -ne 0 ]; then
  echo "::error title=Coverage gate::${crate}: line coverage ${total:-unknown} against a gate of ${minimum}%, or a test failed. The table is in this step's log."
fi
exit "$status"
