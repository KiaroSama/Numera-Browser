/* Engine default, theme, and what the engine reports. */

import React, { useEffect, useState } from "react";
import { Blocks, Check, Copy, HardDrive, Info, KeyRound, Moon, Palette, Plus,
         Settings, Sun, Trash2 } from "lucide-react";
import { api, auth } from "../api";
import { toast } from "../toast";
import { ConfirmModal } from "../ui";
import { T } from "../theme";
import { ENGINE_META, ENGINE_ORDER } from "../deviceData";

/* Named API tokens.
 *
 * `NUMERA_API_TOKEN` is one shared secret: rotating it locks out every client at
 * once and nothing records what a token was for. These can be revoked one at a
 * time, and the table says when each was last seen.
 *
 * The value is shown exactly once - only its hash is stored, so there is no
 * second chance and no file that leaking would expose. */
function ApiTokens({ live }) {
  const [state, setState] = useState(null);      // {tokens, envToken}
  const [name, setName] = useState("");
  const [minted, setMinted] = useState(null);    // the one-time value
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(null);

  const load = async () => {
    try { setState(await api.listTokens()); } catch { /* offline; a 401 routes to the locked screen */ }
  };
  useEffect(() => { if (live) load(); }, [live]);

  const create = async () => {
    if (!name.trim()) return toast.error("Give the token a name — an unnamed token is one nobody can safely revoke later.");
    // Read before the request that may change the answer.
    const wasOpen = !state?.tokens?.length && !state?.envToken;
    setBusy(true);
    try {
      const made = await api.createToken(name.trim());
      // If the API was open, this call is what locked it - on the same running
      // server, from the next request onwards. Adopt the value we were just
      // handed or `load()` below comes back 401, which now sends the whole app
      // to the locked screen and takes the one-time token value off the page
      // with it. When tokens already existed the session is holding a working
      // credential; silently swapping it for one minted for a CI runner would
      // mean revoking the CI runner locks the dashboard out.
      if (wasOpen) auth.set(made.token);
      setMinted(made);
      setName("");
      setCopied(false);
      await load();
    } catch (e) { toast.error(e.message); }
    setBusy(false);
  };

  const revoke = (t) => setConfirm({
    title: "Revoke this token?",
    danger: true,
    label: <><Trash2 size={15} /> Revoke</>,
    body: <>Anything using <b style={{ color: T.text }}>{t.name}</b> stops working immediately.
      Other tokens are unaffected. There is no undo — mint a new one if you need it back.</>,
    run: async () => {
      await api.revokeToken(t.id);
      toast.success("Revoked.");
      await load();
    },
  });

  if (!live) {
    return (
      <div className="ps-set-card">
        <div className="ps-set-h"><KeyRound size={15} color={T.violet} /> API tokens</div>
        <div className="ps-set-note">Connect the engine (<code>numera serve</code>) to mint and revoke tokens.</div>
      </div>
    );
  }

  const list = state?.tokens || [];
  return (
    <div className="ps-set-card">
      <div className="ps-set-h"><KeyRound size={15} color={T.violet} /> API tokens</div>
      <div className="ps-set-note" style={{ marginBottom: 12 }}>
        Bearer tokens for the HTTP API and MCP. Each can be revoked on its own, so one
        integration going away does not lock out the rest.
      </div>

      {minted && (
        <div className="ps-token-new">
          <div className="ps-set-label">Copy it now — this is the only time it is shown</div>
          <div className="ps-token-value">
            <code>{minted.token}</code>
            <button className="ps-btn ghost sm" onClick={() => {
              navigator.clipboard?.writeText(minted.token);
              setCopied(true);
            }}>{copied ? <><Check size={13} /> Copied</> : <><Copy size={13} /> Copy</>}</button>
          </div>
          <div className="ps-set-note" style={{ marginTop: 6 }}>
            Only its hash is stored. Send it as <code>Authorization: Bearer …</code>.
          </div>
          <button className="ps-btn ghost sm" style={{ marginTop: 8 }}
            onClick={() => setMinted(null)}>Done</button>
        </div>
      )}

      {list.length > 0 && (
        <table className="ps-table ps-token-table">
          <thead><tr><th>Name</th><th>Created</th><th>Last used</th><th style={{ width: 44 }} /></tr></thead>
          <tbody>{list.map(t => (
            <tr key={t.id}>
              <td>{t.name}</td>
              <td className="ps-mono dim">{new Date(t.created * 1000).toLocaleDateString()}</td>
              <td className="ps-mono dim">{t.lastUsed
                ? new Date(t.lastUsed * 1000).toLocaleDateString()
                : "never"}</td>
              <td><div className="ps-actions">
                <button title="Revoke" className="del" onClick={() => revoke(t)}><Trash2 size={13} /></button>
              </div></td>
            </tr>
          ))}</tbody>
        </table>
      )}

      <div className="ps-token-add">
        <input className="ps-in" value={name} placeholder="What is it for? e.g. CI runner"
          onChange={e => setName(e.target.value)}
          onKeyDown={e => e.key === "Enter" && create()} />
        <button className="ps-btn primary sm" disabled={busy || !name.trim()} onClick={create}>
          <Plus size={13} /> Create
        </button>
      </div>

      <div className="ps-set-note" style={{ marginTop: 10 }}>
        {list.length === 0 && !state?.envToken
          ? "No tokens: the API is open to anything that can reach this machine. That is fine on localhost and not fine behind a tunnel."
          : state?.envToken
            ? "NUMERA_API_TOKEN is also set, and stays valid. Revoking every token here will not reopen the API while it is."
            : "The API now requires one of these on every request."}
      </div>

      {confirm && <ConfirmModal
        title={confirm.title} body={confirm.body} confirmLabel={confirm.label} danger={confirm.danger}
        onCancel={() => setConfirm(null)}
        onConfirm={async () => {
          try { await confirm.run(); setConfirm(null); }
          catch (e) { toast.error(e.message); }
        }} />}
    </div>
  );
}

