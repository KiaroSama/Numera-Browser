import type { Profile } from "../types";
/* Folder management: create, rename, delete, reassign. */

import { useState } from "react";
import { ArrowRight, Circle, FolderPlus, FolderTree, Pencil, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { T } from "../theme";
import { coherence } from "../deviceData";

function FoldersView({ folders, profiles, onOpen, onCreate, onRename, onDelete }: { folders: string[]; profiles: Profile[]; onOpen: (name: string) => void; onCreate: (name: string) => unknown; onRename: (from: string, to: string) => unknown; onDelete: (name: string) => unknown }) {
  const [newName, setNewName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);   // folder being renamed
  const [draft, setDraft] = useState("");
  const [confirm, setConfirm] = useState<string | null>(null);    // folder pending delete

  const stat = (name: string) => {
    const inF = profiles.filter(p => p.folder === name);
    return {
      total: inF.length,
      running: inF.filter(p => p.status === "running").length,
      coherent: inF.filter(p => coherence(p).length === 0).length,
    };
  };
  const submitNew = () => { const n = newName.trim(); if (n) { onCreate(n); setNewName(""); } };
  const commitRename = () => { if (editing) onRename(editing, draft); setEditing(null); setDraft(""); };

  return (
    <div className="ps-scroll">
      <div className="ps-view-head">
        <div>
          <h2 className="ps-view-title">Folders</h2>
          <p className="ps-view-sub">Group profiles by client, platform or project. Renaming or deleting a
            folder moves the profiles inside it — nothing is deleted.</p>
        </div>
        <div className="ps-newfold">
          <FolderPlus size={15} color={T.muted} />
          <input className="ps-in" placeholder="New folder name" value={newName}
            onChange={e => setNewName(e.target.value)}
            onKeyDown={e => e.key === "Enter" && submitNew()} />
          <button className="ps-btn primary sm" onClick={submitNew}><Plus size={14} /> Add</button>
        </div>
      </div>

      <div className="ps-fold-grid">
        {folders.length === 0 && (
          <div className="ps-empty" style={{ gridColumn: "1/-1", padding: 48 }}>
            <div className="ps-empty-ic"><FolderTree size={26} color={T.violet} /></div>
            <div style={{ color: T.muted, fontSize: 13 }}>No folders yet — add one above.</div>
          </div>
        )}
        {folders.map(f => {
          const s = stat(f);
          return (
            <div key={f} className="ps-fold-card">
              <div className="ps-fold-top">
                <div className="ps-fold-ic"><FolderTree size={16} color={T.violet} /></div>
                {editing === f ? (
                  <input autoFocus className="ps-in sm" value={draft}
                    onChange={e => setDraft(e.target.value)}
                    onKeyDown={e => { if (e.key === "Enter") commitRename(); if (e.key === "Escape") { setEditing(null); setDraft(""); } }}
                    onBlur={commitRename} />
                ) : (
                  <button className="ps-fold-name" onClick={() => onOpen(f)} title="Open in Profiles">{f}</button>
                )}
              </div>
              <div className="ps-fold-stats">
                <span><b>{s.total}</b> profile{s.total === 1 ? "" : "s"}</span>
                {s.running > 0 && <span className="ps-fold-run"><Circle size={7} fill={T.mint} color={T.mint} /> {s.running} running</span>}
                <span className="ps-fold-coh"><ShieldCheck size={12} /> {s.coherent}/{s.total}</span>
              </div>
              <div className="ps-fold-actions">
                <button className="ps-btn ghost sm" onClick={() => onOpen(f)}>Open <ArrowRight size={13} /></button>
                <div style={{ flex: 1 }} />
                <button className="ps-iconbtn sm" title="Rename" onClick={() => { setEditing(f); setDraft(f); }}><Pencil size={13} /></button>
                {confirm === f ? (
                  <button className="ps-btn danger sm" onClick={() => { onDelete(f); setConfirm(null); }} onMouseLeave={() => setConfirm(null)}>Confirm?</button>
                ) : (
                  <button className="ps-iconbtn sm danger" title="Delete folder" onClick={() => setConfirm(f)}><Trash2 size={13} /></button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export { FoldersView };
