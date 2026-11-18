# The disk image's window, for dmgbuild (package.sh runs it from the repository root with
# -D app=<the bundle> -D icon=<its .icns>): the app on the left, an arrow, Applications on the
# right, and nothing else (dmgbuild hides the toolbar, sidebar and status bar by default). It
# writes the window's layout itself; Finder never runs. dmgbuild provides `defines`.
files = [defines["app"]]  # noqa: F821
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]  # noqa: F821
# dmg-background@2x.png beside it is picked up for Retina screens.
background = "packaging/macos/dmg-background.png"
window_rect = ((200, 160), (640, 400))
text_size = 13
# 128-point icons, centred on the ends of the background's arrow.
icon_locations = {"Better Matrix.app": (170, 165), "Applications": (470, 165)}
