#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
mode=full
allow_load=0
only=
out=
while [ $# -gt 0 ]; do
    case $1 in
        --quick) mode=quick ;;
        --full) mode=full ;;
        --allow-load) allow_load=1 ;;
        --only) only=${2:?--only needs a domain}; shift ;;
        --out) out=${2:?--out needs a path}; shift ;;
        -h|--help) sed -n 2,26p "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done
load=$(cut -d' ' -f1 /proc/loadavg)
cores=$(getconf _NPROCESSORS_ONLN)
if awk -v l="$load" -v c="$cores" 'BEGIN { exit !(l > 0.25 * c) }' && [ "$allow_load" = 0 ]; then
    echo "load average $load is above 0.25 per logical core ($cores cores): other work would show in the numbers. Wait, or pass --allow-load (the results are then marked loaded)." >&2
    exit 3
fi
cd "$root"
mkdir -p target/bench
cargo build --release -p focus-search --example bench_suite -p focus-graph --example graph_bench -j 6
if [ -z "$out" ]; then
    if [ "$mode" = quick ]; then out=target/bench/results-quick.json; else out=docs/benchmarks/results.json; fi
fi
mkdir -p "$(dirname "$out")"
python3 -I tools/bench/collect.py --mode "$mode" --allow-load "$allow_load" --out "$out" ${only:+--only "$only"}
if [ "$mode" = quick ]; then report=target/bench/BENCHMARKS-quick.md; else report=docs/BENCHMARKS.md; fi
python3 -I tools/bench/report.py "$out" "$report"
echo "results: $out"
echo "report:  $report"
