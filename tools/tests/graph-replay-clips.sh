#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
vault=${1:?vault}
speed=${2:?speed}
seconds=${3:?seconds}
out=${4:?out dir}
bin=${FOCUS_BIN:-$root/target/debug/examples/demo}
interval=${FOCUS_CLIP_MS:-100}
cache=$root/target/demo-cache/$(basename "$vault")
out=$(realpath -m "$out")
case $out in
    "$root"/target/?*) ;;
    *) echo "the out dir must be under $root/target/" >&2; exit 2 ;;
esac
restore() {
    if [ -n "${FOCUS_REPLAY_CACHE:-}" ]; then
        rm -rf "${cache:?}"
        cp -r "$FOCUS_REPLAY_CACHE" "$cache"
    fi
}
rm -rf "${out:?}"
mkdir -p "$out/frames"
work=$out/scripts
mkdir -p "$work"

printf 'measure rest\nmeasure-spread rest\nspeed %s\nanimate\nmeasure-motion replay %s\nmeasure-spread end\nquit\n' \
    "$speed" "$((seconds * 3))" >"$work/measure.focus"
restore
FOCUS_BIN=$bin FOCUS_SHOT_TIMEOUT=$((seconds * 3 + 120)) FOCUS_SHOT_DIR=$out/frames \
    "$root/tools/shot.sh" "$work/measure.focus" "$vault" >"$out/measures.log" 2>&1

{
    printf 'measure rest\nspeed %s\nanimate\n' "$speed"
    count=$((seconds * 1000 / (interval + 60)))
    for i in $(seq -f '%05g' 1 "$count"); do
        printf 'wait %s\nsnap f%s\n' "$interval" "$i"
    done
    printf 'quit\n'
} >"$work/clip.focus"
restore
FOCUS_BIN=$bin FOCUS_SHOT_TIMEOUT=$((seconds * 3 + 120)) FOCUS_SHOT_DIR=$out/frames \
    "$root/tools/shot.sh" "$work/clip.focus" "$vault" >"$out/clip.log" 2>&1
frames=$(find "$out/frames" -name 'f*.png' | wc -l)
first=$(stat -c %.3Y "$out/frames/f00001.png" 2>/dev/null || echo 0)
last=$(find "$out/frames" -name 'f*.png' -printf '%T@\n' | sort -n | tail -1)
rate=$(python3 -c "import sys; f,a,b=int(sys.argv[1]),float(sys.argv[2]),float(sys.argv[3]); print(f'{(f-1)/max(b-a,0.001):.3f}')" "$frames" "$first" "${last:-0}")
ffmpeg -loglevel error -y -framerate "$rate" -i "$out/frames/f%05d.png" -c:v libx264 -pix_fmt yuv420p \
    -vf 'scale=trunc(iw/2)*2:trunc(ih/2)*2' "$out/replay.mp4"
echo "clip $out/replay.mp4: $frames frames at $rate frames/s"
grep -E '^measure' "$out/measures.log"