function SettingsView({ live, config, engines, defaultEngine, onChangeEngine, palette, toggleTheme, counts }) {
  return (
    <div className="ps-scroll">
      <div className="ps-view-head">
        <div>
          <h2 className="ps-view-title">Settings</h2>
          <p className="ps-view-sub">Defaults for new profiles, appearance, and where Numera keeps your data.</p>
        </div>
      </div>

      <div className="ps-set-grid">
        <div className="ps-set-card">
          <div className="ps-set-h"><Palette size={15} color={T.violet} /> Appearance</div>
          <div className="ps-set-row">
            <div>
              <div className="ps-set-label">Theme</div>
              <div className="ps-set-note">Switch the whole console between light and dark.</div>
            </div>
            <button className="ps-btn ghost" onClick={toggleTheme}>
              {palette === "light" ? <><Moon size={14} /> Dark</> : <><Sun size={14} /> Light</>}
            </button>
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><Blocks size={15} color={T.violet} /> Default launch engine</div>
          <div className="ps-set-note" style={{ marginBottom: 12 }}>Every new profile starts on this engine — you can still change it per profile.</div>
          <div className="ps-seg wrap">
            {ENGINE_ORDER.map(en => (
              <button key={en} className={defaultEngine === en ? "on" : ""} onClick={() => onChangeEngine(en)}
                title={engines && engines[en] === false ? "Not installed" : ""}>
                {ENGINE_META[en].label}{ENGINE_META[en].recommended ? " ★" : ""}
                {engines && engines[en] === false && <span className="ps-set-missing"> · not installed</span>}
              </button>
            ))}
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><HardDrive size={15} color={T.violet} /> Storage</div>
          {live && config?.root ? (
            <>
              <div className="ps-set-label">Data location</div>
              <pre className="ps-code" style={{ marginTop: 6 }}>{config.root}</pre>
              <div className="ps-set-note" style={{ marginTop: 8 }}>Profiles, sessions, proxies and settings all live here on your machine. Back up this folder to move everything to another computer.</div>
            </>
          ) : (
            <div className="ps-set-note">Connect the engine (<code>numera serve</code>) to see where your data is stored. In demo mode nothing is written to disk.</div>
          )}
          <div className="ps-set-counts">
            <span><b>{counts.profiles}</b> profiles</span>
            <span><b>{counts.folders}</b> folders</span>
            <span><b>{config?.proxyCount ?? counts.proxies}</b> proxies</span>
          </div>
        </div>

        <ApiTokens live={live} />

        <div className="ps-set-card">
          <div className="ps-set-h"><Info size={15} color={T.violet} /> About</div>
          <div className="ps-set-row">
            <div className="ps-set-label">Numera Browser</div>
            <span className="ps-pill" style={{ background: T.violetDim, color: T.lilac }}>{config?.version ? "v" + config.version : "version unknown"}</span>
          </div>
          <div className="ps-set-note" style={{ marginTop: 6 }}>Self-hosted anti-detect browser &amp; profile manager. {live ? "Engine connected." : "Running in demo mode — start numera serve to go live."}</div>
        </div>
      </div>
    </div>
  );
}

export { SettingsView };
