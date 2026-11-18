#!/usr/bin/env bash
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
exclude=()
for c in ${FOCUS_CHECK_EXCLUDE:-}; do exclude+=(--exclude "$c"); done
jobs=${FOCUS_CHECK_JOBS:-4}
[ "$jobs" -gt 4 ] && jobs=4
log=${FOCUS_CHECK_LOG:-$root/target/check}
mkdir -p "$log"
failed=0
wanted=("$@")

want() {
    [ ${#wanted[@]} -eq 0 ] && return 0
    local w
    for w in "${wanted[@]}"; do [ "$w" = "$1" ] && return 0; done
    return 1
}

step() {
    want "$1" || return 0
    if "$2" > "$log/$1.txt" 2>&1; then
        echo "ok    $1"
    else
        echo "FAIL  $1 (full output: target/check/$1.txt)"
        head -n 12 "$log/$1.txt" | cut -c1-160
        failed=1
        [ -z "${FOCUS_CHECK_ALL:-}" ] && exit 1
    fi
    return 0
}

per_crate() {
    grep -oE '(crates/[a-z0-9-]+|app)/' | sed 's#/$##; s#^crates/##' | sort | uniq -c | sort -rn | awk '{printf "  %s: %s\n", $2, $1}'
}

check_fmt() {
    local out
    out=$(cargo fmt --check 2>&1)
    [ -z "$out" ] && return 0
    echo "files that rustfmt would change, by crate (diff hunks):"
    echo "$out" | grep '^Diff in' | sed -E 's#Diff in .*/(crates/[a-z0-9-]+|app)/.*#\1/#' | per_crate
    return 1
}

check_clippy() {
    local out
    out=$(cargo clippy --workspace "${exclude[@]}" --all-targets -j "$jobs" -- -D warnings 2>&1)
    local status=$?
    [ $status -eq 0 ] && return 0
    echo "warnings and errors by crate:"
    echo "$out" | grep -E '^ *--> (crates/[a-z0-9-]+|app)/' | per_crate
    echo "$out" | grep -E '^(error|warning)' | grep -v 'generated' | sort | uniq -c | sort -rn | head -8
    return 1
}

check_test() {
    local out
    out=$(cargo test --workspace --no-fail-fast "${exclude[@]}" -j "$jobs" 2>&1)
    local status=$?
    [ $status -eq 0 ] && return 0
    echo "failed tests:"
    echo "$out" | grep -E '^test .* FAILED$' | sed 's/^test //; s/ \.\.\. FAILED//' | head -30
    echo "$out" | grep -E '^error(\[|:)' | head -5
    return 1
}

check_i18n() {
    local out
    out=$(cargo test -p focus-i18n -j "$jobs" 2>&1)
    local status=$?
    [ $status -eq 0 ] && return 0
    echo "$out" | grep -E 'FAILED|panicked|assertion|lacks|missing from|has extra' | head -15
    return 1
}

check_chars() {
    local hits
    hits=$(grep -rnP '[\x{2014}\x{2013}\x{2190}-\x{21FF}\x{27F5}-\x{27FF}\x{2900}-\x{297F}\x{1F300}-\x{1FAFF}\x{2600}-\x{26FF}\x{2700}-\x{27BF}]' \
        --include='*.rs' --include='*.ftl' --include='*.md' --include='*.sh' --include='*.focus' --include='*.toml' \
        app crates locales docs tools README.md \
        --exclude-dir=target --exclude-dir=tests --exclude-dir=reference --exclude-dir=data --exclude=tests.rs --exclude='tests_*.rs' 2>/dev/null || true)
    hits=$(echo "$hits" | grep -vEf <(grep -v '^#' "$root/tools/check-chars-allowed.txt" | grep -v '^$') || true)
    hits=$(echo "$hits" | grep -v '^$' || true)
    [ -z "$hits" ] && return 0
    echo "$hits" | cut -c1-150 | head -20
    echo "total: $(echo "$hits" | wc -l) lines"
    return 1
}

check_french() {
    local allow="$root/tools/check-french-allowed.txt"
    local hits="" file
    while IFS= read -r file; do
        hits+=$(sed '/^#\[cfg(test)\]/,$d' "$file" | grep -nP '[àâçéèêëîïôùûüÿœÀÂÇÉÈÊÎÔÙÛ]' | sed "s#^#$file:#")$'\n'
    done < <(find app crates -name '*.rs' -not -path '*/target/*' -not -path '*/tests/*' -not -path '*/examples/*' -not -path '*/benches/*' -not -name 'tests.rs' -not -name 'tests_*.rs' -not -name 'testvault.rs')
    if [ -f "$allow" ]; then
        hits=$(echo "$hits" | grep -vEf <(grep -v '^#' "$allow" | grep -v '^$') || true)
    fi
    hits=$(echo "$hits" | grep -v '^$' || true)
    [ -z "$hits" ] && return 0
    echo "$hits" | cut -c1-140 | head -25
    echo "total: $(echo "$hits" | wc -l) lines; by crate:"
    echo "$hits" | per_crate
    return 1
}

check_comments() {
    local dir name count
    for dir in app crates/*; do
        [ -d "$dir/src" ] || continue
        name=${dir#crates/}
        count=$(grep -rE '^\s*//' "$dir/src" --include='*.rs' | wc -l)
        printf '  %-18s %5d\n' "$name" "$count"
    done
    return 0
}

step fmt check_fmt
step clippy check_clippy
step test check_test
step i18n check_i18n
step chars check_chars
step french check_french
if want comments; then
    echo "comment lines per crate (src only):"
    check_comments
fi
exit $failed
