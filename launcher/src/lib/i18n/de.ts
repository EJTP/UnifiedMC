/**
 * German, the language the product is written in.
 *
 * This dict is also the fallback: a key missing from another language falls back here rather
 * than showing the player a raw dotted key.
 *
 * Sections are fixed so several people can edit different namespaces without colliding.
 */
export default {
	/* ---------------------------------------------------------------- common. */
	"common.cancel": "Abbrechen",
	"common.save": "Speichern",
	"common.add": "Hinzufügen",
	"common.create": "Anlegen",
	"common.remove": "Entfernen",
	"common.play": "Spielen",
	"common.starting": "Startet",
	"common.loading": "Lädt …",
	"common.more": "Mehr laden",
	"common.close": "Schließen",
	"common.back": "Zurück",
	"common.auto": "Auto",
	"common.none": "Keine",
	"common.detected": "erkannt",
	"common.optional": "Optional",
	"common.default": "Standard",
	"common.search": "Suchen",

	/* ------------------------------------------------------------------ nav. */
	"nav.servers": "Server",
	"nav.instances": "Instanzen",
	"nav.settings": "Einstellungen",

	/* -------------------------------------------------------------- servers. */
	"servers.title": "Server",
	"servers.empty.title": "Noch kein Server",
	"servers.empty.hint":
		"Adresse eintragen — Version, Mods und Configs holt sich der Client vom Server selbst.",
	"servers.address": "Adresse",
	"servers.addressInvalid": "Adresse wie mc.example.com oder mc.example.com:25565",
	"servers.name": "Name",
	"servers.add.title": "Server hinzufügen",
	"servers.addAction": "Server",
	"servers.refresh": "Neu prüfen",
	"servers.setup": "Einrichten",
	"servers.action.setup": "{name} einrichten",
	"servers.action.remove": "{name} entfernen",
	"servers.action.mods": "Mods für {name}",
	"servers.facts.checking": "wird geprüft …",
	"servers.facts.unreachable": "nicht erreichbar",
	"servers.facts.noManifest": "erreichbar · Version unklar — unter Einrichten festlegen",
	"servers.loading": "Server werden geladen …",
	"servers.mods": "{count} Mods",
	"servers.configs": "{count} Configs",
	"servers.players": "{online}/{max}",

	/* ------------------------------------------------------------ instances. */
	"instances.title": "Instanzen",
	"instances.empty.title": "Noch keine Instanz",
	"instances.empty.hint":
		"Version und Loader wählen, dann Mods dazu — für Singleplayer oder jeden Server, der dazu passt.",
	"instances.create.title": "Instanz anlegen",
	"instances.addAction": "Instanz",
	"instances.version": "Minecraft-Version",
	"instances.versionPick": "Version wählen",
	"instances.versionsUnavailable": "Versionsliste nicht erreichbar — Dialog erneut öffnen",
	"instances.loader": "Modding-Plattform",
	"instances.loaderVersion": "Loader-Version",
	"instances.newest": "Neueste",
	"instances.noMods": "ohne Mods",
	"instances.name": "Name",
	"instances.action.remove": "{name} entfernen",
	"instances.action.mods": "Mods für {name}",
	"instances.loading": "Instanzen werden geladen …",

	/* ---------------------------------------------------------------- setup. */
	"setup.title": "Server einrichten",
	"setup.description":
		"Nur nötig, wenn die Erkennung danebenliegt. Sonst bleibt alles auf Automatisch.",
	"setup.detected": "Erkannt",
	"setup.undetected": "noch nicht erkannt",
	"setup.version": "Minecraft-Version",
	"setup.versionAuto": "Automatisch",
	"setup.versionDetected": "{version} · erkannt",
	"setup.loader": "Modding-Plattform",
	"setup.loaderNone": "Vanilla (ohne Mods)",
	"setup.hint.proxy":
		"Proxys und ViaVersion melden die älteste Version, die sie annehmen — nicht die, die du spielen willst.",
	"setup.hint.vanilla":
		"Vanilla und Paper melden gar keinen Loader. Client-seitige Mods laufen trotzdem.",
	"setup.save": "Übernehmen",
	"setup.checking": "Wird geprüft …",

	/* -------------------------------------------------------------- profile. */
	"profile.title": "Womit spielen?",
	"profile.description": "{name}",
	"profile.serverDefault": "Wie der Server es vorgibt",
	"profile.serverDefaultHint": "noch nicht erkannt",
	"profile.create": "Neues Profil …",
	"profile.createHint": "Eigene Instanz mit deinen Mods für diesen Server",
	"profile.version": "Version",
	"profile.versionHint": "Der Server nennt nur das älteste Protokoll, das er annimmt. Unterstützt er mehrere Versionen, wähle hier die, die du spielen willst.",
	"profile.none": "Instanzen mit passender Version erscheinen hier.",
	"profile.play": "Spielen",

	/* ----------------------------------------------------------------- mods. */
	"mods.title": "Mods",
	"mods.tab.search": "Katalog",
	"mods.tab.installed": "Installiert",
	"mods.tab.pack": "Im Pack",
	"mods.hint.search": "Nur was ohne Server-Seite läuft. Grau: kommt schon mit dem Pack.",
	"mods.hint.installed": "Deine eigenen Mods für diesen Server.",
	"mods.hint.pack": "Alles, was der Server ausliefert.",
	"mods.search": "Mod suchen …",
	"mods.inPack": "im Pack",
	"mods.installed": "installiert",
	"mods.willBeRemoved": "wird entfernt",
	"mods.install": "Installieren",
	"mods.remove": "Entfernen",
	"mods.selected": "{count} ausgewählt",
	"mods.nothing": "Nichts hier",
	"mods.nothingOwn": "Noch nichts eigenes",
	"mods.notFound": "Nichts gefunden",
	"mods.installedCount": "{count} installiert, liegt in deinem Profil",
	"mods.installedNone": "Nichts installiert — nichts davon passt zu dieser Version.",
	"mods.removedCount": "{count} entfernt",
	"mods.removedNone": "Nichts entfernt",
	"mods.modrinthOnly": "Ohne CurseForge-Key nur Modrinth — Key unter Einstellungen › Erweitert.",

	/* ------------------------------------------------------------- settings. */
	"settings.title": "Einstellungen",
	"settings.tab.general": "Allgemein",
	"settings.tab.java": "Java",
	"settings.tab.advanced": "Erweitert",
	"settings.language": "Sprache",
	"settings.languageSystem": "Wie das System",
	"settings.offlineName": "Name ohne Anmeldung",
	"settings.offlineNameHint":
		"Gilt nur, solange keine Sitzung vorliegt. Online-Mode-Server lehnen ihn ab.",
	"settings.keepOpen": "Launcher offen lassen",
	"settings.keepOpenHint": "Fenster bleibt sichtbar, während das Spiel läuft.",
	"settings.memory": "Speicher",
	"settings.memoryAuto": "Automatisch",
	"settings.memoryHint": "Auto richtet sich nach der Größe des Packs.",
	"settings.machineMemory": "{gb} GB im Rechner",
	"settings.heapValue": "{mb} MB · {gb} GB",
	"settings.gc": "Garbage Collector",
	"settings.gc.balanced": "Ausgewogen (G1)",
	"settings.gc.balancedHint": "G1 mit kurzen Pausen. Passt für die meisten Packs.",
	"settings.gc.throughput": "Durchsatz (ZGC)",
	"settings.gc.throughputHint":
		"ZGC hält Pausen unter einer Millisekunde, braucht dafür mehr RAM und Kerne.",
	"settings.gc.default": "Standard",
	"settings.gc.defaultHint": "Nur -Xmx. Java wählt den Collector selbst.",
	"settings.gc.custom": "Eigene",
	"settings.gc.customHint": "Deine Flags, unverändert an die JVM. Nichts wird ergänzt.",
	"settings.flags": "Flags",
	"settings.flagsUnavailable": "Vorschau nicht verfügbar",
	"settings.flagsHint": "Genau das, was beim Start an die JVM geht.",
	"settings.flagsAutoHint":
		"In Auto wächst der Heap mit der Zahl der Mods; hier steht der Wert für ein Pack ohne Mods.",
	"settings.customArgs": "Eigene Flags",
	"settings.manifestPort": "Manifest-Port",
	"settings.manifestPortInvalid": "Port zwischen 1 und 65535.",
	"settings.manifestPortHint":
		"Auf diesem Port fragt der Launcher den Server nach seinem Manifest. Muss zum Server-Mod passen.",
	"settings.curseforgeKey": "CurseForge-Key",
	"settings.curseforgeKeyHint": "Ohne Key durchsucht der Katalog nur Modrinth.",
	"settings.dataDir": "Datenverzeichnis",

	/* ------------------------------------------------------------- progress. */
	"progress.prepare": "Wird vorbereitet",
	"progress.probe": "Server wird abgefragt",
	"progress.mods.sync": "Mods werden abgeglichen",
	"progress.mods.download": "Mods werden geladen",
	"progress.mods.summary": "Mods",
	"progress.config": "Konfiguration",
	"progress.java": "Java wird vorbereitet",
	"progress.download": "Dateien werden geladen",
	"progress.launch": "Minecraft startet",

	/* ---------------------------------------------------------------- error. */
	"error.noAddress": "Keine Adresse angegeben",
	"error.noVersionChosen": "Keine Version gewählt",
	"error.serverNoVersion": "Der Server ist erreichbar, sagt aber nicht welche Version er läuft.",
	"error.noManifest": "Der Server veröffentlicht kein Manifest.",
	"error.noSuchServer": "Diesen Server gibt es nicht mehr.",
	"error.noSuchInstance": "Diese Instanz gibt es nicht mehr.",
	"error.unknownLoader": "Unbekannte Modding-Plattform.",

	/* --------------------------------------------------------------- status. */
	"status.ready": "erreichbar",
	"status.unknown": "erreichbar, ohne Manifest",
	"status.offline": "nicht erreichbar",
	"status.checking": "wird geprüft",
	"status.notSignedIn": "nicht angemeldet",
	"status.notSignedInHint": "Ohne Anmeldung erreichst du nur Server im Offline-Modus",
	"status.offlineBadge": "offline"
};
