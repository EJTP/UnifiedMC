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
const t = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = t.find(x => x.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pend = new Map();
ws.addEventListener("message", e => { const m = JSON.parse(e.data); if (pend.has(m.id)) pend.get(m.id)(m.result), pend.delete(m.id); });
await new Promise(r => ws.addEventListener("open", r));
const send = (m, p = {}) => new Promise(r => { const n = ++id; pend.set(n, r); ws.send(JSON.stringify({ id: n, method: m, params: p })); });

const [w, h, lang] = [+process.argv[2], +process.argv[3], process.argv[4]];
await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setUserAgentOverride", { userAgent: "Mozilla/5.0", acceptLanguage: lang });
await send("Page.enable");
await send("Page.navigate", { url: "http://127.0.0.1:8765/" });
await new Promise(r => setTimeout(r, 2500));

const script = `(async () => {
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const scan = (label) => {
    const d = document.querySelector("[role=dialog]");
    if (!d) return { label, dialog: "did not open" };
    const bad = [];
    const walk = (el, dep) => {
      const c = (el.className || "").toString();
      if (el.scrollWidth > el.clientWidth + 1 && !c.includes("sr-only") && el.clientWidth > 0)
        bad.push(el.tagName.toLowerCase() + " " + c.slice(0, 38) + " " + el.scrollWidth + "/" + el.clientWidth);
      if (dep < 16) for (const ch of el.children) walk(ch, dep + 1);
    };
    walk(d, 0);
    const r = d.getBoundingClientRect();
    return { label, w: Math.round(r.width), h: Math.round(r.height),
             fitsTall: r.bottom <= innerHeight + 1, overflow: bad };
  };
  const close = async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await wait(450);
  };
  const nav = (re) => [...document.querySelectorAll("nav button")].find(b => re.test(b.textContent));
  const inMain = (re) => [...document.querySelectorAll("button")].find(b => re.test(b.textContent.trim()) && b.closest("main"));
  const out = [];

  // settings, from the sidebar gear
  [...document.querySelectorAll("nav button")].slice(-1)[0];
  const gear = [...document.querySelectorAll("nav button")].find(b => /settings|einstellung/i.test(b.getAttribute("aria-label") || ""));
  if (gear) { gear.click(); await wait(900); out.push(scan("Settings")); 
    const java = [...document.querySelectorAll("[role=dialog] button")].find(b => /java/i.test(b.textContent));
    if (java) { java.click(); await wait(500); out.push(scan("Settings / Java tab")); }
    await close(); }

  // add server
  nav(/^(Server|Servers)/)?.click(); await wait(500);
  inMain(/^(Server)$/)?.click(); await wait(900); out.push(scan("Add server")); await close();

  // create instance
  nav(/Instan/)?.click(); await wait(500);
  inMain(/^(Instance|Instanz)$/)?.click(); await wait(900); out.push(scan("Create instance")); await close();

  // new hosted server
  nav(/Hosting|Hosten/)?.click(); await wait(500);
  inMain(/^(Server)$/)?.click(); await wait(900); out.push(scan("New server")); await close();

  // per-server setup
  nav(/^(Server|Servers)/)?.click(); await wait(500);
  const setup = [...document.querySelectorAll("button")].find(b => /set up|einrichten/i.test((b.getAttribute("aria-label")||"") + (b.getAttribute("title")||"")));
  if (setup) { setup.click(); await wait(900); out.push(scan("Set up server")); await close(); }

  // skin dialog, from the account button
  const face = [...document.querySelectorAll("nav button")].find(b => /skin/i.test(b.getAttribute("aria-label") || ""));
  if (face) { face.click(); await wait(900); out.push(scan("Skin")); await close(); }

  return out;
})()`;
const r = await send("Runtime.evaluate", { expression: script, returnByValue: true, awaitPromise: true });
console.log(JSON.stringify(r.result?.value ?? r.result, null, 1));
process.exit(0);
