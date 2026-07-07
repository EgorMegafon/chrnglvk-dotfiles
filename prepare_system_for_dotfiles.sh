#!/usr/bin/env bash

FONT_DIR="$HOME/.local/share/fonts"

# === GENERATE SOLID FONT VARIANT ===
mkdir -p "$FONT_DIR"
IN="/usr/share/fonts/ttf-material-symbols-variable/MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf"
OUT="$FONT_DIR/MaterialSymbolsRoundedFilled.ttf"
if [ ! -f "$OUT" ]; then
    fonttools varLib.instancer "$IN" FILL=1 -o "$OUT"
    python3 - "$OUT" <<'PY'
import sys
from fontTools.ttLib import TTFont
p = sys.argv[1]; f = TTFont(p); fam = "Material Symbols Rounded Filled"; n = f["name"]
n.setName(fam, 1, 3, 1, 0x409); n.setName(fam, 4, 3, 1, 0x409)
n.setName("MaterialSymbolsRoundedFilled", 6, 3, 1, 0x409); n.setName(fam, 16, 3, 1, 0x409)
f.save(p)
PY
fi
fc-cache -f "$FONT_DIR"

echo


# === BUILD RUST BINARIES ===
for c in "$EWW_DIR"/scripts/*/Cargo.toml; do
    ( cd "$(dirname "$c")" && cargo build --release )
done
