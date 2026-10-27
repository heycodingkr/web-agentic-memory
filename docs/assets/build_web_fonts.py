#!/usr/bin/env python3
"""Subset the site's three open fonts to Latin and write them as woff2 into web/static/fonts.

app.css names Manrope (headings), DM Sans (text) and DM Mono (labels) first in every font stack. This script
produces the files that make those names resolve. All three are SIL Open Font License 1.1 and none declares a
Reserved Font Name, so subsetting is allowed; the licence texts are written next to the fonts as LICENSE.txt.

Hangul and other scripts are not included: the stacks fall back to Apple SD Gothic Neo / Malgun Gothic.

Needs: fonttools, brotli (pip install fonttools brotli). Sources come from github.com/google/fonts, ofl/:
    Manrope[wght].ttf  DMSans[opsz,wght].ttf  DMMono-Regular.ttf  DMMono-Medium.ttf
    OFL-Manrope.txt    OFL-DMSans.txt         OFL-DMMono.txt        (the OFL.txt of each family)

    python3 docs/assets/build_web_fonts.py --fonts-dir /path/to/fonts
"""
import argparse
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "web" / "static" / "fonts"

# Latin, Latin-1, Latin Extended-A, general punctuation, currency, arrows used in the UI (→ ↗), minus, ≤ ≥.
UNICODES = (
    list(range(0x20, 0x7F)) + list(range(0xA0, 0x180)) + [0x2BB, 0x2BC, 0x2C6, 0x2DA, 0x2DC]
    + list(range(0x2000, 0x2070)) + [0x20AC, 0x2122] + list(range(0x2190, 0x219A)) + [0x2212, 0x2215, 0x2264, 0x2265, 0xFEFF, 0xFFFD]
)


def write(font, name):
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.notdef_outline = True
    options.hinting = False
    options.drop_tables += ["DSIG"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=UNICODES)
    subsetter.subset(font)
    font.flavor = "woff2"
    target = OUT / name
    font.save(target)
    print(f"{name}: {target.stat().st_size / 1024:.1f} KB")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fonts-dir", required=True)
    args = parser.parse_args()
    src = Path(args.fonts_dir)
    needed = ["Manrope[wght].ttf", "DMSans[opsz,wght].ttf", "DMMono-Regular.ttf", "DMMono-Medium.ttf", "OFL-Manrope.txt", "OFL-DMSans.txt", "OFL-DMMono.txt"]
    missing = [n for n in needed if not (src / n).is_file()]
    if missing:
        sys.exit(f"missing in {src}: {', '.join(missing)}")
    OUT.mkdir(parents=True, exist_ok=True)

    write(TTFont(src / "Manrope[wght].ttf"), "manrope-latin.woff2")
    # DM Sans is used at 10-17px: pin the optical size axis to 14 and keep the weight axis.
    write(instantiateVariableFont(TTFont(src / "DMSans[opsz,wght].ttf"), {"opsz": 14}), "dm-sans-latin.woff2")
    write(TTFont(src / "DMMono-Regular.ttf"), "dm-mono-latin-400.woff2")
    write(TTFont(src / "DMMono-Medium.ttf"), "dm-mono-latin-500.woff2")

    notices = []
    for family, name in (("Manrope", "OFL-Manrope.txt"), ("DM Sans", "OFL-DMSans.txt"), ("DM Mono", "OFL-DMMono.txt")):
        notices.append(f"=== {family} ===\n\n{(src / name).read_text().strip()}\n")
    (OUT / "LICENSE.txt").write_text("\n".join(notices))
    print("wrote", OUT)


if __name__ == "__main__":
    main()
