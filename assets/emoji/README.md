# Noto emoji fonts

- `NotoEmoji.ttf` is Google Fonts' Noto Emoji 3.002 from
  [google/fonts](https://github.com/google/fonts/blob/main/ofl/notoemoji/NotoEmoji%5Bwght%5D.ttf).
  The Git blob is `c2c26ab612a88a8610ff9cfbb89299bf2aea6c7a`.
- `NotoColorEmoji.ttf` is Google's Noto Color Emoji 2.057 from
  [googlefonts/noto-emoji](https://github.com/googlefonts/noto-emoji/blob/main/2D/fonts/NotoColorEmoji.ttf).
  The Git blob is `e605743d05f261634f0b3f68a439d9b557d8f335`.
- `BetterMatrixNotoEmoji.ttf` uses Noto Emoji outlines where available and grayscale Noto
  Color Emoji bitmaps for the remaining glyphs. It keeps the color font's Unicode and ligature
  coverage.
- The files ending in `Sbix.ttf` keep their counterparts' artwork and shaping tables.
  They use Apple's [sbix](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6sbix.html)
  bitmap format for iOS.

Run [the font build script](../../scripts/noto-emoji-fonts.py) from the repository root to
reproduce the derived fonts. It uses FontTools, Pillow, uharfbuzz and the installed emojibase data.

The fonts and the iOS conversion are distributed under the included SIL Open Font License 1.1.
