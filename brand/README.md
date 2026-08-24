# Brand

The mark is a block, drawn rather than borrowed.

`render.py` renders it; `textures.py` holds the 16x16 face textures. Both are ours -
Minecraft's own textures belong to Mojang, and shipping them from a public repository
would be redistributing them.

Geometry follows how the game draws block icons: `rotation [30, 225, 0]`, orthographic.
Working that out gives a 2:1 top face and a vertical axis foreshortened to 0.866, so the
side height is 1.2247 half-widths. Picking that ratio by eye gives a squat block; this is
the number.

    python brand/render.py                       # icon.png and mark.png
    cd launcher && pnpm tauri icon src-tauri/icons/source.png

Face textures live in `textures.py`, one function each, returning a 16x16 grid of
brightness multipliers. `panel` is the one in use.
