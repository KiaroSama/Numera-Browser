/* Ctrl-K: jump to a view or act on a profile by name. */

import { useEffect, useMemo, useRef, useState } from "react";
import { Activity, Blocks, FolderTree, Globe, Layers, Pencil, Play, Plus, Search, Settings, Square, Sun, Zap } from "lucide-react";
import { T } from "../theme";

function CommandPalette({ onClose, profiles, setView, setEditor, toggleRun, toggleTheme, newProfile }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const inputRef = useRef(null);
  useEffect(() => { inputRef.current?.focus(); }, []);

  const cmds = useMemo(() => {
    const nav = [
      ["Go to Profiles", () => setView("profiles"), Layers],
      ["Go to Health", () => setView("health"), Activity],
      ["Go to Engines", () => setView("engines"), Blocks],
      ["Go to Proxies", () => setView("proxies"), Globe],
      ["Go to Automation", () => setView("automation"), Zap],
      ["Go to Folders", () => setView("folders"), FolderTree],
      ["Go to Settings", () => setView("settings"), Settings],
    ].map(([label, run, Ic]) => ({ label, run, Ic, group: "Navigate" }));
    const actions = [
      ["New profile", () => newProfile(), Plus],
      ["Toggle light / dark theme", () => toggleTheme(), Sun],
    ].map(([label, run, Ic]) => ({ label, run, Ic, group: "Action" }));
    // Cap the per-profile commands so a 10k list doesn't build 20k entries.
    const profs = profiles.slice(0, 150).flatMap(p => ([
      { label: `Open ${p.name}`, run: () => setEditor(p), Ic: Pencil, group: "Profile" },
      { label: `${p.status === "running" ? "Stop" : "Launch"} ${p.name}`, run: () => toggleRun(p.id),
        Ic: p.status === "running" ? Square : Play, group: "Profile" },
    ]));
    return [...nav, ...actions, ...profs];
  }, [profiles]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    return (s ? cmds.filter(c => c.label.toLowerCase().includes(s)) : cmds).slice(0, 40);
  }, [q, cmds]);
  useEffect(() => { setSel(0); }, [q]);

  const runAt = (i) => { const c = filtered[i]; if (c) { c.run(); onClose(); } };
  const onKey = (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setSel(s => Math.min(s + 1, filtered.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel(s => Math.max(s - 1, 0)); }
    else if (e.key === "Enter") { e.preventDefault(); runAt(sel); }
  };

  return (
    <div className="ps-cmd-scrim" onClick={onClose}>
      <div className="ps-cmd" onClick={e => e.stopPropagation()}>
        <div className="ps-cmd-in">
          <Search size={16} color={T.dim} />
          <input ref={inputRef} value={q} onChange={e => setQ(e.target.value)} onKeyDown={onKey}
            placeholder="Search commands and profiles…" />
          <kbd className="ps-kbd">esc</kbd>
        </div>
        <div className="ps-cmd-list">
          {filtered.length === 0 && <div className="ps-cmd-empty">No matches</div>}
          {filtered.map((c, i) => (
            <button key={i} className={"ps-cmd-item" + (i === sel ? " on" : "")}
              onMouseEnter={() => setSel(i)} onClick={() => runAt(i)}>
              <c.Ic size={15} />
              <span style={{ flex: 1, textAlign: "left" }}>{c.label}</span>
              <span className="ps-cmd-group">{c.group}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export { CommandPalette };
