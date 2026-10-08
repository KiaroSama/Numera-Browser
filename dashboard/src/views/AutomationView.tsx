import type { AttachResult, ProfileId, ProfileViewProps } from "../types";
import { errorMessage } from "../types";
/* Attach a live profile to Selenium / Puppeteer / Playwright over CDP. */

import React, { useState } from "react";
import { Activity, Circle, Clipboard, RefreshCw, Server, Square, Zap } from "lucide-react";
import { api } from "../api";
import { T } from "../theme";
import { ENGINE_META } from "../deviceData";
import { toast } from "../toast";

const CDP_LANGS = ["Playwright", "Puppeteer", "Selenium"];

function cdpSnippet(lang: string, port: number) {
  const base = `http://127.0.0.1:${port}`;
  if (lang === "Puppeteer")
    return `const browser = await puppeteer.connect({ browserURL: "${base}" });\nconst [page] = await browser.pages();`;
  if (lang === "Selenium")
    return `from selenium import webdriver\nopts = webdriver.ChromeOptions()\nopts.add_experimental_option("debuggerAddress", "127.0.0.1:${port}")\ndriver = webdriver.Chrome(options=opts)`;
  return `from playwright.sync_api import sync_playwright\npw = sync_playwright().start()\nbrowser = pw.chromium.connect_over_cdp("${base}")\npage = browser.contexts[0].pages[0]`;
}

