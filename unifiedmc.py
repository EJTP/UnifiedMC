#!/usr/bin/env python3
"""UnifiedMC - join any server, mods sync themselves, no launcher in the player's face.

Flow:  ping server -> read manifest -> sync missing mods -> launch with --quickPlayMultiplayer

Run:   ./unifiedmc.py play mc.example.com
       ./unifiedmc.py ping mc.example.com
       ./unifiedmc.py demo          # self-check, no network
"""
from __future__ import annotations

import base64
import hashlib
import http.server
import io
import json
import os
import shutil
import socket
import re
import struct
import subprocess
import threading
import time
import zipfile
import sys
import tomllib
import urllib.parse
import urllib.request
import webbrowser
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import minecraft_launcher_lib as mll

DATA = Path(os.environ.get("UNIFIEDMC_HOME", Path.home() / ".unifiedmc"))
BLOBS = DATA / "blobs"          # content-addressed, shared by every server
MC = DATA / "mc"                # shared assets/libraries/versions
INSTANCES = DATA / "instances"  # per-server game dir, isolated mods/

# where the server mod publishes. Panels hand out fixed allocations, so this has to be settable.
MANIFEST_PORT = int(os.environ.get("UNIFIEDMC_MANIFEST_PORT", "25566"))
MANIFESTS = DATA / "manifests"    # hand-written fallbacks for servers that publish nothing yet
PROFILES = DATA / "profiles"      # per server: mods the player brings along themselves
HANDOFF = DATA / "handoff.json"   # client -> shell: "the player wants that server"
SERVERS = DATA / "servers.json"   # client -> shell: the player's server list, to scout ahead
DIRECT = DATA / "direct.json"     # shell -> client: servers this instance can serve as is
STATUS = DATA / "status.json"     # shell -> client: what each server in the list actually is
QUERY = DATA / "query.json"       # client -> shell: "search the catalogue for this"
CATALOG = DATA / "catalog.json"   # shell -> client: the answer

# Logged once the window is up and rendering. Until then the old instance has to stay on screen.
VISIBLE = re.compile(rb"Sound engine started|Backend library: LWJGL")
LAUNCH_TIMEOUT = 180

ACCOUNT = DATA / "account.json"   # holds a refresh token - treated as the credential it is
SESSION = DATA / "session.json"   # a live session handed over by whatever launcher started the hub
UI = DATA / "ui.json"             # what the player chose in the hub: size, sound, memory
PROGRESS = DATA / "progress.json"  # shell -> client: what is happening while the player waits
ICONS = DATA / "icons"            # catalogue artwork, fetched here so the screen can just draw it
CLIENT_ID = os.environ.get("UNIFIEDMC_CLIENT_ID", "")
REDIRECT_PORT = int(os.environ.get("UNIFIEDMC_REDIRECT_PORT", "8398"))
REDIRECT_URI = f"http://localhost:{REDIRECT_PORT}"

HUB_VERSION = os.environ.get("UNIFIEDMC_HUB_VERSION", "1.21.11")
HUB_MOD = Path(os.environ.get("UNIFIEDMC_HUB_MOD",
                              Path(__file__).parent / "hub/build/libs/unifiedmc-hub-0.1.0.jar"))


# --- minecraft server list ping (stdlib only) -------------------------------

def _varint(n: int) -> bytes:
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        out += bytes([b | 0x80 if n else b])
        if not n:
            return out


def _read_varint(read) -> int:
    n = shift = 0
    while True:
        b = read(1)[0]
        n |= (b & 0x7F) << shift
        if not b & 0x80:
            return n
        shift += 7
        if shift > 35:
            raise ValueError("varint too long")


def _packet(pid: int, payload: bytes) -> bytes:
    body = _varint(pid) + payload
    return _varint(len(body)) + body


def srv_lookup(host: str) -> tuple[str, int] | None:
    """Most public servers advertise a SRV record, exactly as the vanilla client expects."""
    try:
        import dns.exception
        import dns.resolver
    except ImportError:
        return None
    try:
        answers = dns.resolver.resolve(f"_minecraft._tcp.{host}", "SRV")
    except dns.exception.DNSException:
        return None
    # deliberately not wrapped: a wrong attribute here silently disables SRV for every server,
    # which is exactly the bug a broad except once hid
    return srv_pick(answers)


def srv_pick(records):
    """Lowest priority wins, highest weight breaks the tie - RFC 2782."""
    best = min(records, key=lambda r: (r.priority, -r.weight))
    return str(best.target).rstrip("."), best.port


def ping(host: str, port: int = 25565, timeout: float = 5.0) -> dict:
    """Vanilla status ping. Returns the server's JSON (version, players, motd, ...)."""
    connect_to = (host, port)
    if port == 25565:   # only then does the vanilla client consult SRV, so neither do we
        connect_to = srv_lookup(host) or connect_to
    # the handshake still carries the name the player typed - servers use it for virtual hosting
    addr = host.encode()
    # protocol 0 = "just pinging"; next_state 1 = status
    hs = _varint(0) + _varint(len(addr)) + addr + struct.pack(">H", port) + _varint(1)
    with socket.create_connection(connect_to, timeout) as s:
        s.sendall(_packet(0x00, hs) + _packet(0x00, b""))
        f = s.makefile("rb")
        _read_varint(f.read)                      # packet length
        if _read_varint(f.read) != 0x00:
            raise ValueError("unexpected status packet")
        return json.loads(f.read(_read_varint(f.read)).decode("utf-8"))


# --- manifest ---------------------------------------------------------------

def fetch_manifest(host: str, port: int, status: dict) -> dict:
    """What this server needs client-side. The server is the single source of truth.

    Looked up cheapest first:
      1. a "unifiedmc" key the server mod put in the ping response
      2. http://host:25566/unifiedmc.json - the server mod serving its own mods folder
      3. ~/.unifiedmc/manifests/<host>_<port>.json - hand written, for servers not running it yet
    None of them -> vanilla server, nothing to sync.
    """
    # The game port first: the server mod answers HTTP there as well, so nobody has to know
    # about a second one. The configured port stays as a way out for anyone behind a proxy
    # that will not pass what it does not recognise.
    for candidate in dict.fromkeys([port, MANIFEST_PORT]):
        base = f"http://{host}:{candidate}/"
        if isinstance(status.get("unifiedmc"), dict):
            return normalize(status["unifiedmc"], status, base=base)
        try:
            with urllib.request.urlopen(base + "unifiedmc.json", timeout=5) as r:
                return normalize(json.load(r), status, base=base)
        except Exception:
            continue
    local = MANIFESTS / f"{host}_{port}.json"
    if local.is_file():
        return normalize(json.loads(local.read_text()), status)
    return normalize({}, status)


PROTOCOL_URL = ("https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/"
                "data/pc/common/protocolVersions.json")


def protocol_table() -> dict[int, str]:
    """protocol number -> release version. Cached on disk; fetched only when missing."""
    cache = DATA / "protocolVersions.json"
    if not cache.is_file():
        cache.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(PROTOCOL_URL, timeout=15) as response:
            cache.write_bytes(response.read())
    table: dict[int, str] = {}
    for entry in json.loads(cache.read_text()):
        # newest first in the file; keep the first release seen so 772 -> 1.21.8, not 1.21.7
        if entry.get("releaseType", "release") == "release":
            table.setdefault(entry["version"], entry["minecraftVersion"])
    return table


def normalize(m: dict, status: dict, table: dict[int, str] | None = None,
              base: str | None = None) -> dict:
    """Fill the blanks from the ping. A vanilla server still yields a launchable manifest.

    version.name is free text ("We support: 1.20-1.21") and must never be parsed.
    version.protocol is authoritative. On a ViaVersion proxy it reports the OLDEST
    accepted protocol -- which the proxy accepts by definition, so it still connects.
    """
    version = m.get("minecraft")
    if not version:
        proto = status.get("version", {}).get("protocol")
        table = protocol_table() if table is None else table
        version = table.get(proto)
        if not version:
            raise ValueError(f"cannot resolve MC version for protocol {proto}")
    # the server publishes "/mods/<sha1>", not an absolute url: it has no idea which hostname,
    # port forward or proxy the client reached it through, and it does not need to
    mods = [{**mod, "url": urllib.parse.urljoin(base, mod["url"])} if base and mod.get("url")
            else mod
            for mod in m.get("mods", [])]
    config = [{**item, "url": urllib.parse.urljoin(base, item["url"])} if base and item.get("url")
              else item
              for item in m.get("config", [])]
    return {
        "minecraft": version,
        "loader": m.get("loader"),  # {"type": "fabric"|"neoforge", "version": "..."} or None
        "mods": mods,               # [{"name", "sha1", "url"?}]
        "config": config,           # [{"path", "sha1", "url"?}]
    }


