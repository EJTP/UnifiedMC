<div align="center">

<img src="brand/icon.png" width="104" alt="">

# UnifiedMC

**A launcher for one modded Minecraft server and the people who play on it.**

Pick a server. The client works out which Minecraft version and mod loader it runs,
pulls the mods and configs from the server itself, and drops you into the game.

</div>

---

Nobody installs a modpack by hand, and nobody ends up on the wrong version. Built for a
private group — it is not a general-purpose launcher and does not try to be one.

## What is in here

| | |
|---|---|
| `launcher/` | Desktop app. Tauri, Svelte, Rust — one binary, no Python needed. |
| `server/` | Server mod. Publishes what the server has over HTTP. |
| `hub/` | Client mod. Server list inside Minecraft, for the launcher-free route. |
| `unifiedmc.py` | The original shell. Still the reference for how everything behaves. |
| `brand/` | The mark, and the script that renders it. |

## How it works

The server is both the source of truth and the CDN. It scans what is installed, hashes it,
and serves it:

```
GET /unifiedmc.json    the manifest
GET /mods/<sha1>       a mod
GET /config/<sha1>     a config file
```

Paths in the manifest are relative, so a client resolves them against whatever address it
reached the server on. Behind a proxy or a port forward, nothing has to be configured.

```
mods/                      loaded here, and sent to clients
unifiedmc/client/          sent to clients, never loaded here
unifiedmc/client-config/   config that overrides config/, every launch
unifiedmc/server-only.txt  one name per line: things clients must not get
```

That `client/` directory is the point: a client-only mod in `mods/` is how a server dies on
startup, so shaders and minimaps live somewhere the loader does not scan.

## The constraint everything is shaped around

JVM mods cannot be added to a running Minecraft. Mixins apply at class load and registries
freeze during startup. "Download the mods and then join" always means starting a process
that already has them — the only question is whether the player notices.

## Getting started

```sh
cp env.example.sh env.sh   # then fill it in
./play.sh                  # straight to the server
```

Java is not required: Mojang publishes a JRE per Minecraft version and the launcher fetches
it. Mods are stored by hash and hard-linked into each instance, so the same jar on ten
servers costs one copy.

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
./server/build.sh                 # server mod — one javac call
cd launcher && pnpm tauri dev     # the app
cd launcher/src-tauri && cargo test
./unifiedmc.py demo               # the shell's self-check, no network
```

The [full documentation](docs/reference.md) covers the sync rules, the mod catalogue and how the
pieces talk to each other.

## Licence

MIT. See [LICENSE](LICENSE).

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
