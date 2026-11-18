#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-live-bytes"
fixtures="$root/tools/tests/live-bytes"
bin=${FOCUS_BIN:-$root/target/debug/focus}
[ -x "$bin" ] || { echo "FAIL live-bytes: $bin is not built (cargo build -p focus -j 3)"; exit 1; }

prepare() {
    rm -rf "$work/base" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/base"
    mkdir -p "$work/base/Live" "$work/base/img"
    cp "$root/fixtures/vault/Red Team/assets/capture-exemple.png" "$work/base/img/pic.png"
    cp "$fixtures/Blocs.md" "$fixtures/Cible.md" "$fixtures/Onglets.md" "$work/base/Live/"
    python3 - "$work/base/Live" <<'PY'
import sys
d = sys.argv[1]
blocs = open(f'{d}/Blocs.md', 'rb').read()
open(f'{d}/CRLF.md', 'wb').write(blocs.replace(b'\n', b'\r\n'))
open(f'{d}/SansFin.md', 'wb').write(b'# Sans fin\n\nDernier paragraphe sans retour')
open(f'{d}/DoubleFin.md', 'wb').write(b'# Double fin\n\nParagraphe.\n\n\n')
with open(f'{d}/Long.md', 'w') as f:
    f.write('---\ntitle: Longue\n---\n\n')
    for i in range(1, 3001):
        if i % 50 == 1:
            f.write(f'## Section {i // 50 + 1}\n\n')
        f.write(f'Paragraphe numéro {i} : texte de remplissage avec un [[Cible]] et du **gras**.\n\n')
PY
}

OPEN="open Live/Blocs.md;view live;wait 400"
CLOSE="key escape;wait 1500"
cases=()
block() {
    cases+=("blk-$1|guard|Live/Blocs.md|append:$2:ZZ|$OPEN;block $2;wait 300;key end;type ZZ;$CLOSE|")
}
block paragraph 9;  block heading 12; block list 14;     block nested 17;   block tasks 21;    block table 25
block code 30;      block callout 35; block quote 39;    block mermaid 42;  block math 47;     block image 51
block include 53;   block footnote-reference 55;         block details-paragraph 62;           block details-list 64
block tab-paragraph 72; block tab-code 74; block mdbook-tab 88; block last-paragraph 95
cases+=("noop|guard|Live/Blocs.md|unchanged|$OPEN;block 9;wait 300;$CLOSE|")
cases+=("undo-after-close|guard|Live/Blocs.md|unchanged|$OPEN;block 9;wait 300;key end;type ZZ;$CLOSE;key ctrl-z;wait 1500|")
cases+=("switch-note|guard|Live/Blocs.md|append:9:ZZ|$OPEN;block 9;wait 300;key end;type ZZ;wait 200;open 00 Accueil.md;wait 1500|")
cases+=("switch-to-reading|guard|Live/Blocs.md|append:9:ZZ|$OPEN;block 9;wait 300;key end;type ZZ;wait 200;view reading;wait 1500|")
cases+=("switch-to-source|guard|Live/Blocs.md|append:9:ZZ|$OPEN;block 9;wait 300;key end;type ZZ;wait 200;view source;wait 1500|")
cases+=("crlf-paragraph|guard|Live/CRLF.md|append:9:ZZ|open Live/CRLF.md;view live;wait 400;block 9;wait 300;key end;type ZZ;$CLOSE|")
cases+=("crlf-table|guard|Live/CRLF.md|append:25:ZZ|open Live/CRLF.md;view live;wait 400;block 25;wait 300;key end;type ZZ;$CLOSE|")
cases+=("no-final-newline|guard|Live/SansFin.md|eof:ZZ|open Live/SansFin.md;view live;wait 400;block 3;wait 300;key end;type ZZ;$CLOSE|")
cases+=("two-final-newlines|guard|Live/DoubleFin.md|append:3:ZZ|open Live/DoubleFin.md;view live;wait 400;block 3;wait 300;key end;type ZZ;$CLOSE|")
cases+=("long-note|guard|Live/Long.md|append:6001:ZZ|open Live/Long.md;view live;wait 1500;scroll 0.99;wait 800;block 6001;wait 500;key end;type ZZ;$CLOSE|")
cases+=("tab-fenced|guard|Live/Onglets.md|append:7:ZZ|open Live/Onglets.md;view live;wait 500;block 7;wait 300;key end;type ZZ;$CLOSE|")
cases+=("tab-mdbook|guard|Live/Onglets.md|append:33:ZZ|open Live/Onglets.md;view live;wait 500;block 33;wait 300;key end;type ZZ;$CLOSE|")
cases+=("properties-block|guard|Live/Blocs.md|append:2:ZZ|$OPEN;block 2;wait 300;key end;type ZZ;$CLOSE|")
cases+=("title-block|guard|Live/Blocs.md|append:7:ZZ|$OPEN;block 7;wait 300;key end;type ZZ;$CLOSE|")
cases+=("up-into-block-above|guard|Live/Blocs.md|inside:12:U12|$OPEN;block 14;wait 300;key up;wait 300;type U12;$CLOSE|")
cases+=("external-edit-other-block|guard|Live/Blocs.md|conflict:AABB|$OPEN;block 9;wait 300;key end;type AA;wait 7000;key end;type BB;wait 500;$CLOSE|Dernier paragraphe.=>Dernier paragraphe MODIFIE EXTERNE.")
cases+=("external-edit-same-block|guard|Live/Blocs.md|conflict:AABB|$OPEN;block 9;wait 300;key end;type AA;wait 7000;key end;type BB;wait 500;$CLOSE|Deuxième ligne du même paragraphe.=>Deuxième ligne MODIFIEE EXTERNE.")
cases+=("crlf-new-line|guard|Live/CRLF.md|insert:9:\\r\\nNouvelle|open Live/CRLF.md;view live;wait 400;block 9;wait 300;key end;key enter;type Nouvelle;$CLOSE|")
cases+=("crlf-new-paragraph|guard|Live/CRLF.md|eof:\\r\\nFin.\\r\\n|open Live/CRLF.md;view live;wait 400;block end;wait 300;type Fin.;$CLOSE|")
cases+=("link-completion|guard|Live/Blocs.md|append:55:[[Cible]]|set editor.link_style 1;$OPEN;block 55;wait 300;key end;type  [[Ci;wait 600;key enter;$CLOSE|")
cases+=("footnote-definition|guard|Live/Blocs.md|append:57:ZZ|$OPEN;block 57;wait 300;key end;type ZZ;$CLOSE|")