# --- content-addressed mod sync --------------------------------------------

def sha1_file(p: Path) -> str:
    h = hashlib.sha1()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def have(sha1: str) -> bool:
    return (BLOBS / sha1).is_file()


def download(mod: dict) -> None:
    """Fetch into the shared blob store. Verified by hash, so a corrupt file never sticks."""
    if not mod.get("url"):
        raise ValueError(f"{mod['name']}: not in the blob store and the manifest gives no url")
    BLOBS.mkdir(parents=True, exist_ok=True)
    tmp = BLOBS / f".{mod['sha1']}.part"
    with urllib.request.urlopen(mod["url"], timeout=60) as r, tmp.open("wb") as f:
        shutil.copyfileobj(r, f)
    got = sha1_file(tmp)
    if got != mod["sha1"]:
        tmp.unlink(missing_ok=True)
        raise ValueError(f"{mod['name']}: hash mismatch (got {got}, want {mod['sha1']})")
    tmp.rename(BLOBS / mod["sha1"])


def sync_mods(mods: list[dict], mods_dir: Path) -> dict:
    """Make mods_dir contain exactly `mods`. Returns a small report."""
    missing = [m for m in mods if not have(m["sha1"])]
    if missing:
        done = 0
        progress("Mods werden geladen", f"{len(missing)} fehlen", 0, len(missing))

        def fetch(mod):
            nonlocal done
            download(mod)
            done += 1
            progress("Mods werden geladen", mod["name"], done, len(missing))

        with ThreadPoolExecutor(max_workers=8) as pool:
            list(pool.map(fetch, missing))

    mods_dir.mkdir(parents=True, exist_ok=True)
    wanted = {m["name"]: m["sha1"] for m in mods}
    for stale in mods_dir.iterdir():                 # server dropped a mod -> so do we
        if stale.name not in wanted:
            stale.unlink()
    for name, sha1 in wanted.items():
        link(BLOBS / sha1, mods_dir / name)
    return {"total": len(mods), "downloaded": len(missing), "cached": len(mods) - len(missing)}


def link(src: Path, dst: Path) -> None:
    """Hardlink so N servers sharing a mod cost one copy on disk."""
    if dst.exists():
        dst.unlink()
    try:
        os.link(src, dst)
    except OSError:                                   # cross-device or no-link filesystem
        shutil.copy2(src, dst)


def sync_config(entries: list[dict], inst: Path) -> None:
    """Write the server's config files into the instance.

    A config the player edited is theirs. We only replace one that still matches what we last
    delivered, so an admin's update lands but a personal tweak is never silently undone.
    """
    if not entries:
        return
    ledger = inst / ".unifiedmc-config.json"
    try:
        delivered = json.loads(ledger.read_text())
    except (OSError, ValueError):
        delivered = {}

    written = kept = 0
    for entry in entries:
        relative = Path(entry["path"])
        if relative.is_absolute() or ".." in relative.parts:
            print(f"  refusing config path {entry['path']!r}", file=sys.stderr)
            continue

        target = inst / "config" / relative
        if target.exists():
            current = sha1_file(target)
            if current == entry["sha1"]:
                delivered[entry["path"]] = current
                continue

            # Marked in the pack as an override: it wins every launch. Mods rewrite their own
            # config on shutdown - FancyMenu drops a screen it never got to show - so a file the
            # pack deliberately overrides has to be restored, not treated as a player edit.
            # The only question that matters: has the server moved since we last delivered this?
            # If not, whatever changed the file locally - the player or the mod itself - keeps it.
            if not entry.get("force") and delivered.get(entry["path"]) == entry["sha1"]:
                kept += 1
                continue

        if not have(entry["sha1"]):
            download(entry)
        target.parent.mkdir(parents=True, exist_ok=True)
        # a copy, never a hardlink: this file is meant to be edited, and editing a link would
        # rewrite the shared blob for every other server too
        shutil.copy2(BLOBS / entry["sha1"], target)
        delivered[entry["path"]] = entry["sha1"]
        written += 1

    ledger.write_text(json.dumps(delivered, indent=2, sort_keys=True))
    if written or kept:
        print(f"  config: {written} written, {kept} left alone")


def blob_of(path: Path) -> dict:
    """Take a jar we already have on disk into the blob store. Returns a manifest entry."""
    sha1 = sha1_file(path)
    BLOBS.mkdir(parents=True, exist_ok=True)
    if not (BLOBS / sha1).exists():
        shutil.copy2(path, BLOBS / sha1)
    return {"name": path.name, "sha1": sha1}


def modrinth_jar(slug: str, mc_version: str, loader: str = "fabric") -> dict:
    versions = _modrinth(f"/project/{slug}/version", {
        "game_versions": json.dumps([mc_version]), "loaders": json.dumps([loader])})
    if not versions:
        raise ValueError(f"no {slug} for {mc_version} / {loader}")
    file = versions[0]["files"][0]
    return {"name": file["filename"], "sha1": file["hashes"]["sha1"], "url": file["url"]}


def hub_mods(mc_version: str) -> list[dict]:
    """What makes the hub the hub.

    Hub only, never a target instance: a Fabric jar is bound to one Minecraft version through
    intermediary mappings and fabric-api, so a 1.21.11 build cannot load on the 26.2 a server
    might ask for. The hub therefore stays pinned to HUB_VERSION and the shell owns the loop
    instead - it relaunches the hub when the player leaves a server.
    """
    if not HUB_MOD.is_file():
        raise FileNotFoundError(f"hub mod not built: {HUB_MOD} (run gradle build in hub/)")
    return [blob_of(HUB_MOD),
            modrinth_jar("fabric-api", mc_version),
            modrinth_jar("owo-lib", mc_version)]   # the screens are built on owo-ui


def mod_id(jar: Path) -> str | None:
    """The id a mod declares for itself.

    Deduplicating by filename or hash is not enough: the same mod in two versions has neither in
    common, and loading it twice is a crash rather than a preference.
    """
    try:
        with zipfile.ZipFile(jar) as archive:
            names = set(archive.namelist())
            for entry in ("META-INF/neoforge.mods.toml", "META-INF/mods.toml"):
                if entry in names:
                    declared = tomllib.loads(archive.read(entry).decode("utf-8"))
                    return declared["mods"][0]["modId"]
            if "fabric.mod.json" in names:
                return json.loads(archive.read("fabric.mod.json"))["id"]
    except (OSError, zipfile.BadZipFile, ValueError, KeyError, IndexError, TypeError):
        return None   # unreadable metadata is not worth failing a launch over
    return None


def personal_mods(key: str, from_server: list[dict]) -> list[dict]:
    """Mods the player dropped into their own profile for this server.

    Anything the server already ships is left out. The folder is created empty on first launch,
    which is the whole "profile" - a directory the player can open and drop jars into.
    """
    folder = PROFILES / key / "mods"
    folder.mkdir(parents=True, exist_ok=True)
    jars = sorted(folder.glob("*.jar"))
    if not jars:
        return []

    served_hashes = {mod["sha1"] for mod in from_server}
    served_ids = {found for found in (mod_id(BLOBS / mod["sha1"]) for mod in from_server) if found}

    mine = []
    for jar in jars:
        entry = blob_of(jar)
        if entry["sha1"] in served_hashes:
            print(f"  {jar.name}: the pack already has this exact file")
        elif (declared := mod_id(jar)) and declared in served_ids:
            print(f"  {jar.name}: the pack already has {declared}")
        else:
            mine.append(entry)
    if mine:
        print(f"  plus {len(mine)} of your own")
    return mine


MODRINTH = "https://api.modrinth.com/v2"
CURSEFORGE = "https://api.curseforge.com/v1"
CF_KEY = os.environ.get("UNIFIEDMC_CF_KEY", "")

CF_GAME_MINECRAFT, CF_CLASS_MOD, CF_SORT_DOWNLOADS = 432, 6, 6
CF_REQUIRED_DEPENDENCY = 3
CF_SHA1 = 1
CF_LOADER = {"forge": 1, "fabric": 4, "quilt": 5, "neoforge": 6}


def _modrinth(path: str, params: dict | None = None, body: dict | None = None):
    url = f"{MODRINTH}{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, headers={
        "User-Agent": "UnifiedMC/0.1 (private modpack launcher)",
        **({"Content-Type": "application/json"} if data else {}),
    })
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def _cf(path: str, params: dict):
    request = urllib.request.Request(
        f"{CURSEFORGE}{path}?" + urllib.parse.urlencode(params),
        headers={"x-api-key": CF_KEY, "Accept": "application/json",
                 "User-Agent": "UnifiedMC/0.1 (private modpack launcher)"})
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)["data"]


