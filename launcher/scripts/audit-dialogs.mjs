// Open every dialog and report any that scrolls sideways or runs off the bottom.
//
// Three separate sideways-scroll bugs got shipped before this existed, each found by hand and
// each in a different dialog - one of them only in German, where the strings are longer. This
// opens all of them at a given size and language and says which ones are wrong.
//
//   pnpm build
//   (cd build && python3 -m http.server 8765 --bind 127.0.0.1 &)
//   google-chrome --headless=new --remote-debugging-port=9222 about:blank &
//   node scripts/audit-dialogs.mjs 720 520 de-DE
//
// 720x520 is the window's own minimum, and de-DE is the longest language it ships.

import fs from "node:fs";

const [w, h, lang] = [+process.argv[2] || 720, +process.argv[3] || 520, process.argv[4] || "en-US"];

const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((x) => x.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
	const m = JSON.parse(e.data);
	if (pending.has(m.id)) pending.get(m.id)(m.result), pending.delete(m.id);
});
await new Promise((r) => ws.addEventListener("open", r));
const send = (method, params = {}) =>
	new Promise((r) => {
		const n = ++id;
		pending.set(n, r);
		ws.send(JSON.stringify({ id: n, method, params }));
	});

await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setUserAgentOverride", { userAgent: "Mozilla/5.0", acceptLanguage: lang });
await send("Page.enable");
await send("Page.navigate", { url: "http://127.0.0.1:8765/" });
await new Promise((r) => setTimeout(r, 2600));

// Every dialog in the app, and how to get to it. A dialog missing from here is a dialog
// nobody is checking, which is how three of these shipped.
const script = `(async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const dialog = () => document.querySelector("[role=dialog]");
  const scan = (label) => {
    const d = dialog();
    if (!d) return { label, opened: false };
    const bad = [];
    const walk = (el, dep) => {
      const c = (el.className || "").toString();
      if (el.scrollWidth > el.clientWidth + 1 && !c.includes("sr-only") && el.clientWidth > 0)
        bad.push(el.tagName.toLowerCase() + " " + c.slice(0, 34) + " " + el.scrollWidth + "/" + el.clientWidth);
      if (dep < 16) for (const ch of el.children) walk(ch, dep + 1);
    };
    walk(d, 0);
    const r = d.getBoundingClientRect();
    return { label, opened: true, w: Math.round(r.width), tall: r.bottom <= innerHeight + 1, overflow: bad };
  };
  const close = async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await wait(450);
    if (dialog()) { const x = dialog().querySelector("button"); x && x.click(); await wait(400); }
  };
  const nav = (re) => [...document.querySelectorAll("nav button")].find((b) => re.test(b.textContent));
  const add = () => [...document.querySelectorAll("button")].find((b) => /^(Server|Instance|Instanz)$/.test(b.textContent.trim()) && b.closest("main"));
  const byLabel = (re) => [...document.querySelectorAll("button")].find((b) => re.test((b.getAttribute("aria-label") || "") + " " + (b.getAttribute("title") || "")));
  const out = [];
  const open = async (label, go) => { await go(); await wait(950); out.push(scan(label)); await close(); };

  await open("Update", async () => { const p = [...document.querySelectorAll("header button")].find((b) => /is out|ist da/.test(b.textContent)); p && p.click(); });
  await open("Settings", async () => byLabel(/settings|einstellung/i)?.click());
  await open("Skin", async () => byLabel(/skin/i)?.click());
  await open("Add server", async () => { nav(/^(Server|Servers)/)?.click(); await wait(500); add()?.click(); });
  await open("Set up server", async () => { nav(/^(Server|Servers)/)?.click(); await wait(500); byLabel(/set up|einrichten/i)?.click(); });
  await open("Profile picker", async () => {
    nav(/^(Server|Servers)/)?.click(); await wait(600);
    // a server publishing no pack asks which setup to bring
    // The row that publishes no pack is the one that asks which setup to bring.
    const row = [...document.querySelectorAll("main article")].find((a) => /Vanilla Survival/.test(a.textContent));
    const play = [...(row?.querySelectorAll("button") ?? [])].find((b) => /play|spielen/i.test(b.textContent) && !b.disabled);
    play?.click();
  });
  await open("Create instance", async () => { nav(/Instan/)?.click(); await wait(500); add()?.click(); });
  await open("New server", async () => { nav(/Hosting|Hosten/)?.click(); await wait(500); add()?.click(); });
  return out;
})()`;

const r = await send("Runtime.evaluate", { expression: script, returnByValue: true, awaitPromise: true });
const rows = r.result?.value ?? [];

let problems = 0;
for (const row of rows) {
	if (!row.opened) {
		problems++;
		console.log(`  ??  ${row.label.padEnd(18)} did not open - not checked`);
		continue;
	}
	const bad = row.overflow.length > 0 || !row.tall;
	if (bad) problems++;
	const why = [...row.overflow, row.tall ? "" : "runs off the bottom"].filter(Boolean).join("; ");
	console.log(`  ${bad ? "BAD" : "OK "} ${row.label.padEnd(18)} ${String(row.w).padStart(4)}px  ${why}`);
}
console.log(problems === 0 ? `\nall ${rows.length} dialogs fit.` : `\n${problems} of ${rows.length} need looking at.`);
process.exit(problems === 0 ? 0 : 1);
