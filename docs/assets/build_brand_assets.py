#!/usr/bin/env python3
"""Generate the Web Agentic Memory brand assets (SVG first) into web/static.

The mark is a "W" drawn as one route: a small start dot, a smooth trail, and a larger lime destination
node. It echoes the route lines used across the site. Everything is vector; the wordmark is converted to
outlines so the files render the same everywhere without any font installed.

Needs: fonttools, uharfbuzz, pillow (pip install fonttools brotli uharfbuzz pillow), the `rsvg-convert`
binary (librsvg), and Google Chrome for the social card. Fonts are SIL OFL; pass --fonts-dir with
Manrope[wght].ttf, DMSans[opsz,wght].ttf and DMMono-Medium.ttf (github.com/google/fonts, ofl/).

    python3 docs/assets/build_brand_assets.py --fonts-dir /path/to/fonts
"""
import argparse
import io
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
STATIC = ROOT / "web" / "static"
BRAND = STATIC / "brand"

GREEN = "#1b4338"
GREEN_DARK = "#132f28"
CREAM = "#f5f3ec"
LIME = "#d8ef78"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

TAGLINE = "Leave a path. Move together."
NAME = "web agentic memory"


def num(value):
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return text or "0"


def glyph(trail, ring, stroke=5.0, start_r=3.4, end_r=5.8, ring_w=2.6, start="#d8ef78", end="#d8ef78"):
    """The W route on a 64x64 grid. `ring` separates the destination node from the trail."""
    return (
        f'<path d="M12 21C16.5 21 17 45 22.5 45S27.5 27 32 27 37 45 41.5 45 47.5 21 52 21" fill="none" '
        f'stroke="{trail}" stroke-width="{num(stroke)}" stroke-linecap="round" stroke-linejoin="round"/>'
        f'<circle cx="12" cy="21" r="{num(start_r)}" fill="{start}"/>'
        f'<circle cx="52" cy="21" r="{num(end_r)}" fill="{end}" stroke="{ring}" stroke-width="{num(ring_w)}"/>'
    )


def centered(inner):
    # The glyph's optical centre sits ~1.2 left / 0.7 above the tile centre; nudge it back.
    return f'<g transform="translate(-1.2 0.7)">{inner}</g>'


def svg(view, body, title=None, extra=""):
    t = f'<title id="t">{title}</title>' if title else ""
    label = ' role="img" aria-labelledby="t"' if title else ' aria-hidden="true"'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}"{label}{extra}>{t}{body}</svg>\n'
    )


def tile(inner, fill=GREEN, rx=20, size=64):
    return f'<rect width="{size}" height="{size}" rx="{rx}" fill="{fill}"/>{inner}'


def outline_text(font_path, text, size, weight, tracking_em):
    font = TTFont(font_path)
    font = instantiateVariableFont(font, {"wght": weight})
    buf = io.BytesIO()
    font.save(buf)
    face = hb.Face(buf.getvalue())
    hb_font = hb.Font(face)
    hb_buf = hb.Buffer()
    hb_buf.add_str(text)
    hb_buf.guess_segment_properties()
    hb.shape(hb_font, hb_buf, {"kern": True, "liga": False})
    order = font.getGlyphOrder()
    glyphs = font.getGlyphSet()
    scale = size / font["head"].unitsPerEm
    x_height = (font["OS/2"].sxHeight or font["head"].unitsPerEm * 0.5) * scale
    pen = SVGPathPen(glyphs, ntos=num)
    x = 0.0
    for info, pos in zip(hb_buf.glyph_infos, hb_buf.glyph_positions):
        draw = TransformPen(pen, (scale, 0, 0, -scale, x + pos.x_offset * scale, 0))
        glyphs[order[info.codepoint]].draw(draw)
        x += pos.x_advance * scale + tracking_em * size
    return pen.getCommands(), x - tracking_em * size, x_height


