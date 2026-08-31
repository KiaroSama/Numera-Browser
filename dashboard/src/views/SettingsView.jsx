/* Engine default, theme, and what the engine reports. */

import React from "react";
import { Blocks, HardDrive, Info, Moon, Palette, Settings, Sun } from "lucide-react";
import { T } from "../theme";
import { ENGINE_META, ENGINE_ORDER } from "../deviceData";

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
            <span><b>{counts.proxies}</b> proxies</span>
          </div>
        </div>

        <div className="ps-set-card">
          <div className="ps-set-h"><Info size={15} color={T.violet} /> About</div>
          <div className="ps-set-row">
            <div className="ps-set-label">Numera</div>
            <span className="ps-pill" style={{ background: T.violetDim, color: T.lilac }}>v{config?.version || "0.1.0"}</span>
          </div>
          <div className="ps-set-note" style={{ marginTop: 6 }}>Self-hosted anti-detect browser &amp; profile manager. {live ? "Engine connected." : "Running in demo mode — start numera serve to go live."}</div>
        </div>
      </div>
    </div>
  );
}

export { SettingsView };
