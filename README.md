<div align="center">

<img src="brand/icon.png" width="104" alt="">

# UnifiedMC

**A launcher for one modded Minecraft server and the people who play on it.**

Pick a server. The client works out which Minecraft version and mod loader it runs,
pulls the mods and configs from the server itself, and drops you into the game.

</div>

<!--
  Screenshot goes here. Drop a PNG of the running app at docs/screenshot.png
  (dark theme, the server list with one server online and its MOTD rendered),
  then uncomment the line below. Nothing else references that path.
-->
<!-- <div align="center"><img src="docs/screenshot.png" width="860" alt="The launcher's server list"></div> -->

---

Nobody installs a modpack by hand, and nobody ends up on the wrong version. Built for a
private group — it is not a general-purpose launcher and does not try to be one.

## The constraint everything is shaped around

JVM mods cannot be added to a running Minecraft. Mixins apply at class load and registries
freeze during startup. "Download the mods and then join" therefore always means starting a
process that already has them — the only question is whether the player has to be the one
who arranges that. Here they do not: the arranging happens between the click and the window.

## How it works

```mermaid
sequenceDiagram
    autonumber
    participant P as Player
    participant L as Launcher
    participant S as Server
    P->>L: clicks a server
    L->>S: status ping on the game port
    S-->>L: MOTD, players, protocol version
    L->>S: GET /unifiedmc.json
    S-->>L: minecraft, loader, mods, config
    L->>S: GET /mods/... for every hash not stored yet
    L->>L: hard-link mods, copy config, add the profile's own mods
    L->>P: the JVM starts with the pack already in place
```

The server is both the source of truth and the CDN — no Modrinth lookup, no guessing which
project a jar came from:

```
GET /unifiedmc.json    the manifest
GET /mods/<sha1>       a mod
GET /config/<sha1>     a config file
```