def lockup(font_path, reversed_colors):
    word = GREEN_DARK if not reversed_colors else CREAM
    path, width, x_height = outline_text(font_path, NAME, size=33, weight=800, tracking_em=-0.035)
    gap = 16
    baseline = 32 + x_height / 2
    total = 64 + gap + width
    body = (
        tile(centered(glyph(CREAM, GREEN)))
        + f'<path transform="translate({num(64 + gap)} {num(baseline)})" d="{path}" fill="{word}"/>'
    )
    return svg(f"0 0 {num(total)} 64", body, title="Web Agentic Memory")


def favicon_svg():
    # Bolder strokes and bigger nodes so the W survives at 16px.
    inner = centered(glyph(CREAM, GREEN, stroke=6.6, start_r=4.2, end_r=6.4, ring_w=2.2))
    return svg("0 0 64 64", tile(inner), title="Web Agentic Memory")


def render_png(svg_text, px, out):
    with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as handle:
        handle.write(svg_text)
        source = handle.name
    subprocess.run(["rsvg-convert", "-w", str(px), "-h", str(px), source, "-o", str(out)], check=True)
    Path(source).unlink()


def apple_touch_icon():
    # Full-bleed square: iOS applies its own mask, so no rounded corners and a little more air.
    inner = f'<g transform="translate(32 32) scale(.78) translate(-32 -32)">{centered(glyph(CREAM, GREEN, stroke=5.6, start_r=3.8, end_r=6.2))}</g>'
    return svg("0 0 64 64", tile(inner, rx=0))


CARDS = {
    "en": {"h1": "Browser AI that<br>already knows<br><em>the way.</em>", "p": "Shared, verified how-tos that help AI agents finish web tasks in fewer steps.", "chip": "Leave a path. Move together.", "lang": "en"},
    "ko": {"h1": "이미 길을 아는<br><em>브라우저 AI.</em>", "p": "AI 에이전트가 웹 작업을 더 적은 단계로 끝내도록 돕는, 짧고 검증된 방법.", "chip": "길을 남기고, 함께 나아가요.", "lang": "ko"},
}