def modrinth_side(slug: str) -> str:
    """Second opinion for a CurseForge project with no side tags.

    Most of them are published on Modrinth too, where the field is filled in - so a mod CurseForge
    says nothing about is usually still answerable.
    """
    if not slug:
        return "unknown"
    try:
        return side_verdict(_modrinth(f"/project/{slug}"))
    except (OSError, ValueError):
        return "unknown"


def cf_side(hit: dict, mf: dict) -> str:
    """Same question as side_verdict, answered from CurseForge's own tags.

    They publish it, just not where you would look: a file's gameVersions array carries "Client"
    and "Server" alongside the version and loader. Both present means the server has to carry it
    too. Most search hits already include a matching file, so this is usually free.
    """
    loader = (mf.get("loader") or {}).get("type", "").lower()
    tags = _cf_tags(hit.get("latestFiles", []), mf["minecraft"], loader)
    if not tags:
        try:
            files = _cf(f"/mods/{hit.get('id')}/files", {
                "gameVersion": mf["minecraft"],
                **({"modLoaderType": CF_LOADER[loader]} if loader in CF_LOADER else {}),
                "pageSize": 6,
            })
        except (OSError, ValueError):
            return "unknown"
        tags = _cf_tags(files, mf["minecraft"], loader)

    if "Server" in tags:
        return "no"
    if "Client" in tags:
        return "yes"
    return modrinth_side(hit.get("slug", ""))


def _cf_tags(files: list, mc_version: str, loader: str) -> set:
    tags = set()
    for file in files:
        versions = file.get("gameVersions", [])
        if mc_version in versions and (not loader or any(v.lower() == loader for v in versions)):
            tags |= {v for v in versions if v in ("Client", "Server")}
    return tags


def browse_curseforge(mf: dict, query: str, limit: int) -> list[dict]:
    """The other half of the catalogue. Plenty of mods live only here."""
    if not CF_KEY:
        return []
    loader = (mf.get("loader") or {}).get("type", "")
    try:
        hits = _cf("/mods/search", {
            "gameId": CF_GAME_MINECRAFT, "classId": CF_CLASS_MOD,
            "gameVersion": mf["minecraft"], "searchFilter": query,
            "sortField": CF_SORT_DOWNLOADS, "sortOrder": "desc", "pageSize": limit,
            **({"modLoaderType": CF_LOADER[loader]} if loader in CF_LOADER else {}),
        })
    except (OSError, ValueError) as e:
        print(f"  curseforge unavailable: {e}", file=sys.stderr)
        return []
    # the tag lookup can need a request per hit, so run them together rather than one by one
    already = served_curseforge(mf)
    with ThreadPoolExecutor(max_workers=8) as pool:
        verdicts = list(pool.map(
            lambda h: "yes" if f"cf:{h['id']}" in already else cf_side(h, mf), hits))

    return [{"id": f"cf:{hit['id']}", "title": hit["name"], "downloads": hit["downloadCount"],
             "description": hit["summary"], "source": "curseforge",
             "verified": verdict == "yes", "on_server": False,
             "icon_url": (hit.get("logo") or {}).get("thumbnailUrl", "")}
            for hit, verdict in zip(hits, verdicts) if verdict != "no"]


def pick_version_curseforge(project: str, mf: dict) -> dict | None:
    loader = (mf.get("loader") or {}).get("type", "")
    files = _cf(f"/mods/{project}/files", {
        "gameVersion": mf["minecraft"],
        **({"modLoaderType": CF_LOADER[loader]} if loader in CF_LOADER else {}),
    })
    # releaseType 1 is release; betas only if there is nothing stable for this version
    files = sorted(files, key=lambda f: f.get("releaseType", 9))
    for file in files:
        sha1 = next((h["value"] for h in file.get("hashes", []) if h["algo"] == CF_SHA1), None)
        if not sha1:
            continue
        if not file.get("downloadUrl"):
            # the author opted out of third-party downloads; nothing we can do but say so
            print(f"  {file['fileName']}: curseforge forbids automatic download", file=sys.stderr)
            continue
        return {"name": file["fileName"], "sha1": sha1, "url": file["downloadUrl"],
                "deps": [f"cf:{d['modId']}" for d in file.get("dependencies", [])
                         if d.get("relationType") == CF_REQUIRED_DEPENDENCY]}
    return None


def murmur2(data: bytes, seed: int = 1) -> int:
    """32-bit MurmurHash2, the variant CurseForge fingerprints files with."""
    m, r = 0x5BD1E995, 24
    length = len(data)
    h = (seed ^ length) & 0xFFFFFFFF
    i = 0
    while length >= 4:
        k = int.from_bytes(data[i:i + 4], "little")
        k = (k * m) & 0xFFFFFFFF
        k ^= k >> r
        k = (k * m) & 0xFFFFFFFF
        h = (h * m) & 0xFFFFFFFF
        h ^= k
        i += 4
        length -= 4
    if length == 3:
        h ^= data[i + 2] << 16
    if length >= 2:
        h ^= data[i + 1] << 8
    if length >= 1:
        h ^= data[i]
        h = (h * m) & 0xFFFFFFFF
    h ^= h >> 13
    h = (h * m) & 0xFFFFFFFF
    h ^= h >> 15
    return h


def cf_fingerprint(path: Path) -> int:
    """CurseForge hashes the file with tabs, newlines, carriage returns and spaces removed.
    translate() does the stripping in C - a per-byte loop here costs a minute over a full pack."""
    return murmur2(path.read_bytes().translate(None, b"\t\n\r "))


def served_curseforge(mf: dict) -> set[str]:
    """Which CurseForge projects the server already ships.

    CurseForge has no hash lookup, only its own fingerprint, so we compute it for every jar the
    server sends. Without this the catalogue offers the player mods they already have - and for
    the ones missing from Modrinth it is the only way to tell.
    """
    if not CF_KEY or not mf["mods"]:
        return set()

    # fingerprinting a whole pack means reading every jar, so remember the answer
    key = hashlib.sha1("".join(sorted(m["sha1"] for m in mf["mods"])).encode()).hexdigest()
    cache = DATA / "cache" / f"cf-{key}.json"
    if cache.is_file():
        try:
            return set(json.loads(cache.read_text()))
        except (OSError, ValueError):
            pass

    prints = {}
    missing = 0
    for mod in mf["mods"]:
        blob = BLOBS / mod["sha1"]
        if blob.is_file():
            prints[cf_fingerprint(blob)] = mod["sha1"]
        else:
            missing += 1   # not downloaded yet: this answer is partial, see below
    if not prints:
        return set()
    try:
        request = urllib.request.Request(
            f"{CURSEFORGE}/fingerprints",
            data=json.dumps({"fingerprints": list(prints)}).encode(),
            headers={"x-api-key": CF_KEY, "Content-Type": "application/json",
                     "Accept": "application/json", "User-Agent": "UnifiedMC/0.1"})
        with urllib.request.urlopen(request, timeout=30) as response:
            matched = json.load(response)["data"]["exactMatches"]
    except (OSError, ValueError, KeyError) as e:
        print(f"  curseforge fingerprint lookup failed: {e}", file=sys.stderr)
        return set()

    projects = {f"cf:{match['file']['modId']}" for match in matched}
    if missing:
        # The key covers the manifest, but the answer depends on which jars are on this disk -
        # two different things. Caching a partial result would make it permanent, and the browser
        # would keep offering mods the pack already contains.
        print(f"  {missing} mods not downloaded yet, fingerprints incomplete", file=sys.stderr)
        return projects
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(sorted(projects)))
    return projects


def served_projects(mf: dict) -> set[str]:
    """Which Modrinth projects the server already ships, resolved from the file hashes.

    One batch lookup instead of guessing from names: a jar's hash identifies it exactly, and
    knowing the project id lets the browser hide what the player would only install twice.
    Jars that are not on Modrinth simply do not match; install still checks the mod id.
    """
    hashes = [mod["sha1"] for mod in mf["mods"]]
    if not hashes:
        return set()
    try:
        found = _modrinth("/version_files", body={"hashes": hashes, "algorithm": "sha1"})
    except (OSError, ValueError):
        return set()
    return {version["project_id"] for version in found.values()}


def fetch_icon(url: str) -> str | None:
    """Cache one catalogue icon on disk and return its filename.

    PNG only: Minecraft's NativeImage reads nothing else, and a screen that has to decode webp is
    a screen that crashes on somebody's mod. No icon is a fine outcome.
    """
    if not url:
        return None
    name = hashlib.sha1(url.encode()).hexdigest() + ".png"
    target = ICONS / name
    if target.is_file():
        return name
    try:
        ICONS.mkdir(parents=True, exist_ok=True)
        request = urllib.request.Request(url, headers={"User-Agent": "UnifiedMC/0.1"})
        with urllib.request.urlopen(request, timeout=15) as response:
            data = response.read(4_000_000)
        return _store_png(data, target, name)
    except (OSError, ValueError):
        return None


