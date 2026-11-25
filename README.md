# shoturscreen

Fast region screenshot tool for KDE Plasma on Wayland. Press the shortcut, drag a region, and the image lands in your clipboard. No popups, no notifications, no files.

It replaces Spectacle on Meta+Shift+S. Spectacle keeps the Print key.

## How it works

The screen is captured through KWin's ScreenShot2 D-Bus interface at native resolution, one image per monitor. A layer shell overlay shows the frozen screen, you drag a rectangle, and the crop is offered to the clipboard as image/png. A small background process keeps serving the clipboard until something else replaces it, the same way wl-copy does.

## Controls

| Input | Action |
| --- | --- |
| Left drag | Select a region, release to copy |
| Esc or right click | Cancel |

## Requirements

KDE Plasma 6 on Wayland. Building from source needs Rust and libxkbcommon.

## Install

From a release archive:

```sh
tar xf shoturscreen-x86_64-linux.tar.gz
cd shoturscreen
./install.sh
```

From source:

```sh
git clone https://github.com/xRookieFight/shoturscreen
cd shoturscreen
./install.sh
```

The script installs the binary to ~/.local/bin, adds a desktop entry that grants access to KWin's screenshot interface, moves Meta+Shift+S from Spectacle to shoturscreen and leaves Print on Spectacle.

## Uninstall

```sh
./uninstall.sh
```

This removes the files and gives Meta+Shift+S back to Spectacle.

## License

MIT