def social_card(fonts_dir, out, card):
    manrope = Path(fonts_dir) / "Manrope[wght].ttf"
    dm_sans = Path(fonts_dir) / "DMSans[opsz,wght].ttf"
    dm_mono = Path(fonts_dir) / "DMMono-Medium.ttf"
    mark = tile(centered(glyph(CREAM, GREEN)))
    html = f"""<!doctype html><html lang="{card["lang"]}"><meta charset="utf-8"><style>
@font-face {{ font-family: Manrope; src: url("file://{manrope}"); font-weight: 200 800; }}
@font-face {{ font-family: "DM Sans"; src: url("file://{dm_sans}"); font-weight: 100 1000; }}
@font-face {{ font-family: "DM Mono"; src: url("file://{dm_mono}"); font-weight: 500; }}
* {{ box-sizing: border-box; margin: 0; }}
body {{ width: 1200px; height: 630px; overflow: hidden; background: {GREEN_DARK}; color: {CREAM}; font-family: "DM Sans", sans-serif; position: relative; }}
.dots {{ position: absolute; inset: 0; background-image: radial-gradient(rgba(221,239,196,.32) 1px, transparent 1px); background-size: 26px 26px; -webkit-mask-image: linear-gradient(115deg, #000 0%, transparent 70%); }}
.ring {{ position: absolute; right: -170px; bottom: -330px; width: 760px; height: 760px; border: 1.5px solid rgba(208,235,126,.2); border-radius: 50%; box-shadow: 0 0 0 64px rgba(208,235,126,.035), 0 0 0 128px rgba(208,235,126,.025); }}
.brand {{ position: absolute; left: 72px; top: 64px; display: flex; align-items: center; gap: 16px; font: 800 30px Manrope; letter-spacing: -.035em; }}
.brand svg {{ width: 56px; height: 56px; }}
h1 {{ position: absolute; left: 72px; top: 178px; width: 760px; font: 800 84px/1.02 Manrope; letter-spacing: -.055em; }}
h1 em {{ font-style: normal; color: {LIME}; }}
p {{ position: absolute; left: 72px; top: 466px; width: 600px; font-size: 26px; line-height: 1.45; color: #bfd0c1; }}
.chip {{ position: absolute; left: 72px; bottom: 40px; font: 500 18px "DM Mono"; letter-spacing: .08em; text-transform: uppercase; color: #9db8a2; }}
.route {{ position: absolute; right: 52px; top: 74px; width: 440px; height: 480px; }}
html[lang=ko] p {{ word-break: keep-all; width: 640px; }}
html[lang=ko] .chip {{ letter-spacing: .02em; text-transform: none; font-size: 20px; }}
html[lang=ko] h1 {{ letter-spacing: -.045em; line-height: 1.12; }}
</style>
<div class="dots"></div><div class="ring"></div>
<div class="brand"><svg viewBox="0 0 64 64">{mark}</svg>{NAME}</div>
<h1>{card["h1"]}</h1>
<p>{card["p"]}</p>
<div class="chip">{card["chip"]}</div>
<svg class="route" viewBox="0 0 440 480" fill="none">
  <path d="M30 392C120 392 118 308 200 300S300 372 352 300s48-178 70-190" stroke="#789579" stroke-width="2" stroke-dasharray="6 12"/>
  <path d="M30 392C112 388 128 330 200 322S292 384 346 304 400 126 422 110" stroke="{LIME}" stroke-width="5" stroke-linecap="round"/>
  <circle cx="30" cy="392" r="13" fill="{LIME}" stroke="{GREEN_DARK}" stroke-width="6"/>
  <circle cx="200" cy="322" r="11" fill="{LIME}" stroke="{GREEN_DARK}" stroke-width="6"/>
  <circle cx="346" cy="304" r="11" fill="{LIME}" stroke="{GREEN_DARK}" stroke-width="6"/>
  <circle cx="422" cy="110" r="22" fill="{LIME}" stroke="{GREEN_DARK}" stroke-width="8"/>
</svg>"""
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False) as handle:
        handle.write(html)
        page = handle.name
    shot = Path(tempfile.mkdtemp()) / "og.png"
    subprocess.run(
        [CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
         "--window-size=1200,630", "--virtual-time-budget=3000", f"--screenshot={shot}", f"file://{page}"],
        check=True, capture_output=True,
    )
    Path(page).unlink()
    Image.open(shot).convert("RGB").save(out, optimize=True)
    shutil.rmtree(shot.parent, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fonts-dir", required=True, help="folder with Manrope[wght].ttf, DMSans[opsz,wght].ttf, DMMono-Medium.ttf")
    parser.add_argument("--skip-social", action="store_true", help="do not render og-image.png (needs Chrome)")
    args = parser.parse_args()
    manrope = Path(args.fonts_dir) / "Manrope[wght].ttf"
    if not manrope.is_file():
        sys.exit(f"missing {manrope}")

    BRAND.mkdir(parents=True, exist_ok=True)
    (BRAND / "wam-mark.svg").write_text(svg("0 0 64 64", tile(centered(glyph(CREAM, GREEN))), title="Web Agentic Memory"))
    # Tile-less glyphs: dark trail for light backgrounds, cream trail for dark ones.
    (BRAND / "wam-glyph.svg").write_text(
        svg("0 0 64 64", centered(glyph(GREEN, GREEN, start="#7da332", end=LIME, ring_w=2.4)), title="Web Agentic Memory")
    )
    (BRAND / "wam-glyph-reversed.svg").write_text(
        svg("0 0 64 64", centered(glyph(CREAM, GREEN_DARK)), title="Web Agentic Memory")
    )
    (BRAND / "wam-logo.svg").write_text(lockup(manrope, reversed_colors=False))
    (BRAND / "wam-logo-reversed.svg").write_text(lockup(manrope, reversed_colors=True))

    fav = favicon_svg()
    (STATIC / "favicon.svg").write_text(fav)
    render_png(apple_touch_icon(), 180, STATIC / "apple-touch-icon.png")
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp) / "fav256.png"
        render_png(fav, 256, base)
        Image.open(base).convert("RGBA").save(STATIC / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)])
    if not args.skip_social:
        social_card(args.fonts_dir, STATIC / "og-image.png", CARDS["en"])
        social_card(args.fonts_dir, STATIC / "og-image-ko.png", CARDS["ko"])
    print("wrote brand assets to", STATIC)


if __name__ == "__main__":
    main()