def _store_png(data: bytes, target: Path, name: str) -> str | None:
    """Half the catalogue serves webp or jpeg, so converting is the difference between icons
    everywhere and icons on every other row."""
    tmp = target.with_suffix(".part")
    if data.startswith(b"\x89PNG"):
        tmp.write_bytes(data)
        tmp.replace(target)
        return name
    try:
        from PIL import Image
        with Image.open(io.BytesIO(data)) as image:
            image.convert("RGBA").save(tmp, "PNG")
        tmp.replace(target)
        return name
    except Exception:
        tmp.unlink(missing_ok=True)
        return None   # no icon is a fine outcome; a broken one would crash the screen


def with_icons(hits: list[dict]) -> list[dict]:
    """Fetch the artwork in parallel - one at a time would make the search feel broken."""
    with ThreadPoolExecutor(max_workers=8) as pool:
        for hit, name in zip(hits, pool.map(lambda h: fetch_icon(h.pop("icon_url", "")), hits)):
            if name:
                hit["icon"] = name
    return hits


def side_verdict(hit: dict) -> str:
    """Can one player add this to their own game and have it work?

    Only if the server does not have to carry it too. A backpack or a new block is `server_side:
    required` - putting it in a personal profile does nothing at best and desyncs at worst. Those
    belong on the server, which is whoever runs it's decision, not a browser's.

    Returns "yes", "no", or "unknown" - plenty of authors never filled the field in, and guessing
    either way would be wrong. Unknown ones are still offered, just not first and not unmarked.
    """
    client, server = hit.get("client_side"), hit.get("server_side")
    if client == "unsupported" or server == "required":
        return "no"
    if client in (None, "unknown") or server in (None, "unknown"):
        return "unknown"
    return "yes"


def browse(mf: dict, query: str = "", limit: int = 30) -> list[dict]:
    """Client mods that fit this server: right Minecraft version, right loader, not already shipped."""
    loader = (mf.get("loader") or {}).get("type", "")
    facets = [[f"versions:{mf['minecraft']}"], ["project_type:mod"]]
    if loader:
        facets.append([f"categories:{loader}"])

    hits = _modrinth("/search", {"query": query, "limit": limit, "index": "downloads",
                                 "facets": json.dumps(facets)})["hits"]
    already = served_projects(mf)
    found, unsure, shipped = [], [], []
    for hit in hits:
        on_server = hit["project_id"] in already
        verdict = side_verdict(hit)
        # a mod the server ships is shown whatever its side flags say: it demonstrably works here,
        # and hiding it just makes the player wonder why they cannot find it
        if not on_server and verdict == "no":
            continue
        entry = {"id": hit["slug"], "title": hit["title"], "downloads": hit["downloads"],
                 "description": hit["description"], "source": "modrinth",
                 "verified": verdict == "yes", "on_server": on_server,
                 "icon_url": hit.get("icon_url", "")}
        if on_server:
            shipped.append(entry)
        elif verdict == "yes":
            found.append(entry)
        else:
            unsure.append(entry)

    found.sort(key=lambda hit: -hit["downloads"])

    # Whatever could not be established - a Modrinth author who left the field empty, a CurseForge
    # project with no tagged file - goes after what could, so the larger download counts of the
    # unchecked ones do not push the checked results off the top.
    on_server = served_curseforge(mf)
    # shipped belongs in here too: the same mod exists on both sites, and matching it by hash on
    # one of them does not stop the other's copy from coming back as a fresh suggestion
    seen = {hit["title"].lower() for hit in found + unsure + shipped}
    for hit in browse_curseforge(mf, query, limit):
        if hit["title"].lower() in seen:
            continue
        if hit["id"] in on_server:
            hit["on_server"] = True
            shipped.append(hit)
        elif hit["verified"]:
            found.append(hit)
        else:
            unsure.append(hit)

    # Anything still unestablished stays out. Offering a mod that turns out to need the server is
    # worse than not offering it: it does nothing, and the player has no way to tell why.
    if unsure:
        print(f"  {len(unsure)} hidden: no side information anywhere", file=sys.stderr)

    # one relevance order, not two lists. Someone typing "sodium" wants to know whether they
    # already have it before they want a list of addons; the marker carries that, position would
    # only bury it.
    merged = sorted(found + shipped, key=lambda hit: -hit["downloads"])
    return with_icons(merged)


def pick_version(slug: str, mf: dict) -> dict | None:
    """Newest build of `slug` for this server's Minecraft and loader, release preferred.

    The list comes back newest-first regardless of type, so taking the first entry hands people
    a beta - and worse, hands it to them as somebody else's dependency, which they never chose.
    """
    loader = (mf.get("loader") or {}).get("type", "")
    versions = _modrinth(f"/project/{slug}/version", {
        "game_versions": json.dumps([mf["minecraft"]]),
        **({"loaders": json.dumps([loader])} if loader else {}),
    })
    if not versions:
        return None
    return next((v for v in versions if v.get("version_type") == "release"), versions[0])


def wanted_with_deps(slugs: list[str], mf: dict) -> list[dict]:
    """Resolve each mod plus everything it requires.

    Installing Iris without Sodium is a crash, not a preference, so required dependencies come
    along whether the player noticed them or not.
    """
    queue, seen, chosen = list(slugs), set(), []
    while queue:
        slug = queue.pop(0)
        if slug in seen:
            continue
        seen.add(slug)

        if slug.startswith("cf:"):
            picked = pick_version_curseforge(slug[3:], mf)
            deps = picked.pop("deps") if picked else []
        else:
            version = pick_version(slug, mf)
            picked = None
            deps = []
            if version:
                file = version["files"][0]
                picked = {"name": file["filename"], "sha1": file["hashes"]["sha1"], "url": file["url"]}
                deps = [dep["project_id"] for dep in version.get("dependencies", [])
                        if dep.get("dependency_type") == "required" and dep.get("project_id")]

        if picked is None:
            print(f"  {slug}: nothing built for {mf['minecraft']} / "
                  f"{(mf.get('loader') or {}).get('type', 'vanilla')}", file=sys.stderr)
            continue
        chosen.append(picked)
        queue.extend(deps)
    return chosen


def add_to_profile(key: str, mf: dict, slugs: list[str]) -> list[str]:
    """Download the chosen mods into this server's profile folder."""
    folder = PROFILES / key / "mods"
    folder.mkdir(parents=True, exist_ok=True)
    served = {mod["sha1"] for mod in mf["mods"]}
    # the catalogue can only hide what it can identify; this is what actually prevents a crash,
    # and it works no matter which site the jar came from
    served_ids = {found for found in (mod_id(BLOBS / mod["sha1"]) for mod in mf["mods"]) if found}

    installed = []
    for mod in wanted_with_deps(slugs, mf):
        if mod["sha1"] in served:
            print(f"  {mod['name']}: already in the pack")
            continue
        if not have(mod["sha1"]):
            download(mod)

        declared = mod_id(BLOBS / mod["sha1"])
        if declared and declared in served_ids:
            print(f"  {mod['name']}: {declared} is already in the pack")
            continue

        link(BLOBS / mod["sha1"], folder / mod["name"])
        installed.append(mod["name"])
        print(f"  + {mod['name']}")
    return installed


# --- launch -----------------------------------------------------------------

def system_ram_mb() -> int:
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) // 1024
    except (OSError, ValueError, IndexError):
        pass
    try:
        return os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES") // (1 << 20)
    except (ValueError, OSError, AttributeError):
        return 8192