function AutomationView({ profiles, live, refresh }: ProfileViewProps) {
  const [info, setInfo] = useState<Record<string, AttachResult>>({});     // pid -> { port, wsUrl, snippets }
  const [busy, setBusy] = useState<ProfileId>("");
  const [lang, setLang] = useState("Playwright");
  const [copied, setCopied] = useState("");
  const [headless, setHeadless] = useState(false);
  const CHROMIUM = ["cloak", "patchright", "playwright"];

  const copy = (text: string, key: string) => {
    navigator.clipboard?.writeText(text);
    setCopied(key); setTimeout(() => setCopied(c => (c === key ? "" : c)), 1200);
  };
  const attach = async (id: ProfileId) => {
    setBusy(id);
    try {
      const r = await api.attach(id, headless);
      setInfo(m => ({ ...m, [String(id)]: r }));
      toast.success(`Attached on :${r.port}${r.headless ? " (headless)" : ""}`);
      await refresh();
    } catch (e) { toast.error("Attach failed: " + errorMessage(e)); }
    setBusy("");
  };
  const detach = async (id: ProfileId) => {
    setBusy(id);
    try { await api.stop(id); setInfo(m => { const n = { ...m }; delete n[String(id)]; return n; }); await refresh(); }
    catch (e) { toast.error("Detach failed: " + errorMessage(e)); }
    setBusy("");
  };

  const intro = (
    <div className="ps-hint"><Zap size={12} /> Numera opens the browser wearing the identity; your script <b style={{ color: T.text }}>attaches to that same window</b> over the Chrome DevTools Protocol — so the session, cookies, proxy and fingerprint stay exactly as set. (A browser your automation launches itself would be un-spoofed.) Chromium engines only — <code>cloak</code>, <code>patchright</code>, <code>playwright</code>.</div>
  );

  // Demo mode: no engine to attach to, so show how it works with the CLI and a
  // static example.
  if (!live) {
    return (
      <div className="ps-scroll">
        {intro}
        <div className="ps-cdp" style={{ marginTop: 12 }}>
          <div className="ps-cdp-h"><Server size={13} /> Start the engine, then attach</div>
          <pre className="ps-code">numera attach acct-01 --port 9222</pre>
          <div className="ps-lang-tabs">{CDP_LANGS.map(l => (
            <button key={l} className={"ps-chip" + (lang === l ? " on" : "")} onClick={() => setLang(l)}>{l}</button>
          ))}</div>
          <pre className="ps-code">{cdpSnippet(lang, 9222)}</pre>
        </div>
        <div className="ps-hint"><Activity size={12} /> Start <code>numera serve</code> to attach a real profile from here with one click.</div>
      </div>
    );
  }

  return (
    <div className="ps-scroll">
      {intro}
      <label className="ps-hcheck">
        <input type="checkbox" checked={headless} onChange={e => setHeadless(e.target.checked)} />
        <span>Headless — attach without a visible window (for servers / CI)</span>
      </label>
      {profiles.length === 0
        ? <div className="ps-empty"><Zap size={26} color={T.dim} /><div>No profiles to attach yet.</div></div>
        : (
          <table className="ps-table" style={{ marginTop: 12 }}>
            <thead><tr>
              <th>Profile</th><th>Engine</th><th>Status</th>
              <th style={{ width: 160, textAlign: "right" }}>Automation</th>
            </tr></thead>
            <tbody>
              {profiles.map((p, i) => {
                const canAttach = CHROMIUM.includes(p.engine || "playwright");
                const conn = info[String(p.id)];
                const attachedHere = !!conn;
                const running = p.status === "running";
                return (
                  <React.Fragment key={p.id}>
                    <tr className={"ps-rise" + (attachedHere ? " sel" : "")} style={{ animationDelay: `${Math.min(i, 12) * 28}ms` }}>
                      <td className="ps-pname">{p.name}</td>
                      <td><span className="ps-engine-chip">{ENGINE_META[p.engine ?? ""]?.label || p.engine || "Playwright"}</span></td>
                      <td>{attachedHere
                        ? <span style={{ color: T.mint, display: "inline-flex", gap: 5, alignItems: "center" }}><Circle size={9} /> attached :{conn.port}</span>
                        : running ? <span className="ps-mono dim">running</span> : <span className="ps-mono dim">stopped</span>}</td>
                      <td>
                        <div className="ps-actions" style={{ justifyContent: "flex-end" }}>
                          {attachedHere
                            ? <button className="danger" title="Close the attached browser" disabled={busy === p.id}
                                onClick={() => detach(p.id)}>{busy === p.id ? <RefreshCw size={13} className="ps-spin" /> : <Square size={13} />} Detach</button>
                            : <button className="run" disabled={!canAttach || running || busy === p.id}
                                title={!canAttach ? "This engine has no DevTools — switch to cloak/patchright/playwright"
                                       : running ? "Stop the profile first" : "Open with a CDP endpoint"}
                                onClick={() => attach(p.id)}>{busy === p.id ? <RefreshCw size={13} className="ps-spin" /> : <Zap size={13} />} Attach</button>}
                        </div>
                      </td>
                    </tr>
                    {attachedHere && (
                      <tr className="ps-cdp-row"><td colSpan={4}>
                        <div className="ps-cdp">
                          <div className="ps-cdp-h"><Server size={13} /> Connect your automation
                            <span style={{ flex: 1 }} />
                            <button className="ps-copy" onClick={() => copy(`http://127.0.0.1:${conn.port}`, p.id + "url")}>
                              <Clipboard size={12} /> {copied === p.id + "url" ? "Copied" : `127.0.0.1:${conn.port}`}</button>
                          </div>
                          <div className="ps-lang-tabs">{CDP_LANGS.map(l => (
                            <button key={l} className={"ps-chip" + (lang === l ? " on" : "")} onClick={() => setLang(l)}>{l}</button>
                          ))}</div>
                          <div className="ps-code-wrap">
                            <pre className="ps-code">{cdpSnippet(lang, conn.port)}</pre>
                            <button className="ps-copy floaty" onClick={() => copy(cdpSnippet(lang, conn.port), p.id + lang)}>
                              <Clipboard size={12} /> {copied === p.id + lang ? "Copied" : "Copy"}</button>
                          </div>
                        </div>
                      </td></tr>
                    )}
                  </React.Fragment>
                );
              })}
            </tbody>
          </table>
        )}
    </div>
  );
}

export { CDP_LANGS, cdpSnippet, AutomationView };
