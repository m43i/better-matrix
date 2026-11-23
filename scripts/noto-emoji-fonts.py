"""Build Better Matrix's Noto Emoji variants from the upstream fonts.

Run from the repository root after installing FontTools, Pillow and uharfbuzz.
"""

from copy import copy
from io import BytesIO
from itertools import groupby
from json import loads
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables._g_l_y_f import Glyph
from fontTools.ttLib.tables.sbixGlyph import Glyph as BitmapGlyph
from fontTools.ttLib.tables.sbixStrike import Strike
from PIL import Image, ImageOps


ASSETS = Path("assets/emoji")
COLOR = ASSETS / "NotoColorEmoji.ttf"
OUTLINES = ASSETS / "NotoEmoji.ttf"
SEQUENCES = Path("node_modules/emojibase-data/meta/hexcodes.json")


def rename(font, family, postscript):
    for record in font["name"].names:
        replacement = {1: family, 4: family, 6: postscript}.get(record.nameID)
        if replacement:
            record.string = replacement


def add_outlines(font):
    order = font.getGlyphOrder()
    font["glyf"] = newTable("glyf")
    font["glyf"].setGlyphOrder(order)
    font["glyf"].glyphs = {name: Glyph() for name in order}
    font["loca"] = newTable("loca")


def to_sbix(font, destination):
    sbix = newTable("sbix")
    for locator, bitmaps in zip(font["CBLC"].strikes, font["CBDT"].strikeData, strict=True):
        strike = Strike(ppem=locator.bitmapSizeTable.ppemY)
        for name, bitmap in bitmaps.items():
            bitmap.ensureDecompiled()
            metrics = bitmap.metrics
            strike.glyphs[name] = BitmapGlyph(
                glyphName=name,
                originOffsetX=metrics.BearingX,
                originOffsetY=metrics.BearingY - metrics.height,
                graphicType="png ",
                imageData=bitmap.imageData,
            )
        sbix.strikes[strike.ppem] = strike
    font["sbix"] = sbix
    if "glyf" not in font:
        add_outlines(font)
    del font["CBDT"]
    del font["CBLC"]
    font.save(destination)


def shape_single(font, sequence):
    buffer = hb.Buffer()
    buffer.add_str(sequence)
    buffer.guess_segment_properties()
    hb.shape(font, buffer)
    visible = [
        glyph.codepoint
        for glyph, position in zip(buffer.glyph_infos, buffer.glyph_positions, strict=True)
        if position.x_advance or glyph.codepoint == 0
    ]
    return visible[0] if len(visible) == 1 and visible[0] else None


def matching_outlines(color, mono):
    color_shaper = hb.Font(hb.Face(COLOR.read_bytes()))
    mono_shaper = hb.Font(hb.Face(OUTLINES.read_bytes()))
    variants = loads(SEQUENCES.read_text(encoding="utf-8")).values()
    for hexcode in dict.fromkeys(hexcode for group in variants for hexcode in group):
        sequence = "".join(chr(int(code, 16)) for code in hexcode.split("-"))
        color_glyph = shape_single(color_shaper, sequence)
        mono_glyph = shape_single(mono_shaper, sequence)
        if color_glyph is None or mono_glyph is None:
            continue
        yield color.getGlyphName(color_glyph), mono.getGlyphName(mono_glyph)


def bitmap_tables(font, tables, outlined):
    for table in tables:
        names = [name for name in table.names if name not in outlined]
        # A format 1 index covers a range. Keep sparse glyphs in separate ranges so a
        # rasterizer never reads the next bitmap for an outline at a zero-length offset.
        for _, run in groupby(enumerate(names), key=lambda entry: font.getGlyphID(entry[1]) - entry[0]):
            subset = copy(table)
            subset.names = [name for _, name in run]
            yield subset


def monochrome(font):
    outline_font = TTFont(OUTLINES)
    assert font["head"].unitsPerEm == outline_font["head"].unitsPerEm
    outline_set = outline_font.getGlyphSet(location={"wght": 400})
    add_outlines(font)
    glyf = font["glyf"]
    metrics = font["hmtx"].metrics
    outlined = set()
    for target, source in matching_outlines(font, outline_font):
        if target in outlined:
            continue
        record = DecomposingRecordingPen(outline_set)
        outline_set[source].draw(record)
        if not record.value:
            continue
        advance, _ = metrics[target]
        offset = (advance - outline_set[source].width) / 2
        pen = TTGlyphPen(None)
        record.replay(TransformPen(pen, (1, 0, 0, 1, offset, 0)))
        glyph = pen.glyph()
        glyph.recalcBounds(glyf)
        glyf[target] = glyph
        metrics[target] = advance, glyph.xMin
        outlined.add(target)

    for locator, strike in zip(font["CBLC"].strikes, font["CBDT"].strikeData, strict=True):
        locator.indexSubTables = list(bitmap_tables(font, locator.indexSubTables, outlined))
        for name in outlined & strike.keys():
            del strike[name]
        for bitmap in strike.values():
            bitmap.ensureDecompiled()
            with Image.open(BytesIO(bitmap.imageData)) as source:
                rgba = source.convert("RGBA")
            gray = ImageOps.grayscale(rgba).convert("RGBA")
            gray.putalpha(rgba.getchannel("A"))
            with BytesIO() as result:
                gray.save(result, format="PNG", optimize=True)
                bitmap.imageData = result.getvalue()

    rename(font, "Better Matrix Noto Emoji", "BetterMatrixNotoEmoji")


if __name__ == "__main__":
    color = TTFont(COLOR, recalcTimestamp=False)
    rename(color, "Better Matrix Color Emoji", "BetterMatrixColorEmoji")
    to_sbix(color, ASSETS / "BetterMatrixColorEmojiSbix.ttf")

    mono = TTFont(COLOR, recalcTimestamp=False)
    monochrome(mono)
    mono.save(ASSETS / "BetterMatrixNotoEmoji.ttf")
    to_sbix(mono, ASSETS / "BetterMatrixNotoEmojiSbix.ttf")