def heap_mb(mf: dict) -> int:
    """How much memory to give this instance.

    The JVM default is a quarter of physical memory, which a two hundred mod pack runs out of.
    Automatic scales with the pack, because that is what actually drives the requirement, and is
    capped at half the machine: handing Minecraft more than the system has just moves the
    stalling from the garbage collector to the swap file.
    """
    # never hand out more than the machine can back, whoever asked for it: past physical memory
    # the stalling just moves from the garbage collector to the swap file
    room = max(2048, system_ram_mb() - 2048)

    chosen = read_json(UI, {}).get("ram", 0)
    if chosen:
        return min(int(chosen), room)

    ceiling = min(8192, max(2048, system_ram_mb() // 2))
    return int(min(ceiling, room, 2048 + 12 * len(mf.get("mods", []))))


def instance_key(host: str, mf: dict) -> str:
    loader = f"-{mf['loader']['type']}" if mf.get("loader") else ""
    return f"{host}-{mf['minecraft']}{loader}".replace(":", "_")


def progress(phase: str, detail: str = "", done: int = 0, total: int = 0) -> None:
    """Tell the waiting client what we are doing.

    The player is staring at a loading screen in another process; without this it has nothing to
    show but a spinner. Written then renamed, because that screen polls the file while we write.
    """
    try:
        PROGRESS.parent.mkdir(parents=True, exist_ok=True)
        tmp = PROGRESS.with_suffix(".tmp")
        tmp.write_text(json.dumps({"phase": phase, "detail": detail,
                                   "done": done, "total": total, "at": time.time()}))
        tmp.replace(PROGRESS)
    except OSError:
        pass   # a missing progress line must never stop a launch


def _progress():
    return {"setStatus": lambda t: print(f"  {t}"), "setProgress": lambda v: None, "setMax": lambda v: None}


def java_for(mc_version: str) -> str:
    """Mojang ships a JRE per Minecraft version. The player installs no Java, ever."""
    info = mll.runtime.get_version_runtime_information(mc_version, str(MC))
    if info is None:
        raise ValueError(f"no JVM runtime declared for {mc_version}")
    jvm = info["name"]
    exe = mll.runtime.get_executable_path(jvm, str(MC))
    if exe is None:
        print(f"  installing {jvm}")
        mll.runtime.install_jvm_runtime(jvm, str(MC), callback=_progress())
        exe = mll.runtime.get_executable_path(jvm, str(MC))
    if exe is None:
        raise ValueError(f"{jvm} installed but no executable found")
    return exe


def ensure_version(mf: dict) -> tuple[str, str]:
    """Install the MC version the server asked for, its JRE and its loader.

    Returns (version id, java executable).
    """
    MC.mkdir(parents=True, exist_ok=True)
    cb = _progress()
    progress("Minecraft " + mf["minecraft"], "wird installiert")
    mll.install.install_minecraft_version(mf["minecraft"], str(MC), callback=cb)
    progress("Java", "Laufzeitumgebung wird geprueft")
    java = java_for(mf["minecraft"])

    loader = mf.get("loader")
    if not loader:
        return mf["minecraft"], java

    kind = loader["type"]
    progress(kind.capitalize() + " " + (loader.get("version") or ""), "wird installiert")
    if kind == "fabric":
        mll.fabric.install_fabric(mf["minecraft"], str(MC), loader_version=loader.get("version"),
                                  callback=cb, java=java)
        return next(v["id"] for v in mll.utils.get_installed_versions(str(MC))
                    if v["id"].startswith("fabric-") and v["id"].endswith(mf["minecraft"])), java

    if kind == "neoforge":
        version = loader.get("version")
        if not version:
            raise ValueError("neoforge needs an explicit loader version in the manifest")
        version_id = f"neoforge-{version}"
        if not any(v["id"] == version_id for v in mll.utils.get_installed_versions(str(MC))):
            mll.mod_loader.Neoforge().install(mf["minecraft"], str(MC), cb, java, version)
        return version_id, java

    # ponytail: fabric and neoforge. mll.forge and mll.quilt are right there when a server needs one.
    raise NotImplementedError(f"loader {kind!r}")


SIGNED_IN_PAGE = b"""<!doctype html><meta charset=utf-8>
<body style="font:16px system-ui;display:grid;place-items:center;height:90vh">
<div><h2>Angemeldet.</h2><p>Du kannst dieses Fenster schliessen.</p></div>"""


def catch_auth_code(login_url: str) -> str:
    """Serve the redirect Microsoft sends the browser back to, and read the code out of it.

    A loopback listener is the standard desktop pattern: no redirect service to host, no client
    secret to ship, and the code never leaves the machine.
    """
    landed = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            self.wfile.write(SIGNED_IN_PAGE)
            if "code=" in self.path:
                landed.append(REDIRECT_URI + self.path)

        def log_message(self, *args):
            pass

    with http.server.HTTPServer(("127.0.0.1", REDIRECT_PORT), Handler) as server:
        server.timeout = 300
        print(f"-> sign in with Microsoft; if no browser opened:\n   {login_url}")
        webbrowser.open(login_url)
        while not landed:
            server.handle_request()   # browsers also ask for /favicon.ico - keep waiting
    return landed[0]


def save_account(login: dict) -> None:
    """The refresh token is a credential. Owner-readable only, and renamed into place so a crash
    cannot leave half a token behind."""
    ACCOUNT.parent.mkdir(parents=True, exist_ok=True)
    tmp = ACCOUNT.with_suffix(".tmp")
    tmp.touch(mode=0o600)
    tmp.write_text(json.dumps({"refresh_token": login["refresh_token"]}))
    os.chmod(tmp, 0o600)
    tmp.replace(ACCOUNT)


def sign_in() -> dict:
    """Refresh the saved session, or run the browser flow once and save it."""
    if ACCOUNT.is_file():
        try:
            token = json.loads(ACCOUNT.read_text())["refresh_token"]
            login = mll.microsoft_account.complete_refresh(CLIENT_ID, None, None, token)
            save_account(login)
            return login
        except (ValueError, KeyError, mll.microsoft_account.InvalidRefreshToken):
            print("  saved session expired, signing in again")

    login_url, state, verifier = mll.microsoft_account.get_secure_login_data(CLIENT_ID, REDIRECT_URI)
    code = mll.microsoft_account.parse_auth_code_url(catch_auth_code(login_url), state)
    login = mll.microsoft_account.complete_login(CLIENT_ID, None, REDIRECT_URI, code, verifier)
    save_account(login)
    return login


def token_expiry(token: str) -> int | None:
    """Minecraft access tokens are JWTs. Read `exp` without verifying - we are not the one
    checking the signature, we just want to say "expired" instead of letting a server refuse the
    player for no visible reason."""
    try:
        payload = token.split(".")[1]
        payload += "=" * (-len(payload) % 4)
        return int(json.loads(base64.urlsafe_b64decode(payload))["exp"])
    except (IndexError, ValueError, KeyError, TypeError):
        return None


def borrowed_session() -> dict | None:
    """A session the hub picked up from whatever launcher started it.

    Lets the whole thing work before - or entirely without - an Azure app of our own.
    """
    if not SESSION.is_file():
        return None
    try:
        saved = json.loads(SESSION.read_text())
        name, uuid, token = saved["name"], saved["uuid"], saved["token"]
    except (OSError, ValueError, KeyError):
        return None

    expires = token_expiry(token)
    if expires is not None and expires <= time.time():
        print("  the borrowed session has expired - start the hub from your launcher again",
              file=sys.stderr)
        return None
    return {"username": name, "uuid": uuid, "token": token}


_session: dict | None = None


def account() -> dict:
    """The profile every instance launches with. Resolved once per session."""
    global _session
    if _session is not None:
        return _session

    borrowed = borrowed_session()
    if borrowed:
        print(f"  playing as {borrowed['username']} (session from the launcher)")
        _session = borrowed
        return _session

    if not CLIENT_ID:
        name = os.environ.get("UNIFIEDMC_USER", "Player")
        print("  no UNIFIEDMC_CLIENT_ID set - offline profile, online-mode servers will refuse it")
        _session = {"username": name, "uuid": hashlib.md5(name.encode()).hexdigest(), "token": "0"}
        return _session

    try:
        login = sign_in()
    except mll.microsoft_account.AzureAppNotPermitted:
        raise SystemExit("Azure app is not cleared for the Minecraft API - see README, 'Login'")
    except mll.microsoft_account.AccountNotOwnMinecraft:
        raise SystemExit("that Microsoft account does not own Minecraft Java Edition")

    print(f"  signed in as {login['name']}")
    _session = {"username": login["name"], "uuid": login["id"], "token": login["access_token"]}
    return _session


def provision(mf: dict, mods: list[dict], key: str) -> tuple[str, Path, str]:
    """Make an instance on disk that matches the manifest. Returns (version id, game dir, java)."""
    inst = INSTANCES / key
    inst.mkdir(parents=True, exist_ok=True)

    report = sync_mods(mods, inst / "mods")
    print(f"  mods: {report['cached']} cached, {report['downloaded']} downloaded")

    config = mf.get("config", [])
    if config:
        progress("Konfiguration", f"{len(config)} Dateien")
        sync_config(config, inst)

    version_id, java = ensure_version(mf)
    return version_id, inst, java


def split_addr(entry: str) -> tuple[str, int]:
    """"host", "host:port" or a bare IPv4. Vanilla defaults to 25565.
    ponytail: no bracketed IPv6. Add it the day a server list actually contains one."""
    host, sep, port = entry.rpartition(":")
    return (host, int(port)) if sep and port.isdigit() else (entry, 25565)


def spawn(mf: dict, mods: list[dict], addr: str | None, key: str) -> tuple[subprocess.Popen, Path]:
    version_id, inst, java = provision(mf, mods, key)
    # tells the mod a shell is watching the handoff files; without it the mod stays out of the way
    memory = heap_mb(mf)
    jvm = ["-Dunifiedmc.managed=true", f"-Xmx{memory}M", f"-Xms{min(memory, 1024)}M"]
    opts = {**account(), "gameDirectory": str(inst), "executablePath": java, "jvmArguments": jvm}
    if addr:
        jvm.append(f"-Dunifiedmc.ready={addr}")
        opts["quickPlayMultiplayer"] = addr
    cmd = mll.command.get_minecraft_command(version_id, str(MC), opts)
    log = inst / "launch.log"
    progress("Minecraft startet", f"{version_id}, {memory} MB")
    print(f"-> launch {version_id} with {memory} MB")
    return subprocess.Popen(cmd, cwd=inst, stdout=log.open("wb"), stderr=subprocess.STDOUT), log


def wait_visible(proc: subprocess.Popen, log: Path, timeout: int = LAUNCH_TIMEOUT) -> bool:
    """Block until the new window is on screen and rendering.

    This is the whole trick behind a seamless swap: whatever is being replaced stays up until
    this returns, so there is never a frame showing the bare desktop.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            return False
        if log.is_file() and VISIBLE.search(log.read_bytes()):
            return True
        time.sleep(0.25)
    print("  window never reported ready; retiring the old one anyway", file=sys.stderr)
    return False


def retire(proc: subprocess.Popen | None) -> None:
    """Close the window that has been replaced. By now the new one is already covering it."""
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def resolve(host: str, port: int, quiet: bool = False) -> dict:
    if not quiet:
        progress("Server wird abgefragt", f"{host}:{port}")
    mf = fetch_manifest(host, port, ping(host, port))
    if not quiet:
        print(f"-> {host}:{port} is {mf['minecraft']}, {len(mf['mods'])} server mods")
    return mf


def serves(mf: dict) -> bool:
    """Can the hub join this server exactly as it stands? Then nothing has to restart at all."""
    return mf["minecraft"] == HUB_VERSION and not mf["mods"]


def read_json(path: Path, fallback):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return fallback


def write_json(path: Path, value) -> None:
    """Written then renamed - the client polls these while we write them."""
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(".tmp")
        tmp.write_text(json.dumps(value))
        tmp.replace(path)
    except OSError:
        pass


def probe(host: str, port: int) -> dict:
    """Everything the hub wants to show for one server, in one place."""
    try:
        status = ping(host, port)
    except (OSError, ValueError) as e:
        return {"online": False, "error": type(e).__name__}

    players = status.get("players", {})
    try:
        mf = fetch_manifest(host, port, status)
    except (OSError, ValueError):
        return {"online": True, "version": status.get("version", {}).get("name", ""),
                "online_players": players.get("online", 0), "max_players": players.get("max", 0),
                "error": "manifest"}

    return {
        "online": True,
        "minecraft": mf["minecraft"],
        "loader": (mf.get("loader") or {}).get("type", ""),
        "mods": len(mf["mods"]),
        "config": len(mf.get("config", [])),
        "online_players": players.get("online", 0),
        "max_players": players.get("max", 0),
        "motd": _plain_motd(status.get("description")),
        "ready": serves(mf),
    }


def _plain_motd(description) -> str:
    """Server descriptions are a chat component tree; the hub only wants the words."""
    if isinstance(description, str):
        return description
    if isinstance(description, dict):
        text = description.get("text", "")
        for part in description.get("extra", []):
            text += _plain_motd(part)
        return text
    if isinstance(description, list):
        return "".join(_plain_motd(part) for part in description)
    return ""


def installed_mods(key: str) -> list[dict]:
    """What the player has added themselves for this server."""
    folder = PROFILES / key / "mods"
    if not folder.is_dir():
        return []
    return [{"id": jar.name, "title": jar.name, "downloads": 0,
             "description": mod_id(jar) or "unbekannte Mod-ID",
             "source": "profil", "verified": True, "on_server": False, "removable": True}
            for jar in sorted(folder.glob("*.jar"))]


def pack_mods(mf: dict) -> list[dict]:
    """Everything the server sends, exactly as it sends it."""
    return [{"id": mod["sha1"], "title": mod["name"], "downloads": 0, "description": "",
             "source": "pack", "verified": True, "on_server": True, "removable": False}
            for mod in sorted(mf["mods"], key=lambda m: m["name"].lower())]


def remove_from_profile(key: str, names: list[str]) -> list[str]:
    """Delete the player's own jars again. Only from their own folder, only by plain filename."""
    folder = PROFILES / key / "mods"
    gone = []
    for name in names:
        target = folder / Path(name).name
        if target.is_file():
            target.unlink()
            gone.append(target.name)
    return gone


def answer_query(request: dict) -> None:
    """One list, on behalf of the screen the player has open."""
    host, port = split_addr(request.get("server", ""))
    mode = request.get("mode", "search")
    try:
        mf = resolve(host, port, quiet=True)
        key = instance_key(f"{host}:{port}", mf)
        if mode == "installed":
            hits = installed_mods(key)
        elif mode == "pack":
            hits = pack_mods(mf)
        else:
            hits = browse(mf, request.get("query", ""))
        write_json(CATALOG, {"id": request.get("id"), "hits": hits})
    except (OSError, ValueError) as e:
        write_json(CATALOG, {"id": request.get("id"), "hits": [], "error": str(e)})


def install_request(request: dict) -> None:
    """Install or remove what the player ticked, then report back through the same file."""
    host, port = split_addr(request.get("server", ""))
    try:
        mf = resolve(host, port, quiet=True)
        key = instance_key(f"{host}:{port}", mf)
        if request.get("remove"):
            changed = remove_from_profile(key, request["remove"])
            write_json(CATALOG, {"id": request.get("id"), "removed": changed})
        else:
            changed = add_to_profile(key, mf, request.get("install", []))
            write_json(CATALOG, {"id": request.get("id"), "installed": changed})
    except (OSError, ValueError) as e:
        write_json(CATALOG, {"id": request.get("id"), "installed": [], "error": str(e)})


def scout(proc: subprocess.Popen) -> None:
    """Keep the hub fed while the player is looking at it.

    Two jobs, one thread: probe every server in the list so the screen can show what each one
    actually is, and answer the catalogue searches that screen asks for. Both only make sense
    while that window is open, so both end when it does.

    ponytail: nothing is downloaded on speculation. Pulling a version for a server nobody
    clicked is not a favour.
    """
    for stale in (DIRECT, STATUS, CATALOG, QUERY):
        stale.unlink(missing_ok=True)

    known: dict[str, dict] = {}
    answered = None

    while proc.poll() is None:
        for entry in read_json(SERVERS, []):
            host, port = split_addr(str(entry))
            addr = f"{host}:{port}"
            if addr in known:
                continue
            known[addr] = probe(host, port)
            write_json(STATUS, known)
            write_json(DIRECT, [a for a, s in known.items() if s.get("ready")])

        request = read_json(QUERY, None)
        if isinstance(request, dict) and request.get("id") != answered:
            answered = request.get("id")
            if request.get("install") or request.get("remove"):
                install_request(request)
            else:
                answer_query(request)

        time.sleep(0.4)


def take_handoff() -> tuple[str, int] | None:
    """Read and consume the request the client left behind."""
    if not HANDOFF.is_file():
        return None
    try:
        req = json.loads(HANDOFF.read_text())
        return req["host"], int(req["port"])
    except (ValueError, KeyError) as e:
        print(f"  ignoring unreadable handoff: {e}", file=sys.stderr)
        return None
    finally:
        HANDOFF.unlink(missing_ok=True)


def await_handoff(proc: subprocess.Popen) -> tuple[str, int] | None:
    """Wait for the running client to ask for a different instance, or to exit."""
    while proc.poll() is None:
        request = take_handoff()
        if request:
            return request
        time.sleep(0.25)
    return take_handoff()


def run(first: tuple[str, int] | None = None) -> None:
    """Hub -> server -> hub, for as long as the player keeps playing.

    Only the hub carries the UnifiedMC mod; a target instance is exactly what the server asked
    for and nothing else, so it works on any Minecraft version. Instances overlap during a swap,
    so the player only ever sees a loading screen.
    """
    for stale in (HANDOFF, DIRECT, SERVERS):
        stale.unlink(missing_ok=True)

    target = first
    outgoing = None   # still on screen while its replacement boots

    while True:
        if target:
            host, port = target
            try:
                mf = resolve(host, port)
            except (OSError, ValueError) as e:
                print(f"  cannot reach {host}:{port}: {e}", file=sys.stderr)
                target = None
                continue
            addr = f"{host}:{port}"
            key = instance_key(addr, mf)
            mods = mf["mods"] + personal_mods(key, mf["mods"])
        else:
            print("-> hub")
            mf = {"minecraft": HUB_VERSION, "loader": {"type": "fabric", "version": None}, "mods": []}
            addr, key = None, instance_key("hub", mf)
            mods = hub_mods(HUB_VERSION)

        proc, log = spawn(mf, mods, addr, key)
        wait_visible(proc, log)
        retire(outgoing)
        outgoing = None

        if addr is None:
            threading.Thread(target=scout, args=(proc,), daemon=True).start()

        handoff = await_handoff(proc)
        if handoff:
            outgoing = proc   # hold the window until the next one is up
            target = handoff
        else:
            proc.wait()
            if target is None:
                return        # quitting out of the hub ends the session
            target = None     # left a server, back to the menu


# --- self-check -------------------------------------------------------------

def demo() -> None:
    """No network. Fails loudly if any of the load-bearing pure logic breaks."""
    global BLOBS, HANDOFF, SESSION, ACCOUNT, PROFILES
    # murmur2 is a hash reimplementation: a wrong constant still returns plausible numbers, so
    # pin it to values CurseForge itself agrees with (verified against xaerominimap -> cf:263420)
    # values taken from this implementation after CurseForge confirmed it live
    # (xaerominimap-neoforge-1.21.1-26.4.2.jar fingerprints to their project 263420)
    assert murmur2(b"") == 1540447798, murmur2(b"")
    assert murmur2(b"hello") == 2788266382, murmur2(b"hello")
    assert murmur2(b"abcd") == 3376380438, murmur2(b"abcd")
    assert murmur2(b"abc") == 1621425345, murmur2(b"abc")
    assert murmur2(b"a b\tc\n".translate(None, b"\t\n\r ")) == murmur2(b"abc")

    # SRV picking: lowest priority first, then highest weight. Getting this wrong is silent.
    rec = lambda p, w, t, port: type("R", (), {"priority": p, "weight": w, "target": t, "port": port})()
    assert srv_pick([rec(10, 5, "a.example.com.", 25570),
                     rec(0, 1, "b.example.com.", 25580),
                     rec(0, 9, "c.example.com.", 25590)]) == ("c.example.com", 25590)

    for entry, want in [("mc.example.com", ("mc.example.com", 25565)),
                        ("mc.example.com:25577", ("mc.example.com", 25577)),
                        ("1.2.3.4", ("1.2.3.4", 25565))]:
        assert split_addr(entry) == want, entry

    assert serves({"minecraft": HUB_VERSION, "mods": []})
    assert not serves({"minecraft": HUB_VERSION, "mods": [{"name": "x"}]})
    assert not serves({"minecraft": "26.2", "mods": []})

    for n in (0, 1, 127, 128, 255, 2097151, 25565):
        assert _read_varint(io.BytesIO(_varint(n)).read) == n, n

    p = _packet(0x00, b"hi")
    r = io.BytesIO(p).read
    assert _read_varint(r) == 3 and _read_varint(r) == 0x00 and r(2) == b"hi"

    # vanilla server: no manifest, version resolved from the protocol number
    t = {772: "1.21.8", 47: "1.8.9"}
    v = normalize({}, {"version": {"name": "whatever", "protocol": 772}}, t)
    assert v == {"minecraft": "1.21.8", "loader": None, "mods": [], "config": []}, v
    # free-text names must be ignored entirely, even when they look parseable
    assert normalize({}, {"version": {"name": "We support: 1.20-1.21", "protocol": 47}}, t)["minecraft"] == "1.8.9"
    # every test below hands normalize() an explicit table, so nothing here would notice if the
    # default lookup went missing - an edit already deleted it once
    assert callable(protocol_table)

    # a server publishes paths, not urls; they resolve against wherever we reached it
    served = normalize({"minecraft": "1.21.1", "mods": [{"name": "a.jar", "sha1": "x", "url": "/mods/x"},
                                                        {"name": "b.jar", "sha1": "y"}]},
                       {}, t, base="http://mc.example.com:25566/")
    assert served["mods"][0]["url"] == "http://mc.example.com:25566/mods/x", served["mods"][0]
    assert "url" not in served["mods"][1], "an entry without a url must stay without one"

    # an explicit manifest always wins over the ping
    assert normalize({"minecraft": "1.21.11"}, {"version": {"protocol": 47}}, t)["minecraft"] == "1.21.11"
    try:
        normalize({}, {"version": {"protocol": 999999}}, t)
        raise AssertionError("unknown protocol must raise")
    except ValueError:
        pass

    mf = {"minecraft": "1.21.11", "loader": {"type": "fabric", "version": "0.19.3"}}
    assert instance_key("mc.example.com", mf) == "mc.example.com-1.21.11-fabric"

    # sync is idempotent and prunes what the server dropped
    import tempfile
    keep = BLOBS
    try:
        with tempfile.TemporaryDirectory() as td:
            td = Path(td)
            BLOBS = td / "blobs"
            BLOBS.mkdir()
            a, b = "a" * 40, "b" * 40
            (BLOBS / a).write_bytes(b"mod-a")
            (BLOBS / b).write_bytes(b"mod-b")
            mods_dir = td / "mods"

            r = sync_mods([{"name": "a.jar", "sha1": a}, {"name": "b.jar", "sha1": b}], mods_dir)
            assert r == {"total": 2, "downloaded": 0, "cached": 2}, r
            assert sorted(p.name for p in mods_dir.iterdir()) == ["a.jar", "b.jar"]

            sync_mods([{"name": "a.jar", "sha1": a}], mods_dir)          # b removed server-side
            assert [p.name for p in mods_dir.iterdir()] == ["a.jar"]
            assert (mods_dir / "a.jar").read_bytes() == b"mod-a"

            sync_mods([{"name": "a.jar", "sha1": a}], mods_dir)          # idempotent
            assert [p.name for p in mods_dir.iterdir()] == ["a.jar"]
    finally:
        BLOBS = keep

    # handoff is read once and consumed, so a crash never replays an old request
    keep_h = HANDOFF
    try:
        with tempfile.TemporaryDirectory() as td:
            HANDOFF = Path(td) / "handoff.json"
            assert take_handoff() is None
            HANDOFF.write_text(json.dumps({"host": "mc.example.com", "port": 25566}))
            assert take_handoff() == ("mc.example.com", 25566)
            assert not HANDOFF.exists(), "handoff must be consumed"
            assert take_handoff() is None
            HANDOFF.write_text("{not json")
            assert take_handoff() is None
            assert not HANDOFF.exists(), "unreadable handoff must be consumed too"
    finally:
        HANDOFF = keep_h

    # a session is only usable while it lasts; an expired one must not be handed to the game
    keep_s = SESSION
    try:
        with tempfile.TemporaryDirectory() as td:
            SESSION = Path(td) / "session.json"
            assert borrowed_session() is None, "missing file must not yield a session"

            def jwt(exp):
                body = base64.urlsafe_b64encode(json.dumps({"exp": exp}).encode()).decode().rstrip("=")
                return f"header.{body}.sig"

            live = {"name": "Steve", "uuid": "u-1", "token": jwt(int(time.time()) + 3600)}
            SESSION.write_text(json.dumps(live))
            assert borrowed_session() == {"username": "Steve", "uuid": "u-1", "token": live["token"]}

            SESSION.write_text(json.dumps({**live, "token": jwt(int(time.time()) - 60)}))
            assert borrowed_session() is None, "expired session must be refused"

            SESSION.write_text(json.dumps({"name": "Steve"}))
            assert borrowed_session() is None, "incomplete session must be refused"

            SESSION.write_text(json.dumps({**live, "token": "not-a-jwt"}))
            assert borrowed_session() is not None, "unreadable expiry must not block a session"
    finally:
        SESSION = keep_s

    # dedupe by declared mod id: the same mod in two versions shares neither name nor hash
    keep_b, keep_p = BLOBS, PROFILES
    try:
        with tempfile.TemporaryDirectory() as td:
            td = Path(td)
            BLOBS, PROFILES = td / "blobs", td / "profiles"
            BLOBS.mkdir()

            def jar(path: Path, entry: str | None = None, body: bytes = b"", pad: bytes = b"") -> Path:
                path.parent.mkdir(parents=True, exist_ok=True)
                with zipfile.ZipFile(path, "w") as z:
                    if entry:
                        z.writestr(entry, body)
                    z.writestr("pad.txt", pad)
                return path

            neo = b'[[mods]]\nmodId="jei"\nversion="1"\n'
            served = jar(td / "served-jei-1.0.jar", "META-INF/neoforge.mods.toml", neo)
            assert mod_id(served) == "jei"
            assert mod_id(jar(td / "f.jar", "fabric.mod.json", b'{"id":"sodium"}')) == "sodium"
            assert mod_id(jar(td / "plain.jar")) is None, "a jar with no metadata is not an id"

            from_server = [blob_of(served)]
            mine = PROFILES / "srv" / "mods"
            # same mod, newer build: different name, different bytes, same id -> must be dropped
            jar(mine / "jei-2.0.jar", "META-INF/neoforge.mods.toml", neo, pad=b"different")
            jar(mine / "sodium.jar", "fabric.mod.json", b'{"id":"sodium"}')
            shutil.copy2(served, mine / "exact-copy.jar")          # identical file -> dropped

            kept = personal_mods("srv", from_server)
            assert [m["name"] for m in kept] == ["sodium.jar"], kept
    finally:
        BLOBS, PROFILES = keep_b, keep_p

    # a mod the server must also carry is not something one player can just add
    assert side_verdict({"client_side": "required", "server_side": "unsupported"}) == "yes"
    assert side_verdict({"client_side": "optional", "server_side": "optional"}) == "yes"
    assert side_verdict({"client_side": "required", "server_side": "required"}) == "no"
    assert side_verdict({"client_side": "unsupported", "server_side": "optional"}) == "no"
    # a curseforge project with no tags falls back to modrinth rather than being shown unchecked
    assert cf_side({"id": 0, "latestFiles": [], "slug": ""},
                   {"minecraft": "1.21.1", "loader": None}) == "unknown"

    # curseforge says it in a file's gameVersions array, next to the version and loader
    mc = {"minecraft": "1.21.1", "loader": {"type": "neoforge"}}
    both = {"latestFiles": [{"gameVersions": ["Client", "1.21.1", "NeoForge", "Server"]}]}
    client = {"latestFiles": [{"gameVersions": ["Client", "1.21.1", "NeoForge"]}]}
    other = {"latestFiles": [{"gameVersions": ["Client", "1.20.1", "NeoForge"]}]}
    assert cf_side(both, mc) == "no"
    assert cf_side(client, mc) == "yes"
    assert _cf_tags(other["latestFiles"], "1.21.1", "neoforge") == set(), "wrong version must not count"

    # authors who filled nothing in must not be guessed either way
    assert side_verdict({"client_side": "unknown", "server_side": "unknown"}) == "unknown"
    assert side_verdict({}) == "unknown"

    # only real png reaches the game; anything undecodable comes back as "no icon"
    assert fetch_icon("") is None
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / "x.png"
        assert _store_png(b"not an image", out, "x.png") is None
        assert not out.exists() and not out.with_suffix(".part").exists()
        assert _store_png(b"\x89PNG\r\n\x1a\n rest", out, "x.png") == "x.png"
        assert out.read_bytes().startswith(b"\x89PNG")

    # removal only ever touches the player's own folder, and only by bare filename
    keep_profiles = PROFILES
    try:
        with tempfile.TemporaryDirectory() as td:
            PROFILES = Path(td) / "profiles"
            mine = PROFILES / "srv" / "mods"
            mine.mkdir(parents=True)
            (mine / "keep.jar").write_text("x")
            (mine / "drop.jar").write_text("x")
            outside = Path(td) / "secret.txt"
            outside.write_text("x")

            assert remove_from_profile("srv", ["drop.jar"]) == ["drop.jar"]
            assert not (mine / "drop.jar").exists() and (mine / "keep.jar").exists()
            assert remove_from_profile("srv", ["../../secret.txt"]) == []
            assert outside.exists(), "a path must never reach outside the profile"
            assert remove_from_profile("srv", ["nothere.jar"]) == []
    finally:
        PROFILES = keep_profiles

    # memory scales with the pack and never exceeds what the machine can back
    global UI
    keep_ui = UI
    try:
        with tempfile.TemporaryDirectory() as td:
            UI = Path(td) / "ui.json"
            small = heap_mb({"mods": []})
            big = heap_mb({"mods": [{}] * 214})
            assert small == 2048, small
            assert big > small, (small, big)
            assert big <= min(8192, max(2048, system_ram_mb() // 2)), big

            UI.write_text(json.dumps({"ram": 6144}))
            assert heap_mb({"mods": [{}] * 214}) == min(6144, max(2048, system_ram_mb() - 2048))
            UI.write_text(json.dumps({"ram": 999999}))
            assert heap_mb({"mods": []}) <= max(2048, system_ram_mb() - 2048), "must not exceed the machine"
            UI.write_text(json.dumps({"ram": 0}))
            assert heap_mb({"mods": []}) == 2048, "0 means automatic"
    finally:
        UI = keep_ui

    # motd arrives as a chat component tree, not a string
    assert _plain_motd("A Server") == "A Server"
    assert _plain_motd({"text": "Hello ", "extra": [{"text": "World"}]}) == "Hello World"
    assert _plain_motd([{"text": "a"}, {"text": "b"}]) == "ab"
    assert _plain_motd(None) == ""

    # a config the player edited must survive a relaunch; one they never touched must update
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        inst = td / "inst"
        (inst / "config").mkdir(parents=True)
        keep_blobs = BLOBS
        BLOBS = td / "blobs"
        BLOBS.mkdir()
        try:
            def entry(text: str, path: str = "create.toml") -> dict:
                digest = hashlib.sha1(text.encode()).hexdigest()
                (BLOBS / digest).write_text(text)
                return {"path": path, "sha1": digest}

            first = entry("v1")
            sync_config([first], inst)
            assert (inst / "config/create.toml").read_text() == "v1"

            # admin updates it and the player never touched theirs -> replaced
            sync_config([entry("v2")], inst)
            assert (inst / "config/create.toml").read_text() == "v2"

            # player edits theirs, server has not moved -> left alone
            (inst / "config/create.toml").write_text("mine")
            sync_config([entry("v2")], inst)
            assert (inst / "config/create.toml").read_text() == "mine"

            # player edits theirs and the server does move -> the update lands
            sync_config([entry("v3")], inst)
            assert (inst / "config/create.toml").read_text() == "v3"

            # a forced override wins even when something rewrote it locally
            (inst / "config/create.toml").write_text("rewritten by the mod")
            forced = {**entry("v3"), "force": True}
            sync_config([forced], inst)
            assert (inst / "config/create.toml").read_text() == "v3"

            # nothing escapes the instance
            sync_config([entry("x", "../../etc/passwd")], inst)
            assert not (td / "etc").exists()
        finally:
            BLOBS = keep_blobs

    # the refresh token is a credential: never group- or world-readable
    keep_a = ACCOUNT
    try:
        with tempfile.TemporaryDirectory() as td:
            ACCOUNT = Path(td) / "account.json"
            save_account({"refresh_token": "secret", "name": "x"})
            assert json.loads(ACCOUNT.read_text()) == {"refresh_token": "secret"}
            assert ACCOUNT.stat().st_mode & 0o077 == 0, oct(ACCOUNT.stat().st_mode)
            save_account({"refresh_token": "rotated"})      # overwrite keeps the mode
            assert json.loads(ACCOUNT.read_text())["refresh_token"] == "rotated"
            assert ACCOUNT.stat().st_mode & 0o077 == 0
    finally:
        ACCOUNT = keep_a

    print("demo ok")


if __name__ == "__main__":
    args = sys.argv[1:]
    cmd = args[0] if args else "hub"
    host, _, p = (args[1] if len(args) > 1 else "").partition(":")
    port = int(p or 25565)

    if cmd == "demo":
        demo()
    elif cmd == "ping":
        status = ping(host, port)
        print(json.dumps({"version": status.get("version"),
                          "unifiedmc": fetch_manifest(host, port, status)}, indent=2))
    elif cmd == "hub":
        run()
    elif cmd in ("mods", "add"):
        mf = resolve(host, port)
        key = instance_key(f"{host}:{port}", mf)
        if cmd == "mods":
            for hit in browse(mf, " ".join(args[2:])):
                print(f"{hit['id']:34} {hit['downloads']:>12,}  {hit['title']}  [{hit['source']}]")
                print(f"{'':34} {hit['description'][:88]}")
        else:
            if len(args) < 3:
                raise SystemExit("usage: add <server> <slug> [slug ...]")
            add_to_profile(key, mf, args[2:])
            print(f"-> {PROFILES / key / 'mods'}")
    elif cmd == "play":
        run(first=(host, port))
    elif cmd == "dry":
        # provision for real, print the launch command, start nothing
        mf = resolve(host, port) if host else {"minecraft": HUB_VERSION,
                                               "loader": {"type": "fabric", "version": None},
                                               "mods": []}
        addr = f"{host}:{port}" if host else None
        mods = mf["mods"] if host else hub_mods(HUB_VERSION)
        version_id, inst, java = provision(mf, mods, instance_key(addr or "hub", mf))
        opts = {**account(), "gameDirectory": str(inst), "executablePath": java}
        if addr:
            opts["jvmArguments"] = [f"-Dunifiedmc.ready={addr}"]
            opts["quickPlayMultiplayer"] = addr
        print("\n" + " ".join(mll.command.get_minecraft_command(version_id, str(MC), opts)))
    else:
        print(__doc__)
        sys.exit(1)
