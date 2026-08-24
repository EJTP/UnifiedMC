# Reference

The long version: sync rules, catalogue behaviour, and why each
piece works the way it does.

A launcher for one modded Minecraft server and the people who play on it.

You pick a server, and the client sorts itself out: it works out which Minecraft
version and mod loader that server runs, pulls the mods and configs straight from
the server, and drops you into the game. Nobody installs a modpack by hand, and
nobody ends up on the wrong version.

Built for a private group. It is not a general-purpose launcher and does not try
to be one.

## How it works

Two processes. A Python shell owns the JVM, and a Fabric mod runs inside it.

    shell starts the hub          real Minecraft, showing a server list
    player clicks a server        mod writes a handoff request and waits
    shell reads it                pings, resolves the manifest, syncs, launches
    target instance comes up      straight into the server, no menu
    player leaves the server      shell brings the hub back

The two talk through small files in `~/.unifiedmc`, written with write-then-rename
because both sides poll them while the other writes.

    servers.json    mod -> shell   the player's server list
    status.json     shell -> mod   what each of those servers actually is
    handoff.json    mod -> shell   "the player wants to go there"
    query.json      mod -> shell   catalogue search or install request
    catalog.json    shell -> mod   the answer
    progress.json   shell -> mod   what is happening during a long wait
    session.json    mod -> shell   the login of whatever launcher started the hub

### The constraint everything is shaped around

JVM mods cannot be added to a running Minecraft. Mixins apply at class load and
registries freeze during startup. "Download the mods and then join" always means
starting a process that already has them.

That is why the swap overlaps: the old instance stays on screen showing progress
until the new one reports its window is up, and only then is it closed. There is
never a frame with no window, which is the only part of a restart a player would
otherwise notice.

It is also why only the hub carries the mod. A Fabric jar is bound to one
Minecraft version through intermediary mappings, so a 1.21.11 build cannot load on
the 26.2 a server might ask for. Target instances get exactly what the server asked
for and nothing else.

## Server side

`server/` is a mod for the Minecraft server. It scans what is installed and serves
it over HTTP. Build it with `./server/build.sh` — one `javac` call, because it
touches no Minecraft class beyond the `@Mod` annotation. That also means porting it
to another loader or version is close to free.

    mods/                    loaded here, and sent to clients
    unifiedmc/client/        sent to clients, never loaded here
    unifiedmc/client-config/ config that overrides config/, every launch
    unifiedmc/server-only.txt  one name per line: things clients must not get

The `client/` directory is the point: a client-only mod in `mods/` is how a server
dies on startup, so shaders and minimaps live somewhere the loader does not scan.

`config/` becomes readable by anyone who can reach that port, with no token asked
for. Anything secret in there belongs in `server-only.txt` - a bare name matches a
file, and a name ending in `/` holds back a whole directory and everything under
it. The server prints the file count and says so at startup.

Endpoints:

    GET /unifiedmc.json    the manifest
    GET /mods/<sha1>       a mod
    GET /config/<sha1>     a config file

Paths in the manifest are relative, so the client resolves them against whatever
address it reached the server on. Behind a proxy or a port forward, nothing has to
be configured.

## Client side

    cp env.example.sh env.sh    # then fill it in
    ./play.sh                   # straight to the server
    ./unifiedmc.py hub          # the server list

Java is not required. Mojang publishes a JRE per Minecraft version and the launcher
fetches it.

Mods are stored by hash and hard-linked into each instance, so the same jar on ten
servers costs one copy. Configs are copied rather than linked, because they are
meant to be edited and a link would write the change back into the shared store.

A config is replaced only when the server's copy has changed since it was last
delivered. Otherwise whatever changed it locally keeps it — the player, or the mod
itself, which many rewrite on shutdown. Files the pack puts in `client-config/`
override that and win every launch.

### Your own mods

Each server gets a profile directory. Anything dropped in it is loaded alongside
what the server sends.

    ~/.unifiedmc/profiles/<server>/mods/

The in-game browser searches Modrinth and CurseForge, filtered to the server's
version and loader, and shows only mods a single player can actually add. A mod
marked `server_side: required` needs the server to carry it too, so it is left out;
that is the server owner's decision, not a browser's. Where CurseForge publishes no
side information, Modrinth is asked for a second opinion, and anything neither can
answer is not offered.

What the server already ships is marked rather than hidden. Hiding it does not
answer the question somebody has when they type "sodium".

Duplicates are caught three ways, because one is not enough: Modrinth by file hash,
CurseForge by its own fingerprint, and after download by the mod id the jar
declares. The last one is what actually prevents a crash — the same mod in two
versions shares neither a filename nor a hash.

## Signing in

Standard Microsoft authorization code flow with PKCE, scope
`XboxLive.signin offline_access`, redirect to a loopback listener on
`http://localhost:8398`. No client secret is embedded and the authorization code
never leaves the machine. The refresh token is stored owner-readable only.

Nothing about ownership or authentication is bypassed or weakened: the game is
launched with the player's own session, and an account that does not own Minecraft
cannot start it. Game files come from Mojang's own endpoints and are never
redistributed or modified.

Set `UNIFIEDMC_CLIENT_ID` to your own Azure application id. Using the Microsoft
authentication API from a third-party application requires the app to be approved
by Microsoft first; until then, or without the variable, the launcher runs on an
offline profile and can only reach offline-mode servers.

## Layout on disk

    ~/.unifiedmc/
      blobs/<sha1>          every mod file once, shared across servers
      mc/                   assets, libraries, versions, runtimes
      instances/<server>/   one game directory per server
      profiles/<server>/    mods the player added themselves

## Limits

- Fabric and NeoForge. Forge and Quilt are a few lines away but untested.
- The hub is pinned to one Minecraft version, so the no-restart path only applies
  to servers on that version.
- The catalogue cannot verify a mod that neither Modrinth nor CurseForge describes,
  and does not offer it.

## Development

    ./unifiedmc.py demo                   self-check, no network
    cd launcher && pnpm check             the desktop app

The server mod lives in its own repository, github.com/EJTP/UnifiedMC-Server. The client
mod described above is not in this repository either; point UNIFIEDMC_HUB_MOD at a built
jar to use the launcher-free route.

---

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