Paths in the manifest are relative, so a client resolves them against whatever address it
reached the server on. Behind a proxy or a port forward, nothing has to be configured. The
publisher lives in its own repository: [**UnifiedMC-Server**](https://github.com/EJTP/UnifiedMC-Server).

## Servers that publish nothing

Vanilla, Paper, or anything without the mod announces no pack and often no usable version
either — a proxy answers the ping with the *oldest* protocol it accepts, which pins a player
to 1.8.9 on a server that happily takes 1.21. So detection is a suggestion, not a verdict.

```mermaid
flowchart LR
    A[Ping answered] --> B{Publishes a pack?}
    B -- yes --> C[Sync it, launch]
    B -- no --> D[Ask which profile to join with]
    D --> E[An instance on that version]
    D --> F[New profile, version and loader prefilled]
```

Every server row has a setup action: pick the Minecraft version from the full release list,
pick the loader, or leave both automatic. When the server runs no loader of its own, any
instance on the same version fits — client-side mods do not need the server to know about
them.

## Running a server yourself

The third tab builds one on this machine and runs it. Drop in a `.mrpack` or a CurseForge zip,
or pick a version and a loader and get a bare server. Java is not a prerequisite there either —
the JRE Mojang publishes per version is already on disk from playing, and the server runs on the
same one.

```mermaid
flowchart LR
    A[Modpack, or a version] --> B[Split by side]
    B --> C[Loader server installed]
    C --> D[Publisher mod into mods/]
    D --> E[Start · console · players]
    E --> F[A friend adds the address]
    F --> G[Their launcher pulls the pack, then starts the game]
```

The last two steps are the point. **Let anyone join without installing mods** puts the publisher
mod in `mods/`, so the server hands its pack to whoever connects — a friend with an empty mods
folder types the address and is in. That is the same manifest a remote server publishes, served
from a machine in the room.

The console is live: output as it prints, a command box that types straight at the server's
stdin, and the player list read out of what it says. Mods added from the browser are sorted by
side — anything Modrinth calls client-only goes to `unifiedmc/client/` rather than `mods/`,
because a client-only jar in `mods/` takes the start down.

Closing the launcher stops what it started and waits for the worlds to save. Servers live in
`~/.unifiedmc/hosted/<id>/`, and the folder button opens it — `server.properties`, the world,
and the crash reports are all where they always are.

The [CLI](#the-server-side-companion) writes the same directory through the same code, for a
server that ships to a machine that never runs this launcher.

## Playtime

Every launch is timed — the call that starts the game does not return until the window closes,
so the length of a session is the length of that call. Each server and instance carries its
total and when it was last played, the lists sort most-recent-first, and the sidebar keeps a
running fortnight with a trend beside it. Kept in `~/.unifiedmc/playtime.json`, bucketed by
whole days since the epoch — a sparkline needs buckets, not calendars, and integers mean no
date library on either side.

A session under a minute counts its seconds but not as a round: that one is a crash on start,
not an evening.

## Updates and releases

The launcher asks GitHub once at start whether the latest release tag is newer than its own
version, and says so in the title bar. Pressing it opens the release, and offers the file for
the machine it is running on. It does not replace the running binary — that needs signing keys
and a per-platform manifest, and neither exists yet.

Tagging `v*` builds and attaches:

| | |
|---|---|
| Windows | an `.msi` and an NSIS `-setup.exe`, plus `unifiedmc-server-cli.exe` |
| macOS | one universal `.dmg` — Intel and Apple Silicon download the same file — plus a universal `unifiedmc-server-cli-macos` |
| Linux | an `.AppImage`, a `.deb` and an `.rpm`, plus `unifiedmc-server-cli-linux` |

The Linux build is made on Ubuntu 22.04 rather than the newest runner: a binary links against
the glibc it was built on and runs on that version or newer, never older, so building on the
newest thing available is how a package comes out refusing to start on every distro a year
behind it.

**The macOS build is not signed.** There is no Apple Developer ID behind this project, so the
first open needs right-click → Open rather than a double-click, or:

```sh
xattr -dr com.apple.quarantine /Applications/UnifiedMC.app
```

To sign, add `APPLE_CERTIFICATE` (base64 of the `.p12`), `APPLE_CERTIFICATE_PASSWORD` and
`APPLE_SIGNING_IDENTITY` as an `env:` block on the build step in `release.yml`, plus
`APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` to notarise. They are deliberately not wired
up in advance: Tauri decides to sign by whether those variables are *set*, not by whether they
hold anything, so pointing them at secrets that do not exist yet fails the build outright.

## What is in here

| | |
|---|---|
| `launcher/` | The desktop app. Tauri, Svelte, Rust — one binary, no Python needed. |
| `unifiedmc.py` | The original shell. Still the reference for how everything behaves. |
| `docs/` | [Reference](docs/reference.md) and the [remote-control rules](docs/remote-control.md). |
| `brand/` | The mark, and the script that renders it. |

The server mod used to live in `server/`. It is now [its own repository](https://github.com/EJTP/UnifiedMC-Server),
because it ships to a different machine, on a different schedule, to people who never run
this launcher.

## Getting started

```sh
cd launcher
pnpm install
pnpm tauri dev
```

You need Node with pnpm, a Rust toolchain, and Tauri's system dependencies. Java you do not
need: Mojang publishes a JRE per Minecraft version and the launcher fetches the right one.

Then add a server by address. It is pinged straight away; if it publishes a pack, pressing
play is the whole procedure.

Mods are stored by hash and hard-linked into each instance, so the same jar on ten servers
costs one copy. Configs are copied instead — they are meant to be edited, and a link would
write the change back into the shared store.

```
~/.unifiedmc/
  blobs/<sha1>          every mod file once, shared across servers
  mc/                   assets, libraries, versions, runtimes
  instances/<key>/      one game directory per server or profile
  profiles/<key>/mods/  mods the player added themselves
```

## Your own mods

Each server and each profile has a directory in `profiles/`. Anything dropped there is
loaded alongside what the server sends, and the server never learns about it.

The built-in browser searches Modrinth and CurseForge, filtered to the version and loader in
play, and offers only mods a single player can actually add — a mod marked
`server_side: required` needs the server to carry it too, so it is left out. What the server
already ships is marked rather than hidden.

Two tabs: the catalogue, and what is actually here. The second merges what the server ships
with what you added yourself, because that is one question; whether a thing can be taken away
again is a property of the row, not of which tab you found it on. Every row carries its own
Install or Remove button — there is no selection mode, so a highlight cannot mean "install"
on one tab and "delete" on another. Rows say who wrote it, which build, where it came from and
how many downloads, and link out to the project's own page. The catalogue can be ordered by
best match, downloads, followers, recently updated or newest.

## Settings

Memory is a slider in 512 MB steps with an automatic mode that follows the size of the pack,
capped so an explicit choice cannot push the machine into swap. The collector is a choice
between G1 tuned for short pauses, ZGC where the heap and the cores are there for it, the
JVM's own defaults, and your own flags. The exact command-line the game will get is shown
read-only underneath — it is generated by the same function that launches the game, so it
cannot drift from what actually happens.

The interface is German or English, following the system by default, in one of six accents —
buttons, focus rings, chips and the glow behind the window all follow the pair you pick.

## The server-side companion

A CLI in this repository turns a modpack into a server directory and changes a running
server without SFTP, sharing the launcher's pack reader so "which side is this mod for" is
answered in one place.

```sh
cd launcher/src-tauri
cargo run --bin unifiedmc-server-cli -- init pack.mrpack
cargo run --bin unifiedmc-server-cli -- push mc.example.com:25566 sodium.jar
```

What that endpoint may and may not do is written down in [docs/remote-control.md](docs/remote-control.md);
it is off unless a token is configured.

## Signing in

Standard Microsoft authorization code flow with PKCE, scope `XboxLive.signin offline_access`,
redirecting to a loopback listener. No client secret is embedded and the authorization code
never leaves the machine.

Nothing about ownership or authentication is bypassed or weakened: the game launches with the
player's own session, and an account that does not own Minecraft cannot start it. Game files
come from Mojang's own endpoints and are never redistributed or modified.

Using the Microsoft authentication API from a third-party application requires the app to be
approved by Microsoft first. Until then the launcher runs on an offline profile, which only
reaches offline-mode servers.

## Development

```sh
cd launcher && pnpm check                              # svelte-check
cd launcher/src-tauri && cargo clippy --all-targets -- -D warnings
cd launcher/src-tauri && cargo test
./unifiedmc.py demo                                    # the shell's self-check, no network
```

One test is ignored by default, because it fetches Mojang's server jar and starts a JVM. It is
the only thing that proves hosting works end to end, so run it after touching `host.rs`:

```sh
cd launcher/src-tauri
cargo test --lib host::tests::a_vanilla_server_is_built_and_runs -- --ignored --nocapture
```

Tests exist for the rules that would fail quietly: the hand-rolled ping protocol, the
CurseForge hash reimplementation, heap sizing, and which side of a pack each mod belongs to.
[CONTRIBUTING.md](CONTRIBUTING.md) says what each of them is guarding.

## How this was made

The backend is mine. Most of the rest was written with AI, across a lot of long sessions, and I
would rather say so plainly than have somebody work it out from the commit history.

It works, and it can be better. If you want to fix something, add a loader, catch a case I did
not think of, or tell me where this is wrong: issues and pull requests are both welcome.

## Licence

MIT. See [LICENSE](LICENSE).

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
