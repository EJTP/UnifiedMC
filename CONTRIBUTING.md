# Contributing

This is a small project for a small group. Patches are welcome; so is being told a decision
here is wrong.

## Running things

```sh
./unifiedmc.py demo               # the shell's self-check, no network needed
cd launcher/src-tauri && cargo test
cd launcher && pnpm tauri dev
./server/build.sh                 # also runs its own assertions
```

## What the tests are for

Every non-obvious rule has one, and they are meant to fail loudly rather than cover lines.
The interesting ones:

- **`servers::varints_round_trip`** — the ping protocol, hand-rolled.
- **`catalogue::murmur2_*`** — a hash reimplementation. A wrong constant still returns
  plausible numbers, so the values are pinned to what CurseForge itself agreed with.
- **`settings::memory_scales_*`** — heap sizing, capped so an explicit choice cannot push the
  machine into swap.
- **`pack::mrpack_carries_the_side_split`** — which mods are client-only. Getting this wrong
  puts a client mod in a server's `mods/`, which is how a server dies on startup.

## Style

Comments explain *why*, not *what*. If a line looks odd, the comment says what goes wrong
without it — several here exist because something actually went wrong.