want=("$@")
selected() { [ ${#want[@]} -eq 0 ] && return 0; local w; for w in "${want[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

prepare
failed=0
for entry in "${cases[@]}"; do
    IFS='|' read -r name status file spec steps edit <<<"$entry"
    selected "$name" || continue
    dir="$work/$name"
    rm -rf "$dir" && mkdir -p "$dir" && cp -r "$work/base" "$dir/vault"
    printf 'set editor.edit_in_place off\n%s\n' "${steps//;/$'\n'}" >"$dir/script.focus"
    echo quit >>"$dir/script.focus"
    if [ -n "$edit" ]; then
        ( for _ in $(seq 1 600); do grep -q "AA" "$dir/vault/Live/Blocs.md" && break; sleep 0.1; done
          python3 - "$dir/vault/Live/Blocs.md" "${edit%%=>*}" "${edit##*=>}" <<'PY'
import sys
p, a, b = sys.argv[1:4]
t = open(p, encoding='utf-8').read().replace(a, b)
open(p, 'w', encoding='utf-8').write(t)
PY
        ) &
    fi
    FOCUS_BIN="$bin" FOCUS_SHOT_DIR="$dir/shots" "$root/tools/shot.sh" "$dir/script.focus" "$dir/vault" >"$dir/log.txt" 2>&1
    code=$?
    wait
    if [ $code -ne 0 ]; then
        result="the application exited with status $code: $(grep -m1 panicked "$dir/log.txt" || echo see $dir/log.txt)"
    elif [ "${spec%%:*}" = conflict ]; then
        copy="$dir/vault/Live/Blocs (unsaved copy).md"
        result=""
        grep -q "AA" "$dir/vault/$file" || result="AA is missing from the note"
        grep -q -- "${edit##*=>}" "$dir/vault/$file" || result="the change on disk is gone from the note"
        grep -q "BB" "$dir/vault/$file" && result="BB reached the note over the change on disk"
        [ -f "$copy" ] && grep -q -- "${spec#conflict:}" "$copy" || result="${result:+$result; }no unsaved copy holding ${spec#conflict:}"
    else
        result=$(python3 "$root/tools/tests/live-bytes.py" "$work/base" "$dir/vault" "$file" "$spec" 2>&1) && result=""
    fi
    case "$status" in
        guard)
            if [ -z "$result" ]; then echo "ok   $name"; else echo "FAIL $name: ${result%%$'\n'*}"; [ "$result" != "${result%%$'\n'*}" ] && echo "${result#*$'\n'}" | sed 's/^/       /'; failed=1; fi ;;
        xfail:*)
            tag=${status#xfail:}
            if [ -n "$result" ]; then echo "xfail $name ($tag): ${result%%$'\n'*}"; else echo "XPASS $name ($tag): it passes now, move it to the guarded cases"; failed=1; fi ;;
    esac
done
exit $failed
