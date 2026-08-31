/* ==================================================================
 *  Numera — theme
 *
 *  Everything that decides how the app looks: the palettes, the CSS custom
 *  properties they are emitted as, and the stylesheet itself. The first module
 *  to come out of App.jsx, and the cleanest cut — appearance depends on nothing
 *  else here, so it imports nothing local. The views followed later.
 *
 *  `T` is exported as a live mutable object on purpose. The remaining inline
 *  styles in JSX read it directly, and switching theme mutates it in place
 *  (see App). New CSS should use var(--…) and leave T alone; it exists to
 *  serve the inline reads that have not been migrated yet.
 * ================================================================== */

/* ---- design tokens: "ink violet" identity console ---------------- */
export const DARK = {
  bg: "#191d24", bg2: "#1f242c", surface: "#252b35", surface2: "#2c323e",
  line: "#39404e", lineSoft: "#2e343f",
  text: "#ECEDF5", muted: "#A2A7BC", dim: "#6E7488",
  violet: "#8B7CFF", violetDeep: "#6D5CE8", violetDim: "#2C265221",
  lilac: "#B7ADFF", mint: "#4FE0B0", mintDim: "#1B4A3E",
  amber: "#F2B84B", amberDim: "rgba(240,180,80,.14)", red: "#FF6B7A", grey: "#7C8296",
};
export const LIGHT = {
  bg: "#F5F5FB", bg2: "#FFFFFF", surface: "#FFFFFF", surface2: "#F0F0F8",
  line: "#E3E3EE", lineSoft: "#EDEDF5",
  text: "#171826", muted: "#5C5F79", dim: "#9A9DB8",
  violet: "#6D5CE8", violetDeep: "#5A48D6", violetDim: "#ECE9FB",
  lilac: "#6D5CE8", mint: "#12B886", mintDim: "#DCF5EC",
  amber: "#B7860B", amberDim: "#FBEFD3", red: "#E5484D", grey: "#8A8DA6",
};
// A live palette the whole app reads. Switching theme mutates it in place and
// re-renders from the root (see App), so every inline style and icon colour
// follows along without the theme being threaded through each component.
export const T = { ...DARK };

/* ---- CSS custom properties ---------------------------------------
 * The `T` object above can only reach inline styles, and an inline style
 * cannot express :hover, :focus-visible or a media query — which is the real
 * reason parts of this UI felt unfinished rather than any colour being wrong.
 * So the same tokens are also emitted as custom properties: dark on bare
 * :root, light as a [data-theme="light"] override, exactly as the target
 * design does. New CSS should reach for var(--…) and never interpolate T.
 *
 * The four-step background and text ramps come from the target design and are
 * the structural upgrade over the flatter set above: enough steps to separate
 * page, panel, card and raised surfaces without inventing one-off colours. */
const RAMPS = {
  dark: {
    "bg-0": "#191d24", "bg-1": "#1f242c", "bg-2": "#252b35", "bg-3": "#2c323e",
    "bg-hover": "#2a303b",
    "bd-1": "#2e343f", "bd-2": "#39404e", "bd-3": "#495264",
    "tx-1": "#eceef3", "tx-2": "#a8aebd", "tx-3": "#7d8494", "tx-4": "#727a8c",
  },
  light: {
    "bg-0": "#eef0f4", "bg-1": "#ffffff", "bg-2": "#f6f7fa", "bg-3": "#e9ebf1",
    "bg-hover": "#eceef4",
    "bd-1": "#e3e6ec", "bd-2": "#d3d7e0", "bd-3": "#c3c8d4",
    "tx-1": "#171826", "tx-2": "#5c5f79", "tx-3": "#7d8096", "tx-4": "#9a9db8",
  },
};

const varBlock = (mode) => {
  const flat = mode === "light" ? LIGHT : DARK;
  const pairs = [
    ...Object.entries(RAMPS[mode]).map(([k, v]) => `  --${k}: ${v};`),
    // The existing flat names stay available so the two systems agree while
    // the remaining inline styles are migrated across.
    ...Object.entries(flat).map(([k, v]) => `  --${k}: ${v};`),
  ];
  return pairs.join("\n");
};
export const FONT = {
  display: '"Space Grotesk", ui-sans-serif, system-ui, sans-serif',
  body: '"Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
  mono: '"JetBrains Mono", ui-monospace, Menlo, monospace',
};

export const makeCss = () => `
:root {
  color-scheme: dark;
${varBlock("dark")}
  --radius-sm: 6px; --radius: 8px; --radius-lg: 12px;
  --focus-ring: var(--lilac);

  /* Semantic layer. The names above say what a colour *is*; these say what it
     is *for*, so a surface can be restyled without hunting down every rule that
     happened to use the same hex. Both themes define the same set, which is why
     switching is one attribute and never a second code path. */
  --surface-page: var(--bg);
  --surface-panel: var(--surface);
  --surface-raised: var(--surface2);
  --text-primary: var(--text);
  --text-secondary: var(--muted);
  --text-tertiary: var(--dim);
  --border-strong: var(--line);
  --border-subtle: var(--lineSoft);
  --accent: var(--violet);
  --accent-strong: var(--violetDeep);
  --state-ok: var(--mint);
  --state-warn: var(--amber);
  --state-error: var(--red);
}
:root[data-theme="light"] {
  color-scheme: light;
${varBlock("light")}
  --focus-ring: ${LIGHT.violetDeep};

  /* Semantic layer. The names above say what a colour *is*; these say what it
     is *for*, so a surface can be restyled without hunting down every rule that
     happened to use the same hex. Both themes define the same set, which is why
     switching is one attribute and never a second code path. */
  --surface-page: var(--bg);
  --surface-panel: var(--surface);
  --surface-raised: var(--surface2);
  --text-primary: var(--text);
  --text-secondary: var(--muted);
  --text-tertiary: var(--dim);
  --border-strong: var(--line);
  --border-subtle: var(--lineSoft);
  --accent: var(--violet);
  --accent-strong: var(--violetDeep);
  --state-ok: var(--mint);
  --state-warn: var(--amber);
  --state-error: var(--red);
}

* { box-sizing: border-box; }

/* Keyboard focus.
   There was no *authored* focus style for buttons or links before - only
   .ps-in:focus and .ps-search:focus-within for text fields, plus one
   outline:none. Everything else fell through to Chromium's default ring,
   which is drawn in currentColor, differs between browsers, and vanishes
   wherever an outline is reset. So the ring is defined here instead, in the
   theme's own token.

   Not written with :where(): its zero specificity loses to the UA ring for
   outline-width and outline-color (verified in the browser - only
   outline-style took effect), which is exactly the trap that makes a focus
   fix look applied while rendering the browser default. Plain selectors.
   NOTE: no backticks anywhere in this block - it lives inside a JS template
   literal, and one backtick ends the whole stylesheet string. */
a:focus-visible,
button:focus-visible,
input:focus-visible,
select:focus-visible,
textarea:focus-visible,
[tabindex]:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
  /* A focused control inside a scroll area must not be clipped out of sight. */
  scroll-margin: 12px;
}

/* Honour the OS setting rather than deciding for the user. Transitions become
   instant instead of being dropped, so state changes are still legible. */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: .01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: .01ms !important;
    scroll-behavior: auto !important;
  }
}
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { background: var(--line); border-radius: 6px; }
::-webkit-scrollbar-thumb:hover { background: #33334f; }
::-webkit-scrollbar-track { background: transparent; }
::selection { background: color-mix(in srgb, var(--violet) 27%, transparent); }
a { color: inherit; }

@keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
@keyframes slidein { from { transform: translateX(28px); opacity: .3; } to { transform: none; opacity: 1; } }
@keyframes pop { from { transform: scale(.96) translateY(6px); opacity: 0; } to { transform: none; opacity: 1; } }
@keyframes pulsering { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--mint) 33%, transparent); } 70% { box-shadow: 0 0 0 6px color-mix(in srgb, var(--mint) 0%, transparent); } 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--mint) 0%, transparent); } }
@keyframes haloshift { 0%,100% { transform: rotate(0deg); } 50% { transform: rotate(180deg); } }
.ps-rise { animation: rise .42s cubic-bezier(.2,.7,.3,1) both; }

.ps-shell { display: flex; min-height: 100vh; }

.ps-root { background: var(--surface-page); color: var(--text-primary); min-height: 100vh; font-family: ${FONT.body}; }

/* extensions page */
.ps-ext-add { display: flex; gap: 8px; align-items: center; margin: 0 0 10px; }
.ps-ext-add .ps-in { flex: 1; }
.ps-ext-err { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--state-error); margin-bottom: 10px; }
.ps-ext-list { display: flex; flex-direction: column; gap: 6px; }
.ps-ext-head { display: flex; gap: 10px; padding: 0 11px 6px; font-size: 11px; font-weight: 600; letter-spacing: .4px; text-transform: uppercase; color: var(--text-tertiary); }
.ps-ext-row { display: flex; align-items: center; gap: 10px; padding: 9px 11px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-panel); transition: background .16s, border-color .16s; }
.ps-ext-row:hover { background: var(--bg-hover); border-color: var(--bd-2); }
.ps-ext-ic { display: flex; width: 30px; height: 30px; align-items: center; justify-content: center; border-radius: 8px; background: var(--violetDim); flex-shrink: 0; overflow: hidden; }
.ps-ext-ic:has(.ps-ext-logo) { background: transparent; }
.ps-ext-logo { width: 26px; height: 26px; object-fit: contain; display: block; }
.ps-ext-apply { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-top: 14px; padding: 12px 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-panel); }
.ps-ext-apply-t { font-size: 13px; font-weight: 600; color: var(--text-primary); margin-bottom: 2px; }
.ps-ext-done { display: flex; align-items: center; gap: 6px; margin-top: 8px; font-size: 12px; color: var(--state-ok); }
.ps-ext-path { flex: 1; font-size: 12px; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ps-ext-named { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.ps-ext-named b { font-size: 13px; font-weight: 600; color: var(--text); }
.ps-ext-sub { font-size: 11px; color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* sidebar */
.ps-side { width: 244px; flex-shrink: 0; background: linear-gradient(180deg, var(--bg2), var(--bg)); border-right: 1px solid var(--line); display: flex; flex-direction: column; padding: 18px 13px; position: sticky; top: 0; height: 100vh; }
.ps-brand { display: flex; align-items: center; gap: 12px; padding: 4px 6px 20px; }
.ps-logo { position: relative; width: 36px; height: 36px; border-radius: 11px; overflow: hidden; display: flex; align-items: center; justify-content: center; }
.ps-logo img { width: 100%; height: 100%; object-fit: contain; display: block; }
.ps-logo-glow { position: absolute; inset: -40%; background: conic-gradient(from 0deg, color-mix(in srgb, var(--mint) 0%, transparent), color-mix(in srgb, var(--mint) 53%, transparent), color-mix(in srgb, var(--mint) 0%, transparent) 40%); animation: haloshift 6s linear infinite; opacity: .5; }
.ps-brand-name { font-family: ${FONT.display}; font-weight: 700; font-size: 16px; letter-spacing: -.2px; }
.ps-brand-sub { font-size: 10px; color: var(--dim); letter-spacing: 3px; text-transform: uppercase; margin-top: 1px; }
.ps-nav { display: flex; flex-direction: column; gap: 3px; }
.ps-navitem { display: flex; align-items: center; gap: 11px; padding: 9px 11px; border-radius: 9px; border: none; background: transparent; color: var(--muted); font-size: 13px; font-weight: 500; cursor: pointer; transition: .14s; text-align: left; position: relative; }
.ps-navitem:hover { background: var(--surface); color: var(--text); }
.ps-navitem.active { background: var(--surface2); color: var(--text); box-shadow: inset 2px 0 0 var(--violet); }
.ps-folders { margin-top: 22px; flex: 1; overflow-y: auto; }
.ps-folders-h { display: flex; align-items: center; font-size: 10px; letter-spacing: 1.6px; text-transform: uppercase; color: var(--dim); padding: 0 8px 8px; }
.ps-folder { display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 9px; border-radius: 8px; border: none; background: transparent; color: var(--muted); font-size: 12.5px; cursor: pointer; transition: .14s; }
.ps-folder:hover { background: var(--surface); color: var(--text); }
.ps-folder.active { color: var(--lilac); background: var(--violetDim); }
.ps-count { font-size: 11px; color: var(--dim); font-family: ${FONT.mono}; }
.ps-side-foot { display: flex; align-items: center; gap: 8px; font-size: 11.5px; color: var(--muted); padding: 11px 10px; margin-top: 8px; border-top: 1px solid var(--lineSoft); }
.ps-foot-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--grey); }
.ps-side-foot.live .ps-foot-dot { background: var(--mint); animation: pulsering 2.4s infinite; }
.ps-side-foot.demo .ps-foot-dot { background: var(--amber); }
.ps-foot-ver { margin-left: auto; font-family: ${FONT.mono}; font-size: 10.5px; color: var(--dim); }

/* main */
.ps-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.ps-stats { display: flex; align-items: center; gap: 26px; padding: 18px 26px; border-bottom: 1px solid var(--line); background: var(--bg2); flex-wrap: wrap; }
.ps-stat { display: flex; align-items: center; gap: 11px; }
.ps-stat-ic { width: 34px; height: 34px; border-radius: 10px; background: var(--surface); border: 1px solid var(--line); display: flex; align-items: center; justify-content: center; }
.ps-stat-ic.pulse { animation: pulsering 2.4s infinite; }
.ps-stat-val { font-size: 19px; font-weight: 700; font-family: ${FONT.display}; letter-spacing: -.3px; }
.ps-stat-label { font-size: 10.5px; color: var(--dim); text-transform: uppercase; letter-spacing: 1.2px; margin-top: 1px; }

.ps-btn { display: inline-flex; align-items: center; gap: 7px; border-radius: 10px; padding: 9px 15px; font-size: 13px; font-weight: 600; cursor: pointer; border: 1px solid transparent; transition: .14s; font-family: inherit; }
.ps-btn.sm { padding: 7px 11px; font-size: 12px; }
.ps-btn.primary { background: linear-gradient(135deg, var(--violet), var(--violetDeep)); color: #fff; box-shadow: 0 3px 14px color-mix(in srgb, var(--violet) 20%, transparent); }
.ps-btn.primary:hover { filter: brightness(1.08); transform: translateY(-1px); }
.ps-btn.primary:active { transform: none; }
.ps-btn.primary:disabled { opacity: .4; cursor: not-allowed; box-shadow: none; transform: none; }
.ps-btn.ghost { background: var(--surface); border-color: var(--line); color: var(--text); }
.ps-btn.ghost:hover { background: var(--surface2); border-color: #33334f; }

.ps-toolbar { display: flex; align-items: center; gap: 12px; padding: 16px 26px; }
.ps-search { display: flex; align-items: center; gap: 8px; background: var(--surface); border: 1px solid var(--line); border-radius: 10px; padding: 9px 12px; width: 320px; max-width: 42vw; transition: .14s; }
.ps-search:focus-within { border-color: var(--violet); box-shadow: 0 0 0 3px color-mix(in srgb, var(--violet) 13%, transparent); }
.ps-search input { background: transparent; border: none; outline: none; color: var(--text); font-size: 13px; width: 100%; font-family: inherit; }
.ps-crumb { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
.ps-bulkbar { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); background: var(--surface); padding: 5px 8px 5px 12px; border-radius: 9px; border: 1px solid var(--line); }
.ps-bulkbar button { display: flex; align-items: center; gap: 5px; border: none; background: var(--violetDim); color: var(--lilac); padding: 5px 9px; border-radius: 7px; font-size: 12px; cursor: pointer; font-family: inherit; }
.ps-bulkbar button.danger { background: transparent; color: var(--red); }

.ps-scroll { flex: 1; overflow: auto; padding: 8px 26px 28px; }

/* table */
.ps-table { width: 100%; border-collapse: separate; border-spacing: 0; }
.ps-table th { text-align: left; font-size: 10.5px; letter-spacing: 1px; text-transform: uppercase; color: var(--dim); font-weight: 600; padding: 11px 13px; border-bottom: 1px solid var(--line); position: sticky; top: 0; background: var(--bg); z-index: 1; }
.ps-table th:last-child { text-align: right; }
.ps-table td { padding: 13px; border-bottom: 1px solid var(--lineSoft); vertical-align: middle; }
.ps-table tbody tr { transition: background .14s; }
.ps-table tbody tr:hover td { background: var(--bg2); }
.ps-table tr.sel td { background: var(--violetDim); }
.ps-name { display: flex; align-items: center; gap: 12px; }
/* The avatar is decoration that carries identity, so it sits quietly: a soft
   surface behind it keeps a sparse pattern from floating on the row. */
.ps-identicon { display: block; flex: none; border-radius: 8px; overflow: hidden;
  background: var(--bg); border: 1px solid var(--line); line-height: 0; }
.ps-identicon svg { display: block; }
.ps-pname { font-weight: 600; font-size: 13px; }
/* The pencil only appears on hover: a permanent one on every row is noise, and
   the name is the obvious thing to click anyway. */
.ps-pname.editable { display: inline-flex; align-items: center; gap: 5px; cursor: text;
  border-radius: 5px; padding: 1px 5px; margin-left: -5px; transition: background .12s; }
.ps-pname.editable:hover { background: var(--bg); }
.ps-pname.busy { opacity: .55; }
.ps-pname-pen { opacity: 0; color: var(--muted); transition: opacity .12s; }
.ps-pname.editable:hover .ps-pname-pen { opacity: 1; }
.ps-pname-in { font: 600 13px/1.2 inherit; font-family: inherit; color: var(--text);
  background: var(--bg); border: 1px solid var(--violetDim); border-radius: 5px;
  padding: 1px 5px; margin-left: -5px; width: 100%; max-width: 220px; outline: none; }
.ps-status { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.ps-status.running { background: var(--mint); animation: pulsering 2.4s infinite; }
.ps-status.stopped { background: var(--grey); }
.ps-tags { display: flex; gap: 4px; margin-top: 4px; flex-wrap: wrap; }
.ps-tag { font-size: 10px; color: var(--muted); background: var(--surface); border: 1px solid var(--line); padding: 1px 6px; border-radius: 5px; }
.ps-tag.clickable { cursor: pointer; transition: border-color .15s, color .15s; }
.ps-tag.clickable:hover { color: var(--text); border-color: var(--violet); }
.ps-tag.on { color: var(--bg); background: var(--violet); border-color: var(--violet); }
.ps-shared { display: inline-flex; align-items: center; gap: 3px; margin-left: 8px; font-size: 9px; font-weight: 700; letter-spacing: .4px; text-transform: uppercase; color: var(--amber); background: var(--amberDim, rgba(240,180,80,.14)); padding: 1px 6px; border-radius: 5px; vertical-align: middle; }
.ps-cdp-row td { padding: 0 !important; background: none; }
.ps-cdp { margin: 2px 0 14px; border: 1px solid var(--line); border-radius: 10px; background: var(--surface); padding: 14px; }
.ps-cdp-h { display: flex; align-items: center; gap: 7px; font-size: 12px; font-weight: 600; color: var(--muted); margin-bottom: 10px; }
.ps-lang-tabs { display: flex; gap: 6px; margin-bottom: 10px; }
.ps-code-wrap { position: relative; }
.ps-code { font-family: ${FONT.mono}; font-size: 12px; line-height: 1.55; color: var(--text); background: var(--bg); border: 1px solid var(--line); border-radius: 8px; padding: 12px 14px; margin: 0; overflow-x: auto; white-space: pre; }
.ps-copy { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; font-family: ${FONT.mono}; color: var(--lilac); background: var(--violetDim); border: 1px solid var(--line); border-radius: 6px; padding: 3px 9px; cursor: pointer; transition: color .15s, border-color .15s; }
.ps-copy:hover { color: var(--text); border-color: var(--violet); }
.ps-copy.floaty { position: absolute; top: 8px; right: 8px; }
.ps-hcheck { display: inline-flex; align-items: center; gap: 8px; margin: 12px 0 2px; font-size: 12px; color: var(--muted); cursor: pointer; }

.ps-iconbtn { display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: 9px; border: 1px solid var(--line); background: var(--surface); color: var(--muted); cursor: pointer; transition: color .15s, border-color .15s; }
.ps-iconbtn:hover { color: var(--text); border-color: var(--violet); }

.ps-toaster { position: fixed; top: 18px; right: 18px; display: flex; flex-direction: column; gap: 10px; z-index: 1000; }
.ps-toast { display: flex; align-items: center; gap: 10px; min-width: 240px; max-width: 380px; padding: 11px 14px; border-radius: 10px; font-size: 13px; font-weight: 500; color: var(--text); background: var(--surface2); border: 1px solid var(--line); box-shadow: 0 10px 30px rgba(0,0,0,.35); cursor: pointer; animation: toastIn .22s ease; }
.ps-toast.success { border-left: 3px solid var(--mint); } .ps-toast.success svg { color: var(--mint); }
.ps-toast.error { border-left: 3px solid var(--red); } .ps-toast.error svg { color: var(--red); }
.ps-toast.info { border-left: 3px solid var(--violet); } .ps-toast.info svg { color: var(--lilac); }
@keyframes toastIn { from { opacity: 0; transform: translateX(20px); } to { opacity: 1; transform: none; } }

.ps-cmd-scrim { position: fixed; inset: 0; background: rgba(0,0,0,.5); backdrop-filter: blur(2px); display: flex; align-items: flex-start; justify-content: center; padding-top: 12vh; z-index: 1100; animation: cmdFade .12s ease; }
@keyframes cmdFade { from { opacity: 0; } to { opacity: 1; } }
.ps-cmd { width: min(560px, 92vw); background: var(--bg2); border: 1px solid var(--line); border-radius: 14px; box-shadow: 0 24px 70px rgba(0,0,0,.5); overflow: hidden; }
.ps-cmd-in { display: flex; align-items: center; gap: 10px; padding: 14px 16px; border-bottom: 1px solid var(--line); }
.ps-cmd-in input { flex: 1; background: none; border: none; outline: none; color: var(--text); font-size: 15px; font-family: ${FONT.body}; }
.ps-kbd { font-size: 10px; font-family: ${FONT.mono}; color: var(--dim); border: 1px solid var(--line); border-radius: 5px; padding: 2px 6px; }
.ps-cmd-list { max-height: 46vh; overflow-y: auto; padding: 6px; }
.ps-cmd-item { display: flex; align-items: center; gap: 11px; width: 100%; padding: 9px 12px; border: none; background: none; color: var(--muted); font-size: 13px; border-radius: 8px; cursor: pointer; text-align: left; }
.ps-cmd-item.on { background: var(--surface2); color: var(--text); }
.ps-cmd-item.on svg { color: var(--violet); }
.ps-cmd-group { font-size: 10px; text-transform: uppercase; letter-spacing: .5px; color: var(--dim); }
.ps-cmd-empty { padding: 24px; text-align: center; color: var(--dim); font-size: 13px; }
.ps-env { display: flex; align-items: center; gap: 8px; }
.ps-os { font-size: 11px; font-weight: 600; color: var(--lilac); background: var(--violetDim); padding: 2px 8px; border-radius: 6px; }
.ps-engine-chip { font-size: 11px; font-weight: 500; color: var(--muted); background: var(--surface); border: 1px solid var(--line); padding: 3px 9px; border-radius: 6px; }
.ps-mono { font-family: ${FONT.mono}; font-size: 11.5px; }
.ps-mono.dim, .dim { color: var(--dim); }
.ps-proxy { display: flex; align-items: center; gap: 6px; font-size: 12px; }
.ps-proxy.off { color: var(--dim); }
.ps-pill { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 600; padding: 3px 9px; border-radius: 20px; }
.ps-pill.ok { color: var(--mint); background: color-mix(in srgb, var(--mint) 10%, transparent); }
.ps-pill.warn { color: var(--amber); background: color-mix(in srgb, var(--amber) 10%, transparent); cursor: help; }
.ps-actions { display: flex; gap: 5px; justify-content: flex-end; }
.ps-actions button { width: 31px; height: 31px; border-radius: 8px; border: 1px solid var(--line); background: var(--surface); color: var(--muted); display: flex; align-items: center; justify-content: center; cursor: pointer; transition: .14s; }
.ps-actions button:hover { color: var(--text); background: var(--surface2); transform: translateY(-1px); }
.ps-actions button.run:hover { color: var(--mint); border-color: var(--mint); }
.ps-actions button.stop { color: var(--amber); border-color: color-mix(in srgb, var(--amber) 40%, transparent); }
.ps-actions button.del:hover { color: var(--red); border-color: var(--red); }
.ps-empty { display: flex; flex-direction: column; align-items: center; gap: 13px; padding: 64px; color: var(--muted); font-size: 13px; }
.ps-empty-ic { width: 52px; height: 52px; border-radius: 14px; background: var(--violetDim); display: flex; align-items: center; justify-content: center; }
input[type=checkbox] { accent-color: var(--violet); width: 15px; height: 15px; cursor: pointer; }

/* health view */
.ps-health-summary { display: flex; gap: 10px; align-items: center; margin: 4px 0 16px; flex-wrap: wrap; }
.ps-hchip { display: flex; flex-direction: column; gap: 2px; padding: 10px 15px; border-radius: 12px; background: var(--surface); border: 1px solid var(--line); min-width: 92px; }
.ps-hchip-val { font-family: ${FONT.display}; font-size: 19px; font-weight: 600; color: var(--text); }
.ps-hchip-label { font-size: 10.5px; letter-spacing: .5px; text-transform: uppercase; color: var(--dim); }
.ps-grade { display: inline-flex; align-items: center; justify-content: center; min-width: 22px; height: 22px; padding: 0 6px; border-radius: 6px; border: 1px solid; font-family: ${FONT.mono}; font-size: 12px; font-weight: 700; }
.ps-table tr.ps-attention td { box-shadow: inset 2px 0 0 var(--amber); }
.ps-linkish { background: none; border: none; color: var(--text); font-weight: 600; font-size: 13px; cursor: pointer; padding: 0; font-family: inherit; }
.ps-linkish:hover { color: var(--violet); text-decoration: underline; }
.ps-runtag { margin-left: 8px; font-size: 9px; font-weight: 700; letter-spacing: .5px; text-transform: uppercase; color: var(--mint); background: var(--mintDim); padding: 1px 5px; border-radius: 5px; }

/* engines view */
.ps-eng-hero { padding: 30px 32px 26px; margin: 8px 0 22px; border-radius: 18px; background: radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--violet) 13%, transparent), transparent 55%), linear-gradient(180deg, var(--bg2), var(--surface)); border: 1px solid var(--line); position: relative; overflow: hidden; }
.ps-eng-hero-badge { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 600; color: var(--lilac); background: var(--violetDim); border: 1px solid color-mix(in srgb, var(--violet) 27%, transparent); padding: 4px 10px; border-radius: 20px; margin-bottom: 16px; }
.ps-eng-title { font-family: ${FONT.display}; font-size: 30px; font-weight: 700; line-height: 1.12; letter-spacing: -.6px; margin: 0 0 12px; }
.ps-eng-title .grad { background: linear-gradient(100deg, var(--violet), var(--mint)); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; }
.ps-eng-lead { color: var(--muted); font-size: 14px; line-height: 1.65; max-width: 640px; margin: 0 0 18px; }
.ps-eng-flow { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.ps-flow-node { display: inline-flex; align-items: center; gap: 7px; font-size: 12.5px; font-weight: 500; color: var(--text); background: var(--surface); border: 1px solid var(--line); padding: 8px 13px; border-radius: 10px; }
.ps-flow-node svg { color: var(--violet); }

.ps-sechead { display: flex; align-items: center; gap: 9px; margin: 8px 2px 14px; }
.ps-sechead-ic { color: var(--violet); display: flex; }
.ps-sechead-t { font-family: ${FONT.display}; font-size: 14px; font-weight: 600; }
.ps-sechead-n { font-size: 11.5px; color: var(--dim); }
.ps-eng-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 15px; margin-bottom: 30px; }
.ps-eng-card { background: var(--surface); border: 1px solid var(--line); border-radius: 15px; padding: 18px; display: flex; flex-direction: column; gap: 13px; transition: .16s; }
.ps-eng-card:hover { transform: translateY(-3px); border-color: #38385a; box-shadow: 0 10px 30px #00000055; }
.ps-eng-card.featured { border-color: color-mix(in srgb, var(--violet) 33%, transparent); background: linear-gradient(180deg, color-mix(in srgb, var(--violet) 5%, transparent), var(--surface)); }
.ps-eng-card-top { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; }
.ps-eng-name { font-family: ${FONT.display}; font-size: 16px; font-weight: 600; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.ps-rec { display: inline-flex; align-items: center; gap: 3px; font-size: 9.5px; font-weight: 700; text-transform: uppercase; letter-spacing: .4px; color: var(--violet); background: var(--violetDim); padding: 2px 7px; border-radius: 12px; font-family: ${FONT.body}; }
.ps-rec svg { fill: var(--violet); }
.ps-eng-kind { font-size: 11.5px; color: var(--dim); margin-top: 3px; }
.ps-eng-status { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 600; white-space: nowrap; }
.ps-eng-status-dot { width: 7px; height: 7px; border-radius: 50%; }
.ps-strength { display: flex; align-items: center; gap: 9px; }
.ps-strength-label { font-size: 10px; text-transform: uppercase; letter-spacing: .8px; color: var(--dim); }
.ps-strength-bars { display: flex; gap: 4px; }
.ps-sb { width: 22px; height: 5px; border-radius: 3px; background: var(--line); transition: .2s; }
.ps-sb.on { background: linear-gradient(90deg, var(--violet), var(--mint)); }
.ps-eng-desc { font-size: 12.5px; color: var(--muted); line-height: 1.6; margin: 0; flex: 1; }
.ps-eng-install { display: flex; align-items: center; gap: 8px; background: var(--bg); border: 1px solid var(--line); border-radius: 9px; padding: 8px 10px; }
.ps-eng-install code { font-family: ${FONT.mono}; font-size: 11px; color: var(--lilac); flex: 1; overflow-x: auto; white-space: nowrap; }
.ps-eng-install button { flex-shrink: 0; width: 28px; height: 28px; border-radius: 7px; border: 1px solid var(--line); background: var(--surface); color: var(--muted); cursor: pointer; display: flex; align-items: center; justify-content: center; transition: .14s; }
.ps-eng-install button:hover { color: var(--text); border-color: var(--violet); }
.ps-eng-builtin { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--muted); background: color-mix(in srgb, var(--mintDim) 33%, transparent); border: 1px solid color-mix(in srgb, var(--mint) 20%, transparent); border-radius: 9px; padding: 9px 11px; }
.ps-eng-docs { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 600; color: var(--lilac); text-decoration: none; }
.ps-eng-docs:hover { color: var(--violet); }
.ps-eng-note { display: flex; align-items: center; gap: 9px; font-size: 12.5px; color: var(--muted); background: var(--surface); border: 1px solid var(--line); border-radius: 12px; padding: 14px 16px; }
.ps-eng-note code { font-family: ${FONT.mono}; font-size: 11.5px; color: var(--lilac); background: var(--bg); padding: 2px 6px; border-radius: 5px; }

/* overlay + drawer */
.ps-overlay { position: fixed; inset: 0; background: #05060acc; backdrop-filter: blur(4px); display: flex; justify-content: flex-end; z-index: 50; animation: rise .18s ease; }
.ps-overlay.center { justify-content: center; align-items: center; }
/* The profile editor is a page, not a drawer. Its fields describe one device
   and several only mean anything read against each other, so they get room and
   a summary column instead of a 580px strip. NOTE: no backticks in this block -
   it lives inside a JS template literal. */
.ps-overlay.ps-page { align-items: stretch; justify-content: center; padding: 0; }
.ps-editor { width: 100%; max-width: 1500px; height: 100%; background: var(--bg2); border-left: 1px solid var(--line); border-right: 1px solid var(--line); display: flex; flex-direction: column; animation: slidein .22s cubic-bezier(.2,.7,.3,1); }
.ps-editor-h { display: flex; align-items: center; gap: 14px; padding: 16px 22px; border-bottom: 1px solid var(--line); }
.ps-editor-id { display: flex; flex-direction: column; gap: 3px; }
.ps-editor-name { font-family: ${FONT.display}; font-size: 19px; font-weight: 600; border: none; background: transparent; padding: 0; min-width: 260px; }
.ps-editor-name:focus { outline: none; }
.ps-editor-folder { max-width: 200px; }
.ps-editor-main { flex: 1; display: flex; min-height: 0; }
.ps-editor-body { flex: 1; overflow-y: auto; padding: 20px 22px 28px; }
/* Two columns of fields once there is room; one on a narrow window. */
@media (min-width: 1100px) {
  .ps-editor-body { column-count: 2; column-gap: 30px; }
  .ps-editor-body > * { break-inside: avoid; }
}
.ps-summary { width: 300px; flex-shrink: 0; border-left: 1px solid var(--line); background: var(--bg); overflow-y: auto; padding: 16px 18px; }
.ps-summary-h { display: flex; align-items: center; justify-content: space-between; font-size: 12px; font-weight: 600; letter-spacing: .3px; text-transform: uppercase; color: var(--text-tertiary); margin-bottom: 12px; }
.ps-summary-list { margin: 0; }
.ps-summary-row { display: flex; gap: 10px; padding: 5px 0; border-bottom: 1px solid var(--lineSoft); font-size: 11.5px; }
.ps-summary-row dt { flex: 0 0 108px; color: var(--text-tertiary); }
.ps-summary-row dd { margin: 0; flex: 1; min-width: 0; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: ${FONT.mono}; font-size: 11px; }
.ps-summary-iss { margin-top: 14px; font-size: 11.5px; line-height: 1.7; color: var(--amber); }
.ps-drawer { width: 580px; max-width: 96vw; height: 100%; background: var(--bg2); border-left: 1px solid var(--line); display: flex; flex-direction: column; animation: slidein .24s cubic-bezier(.2,.7,.3,1); }
.ps-modal { width: 520px; max-width: 94vw; background: var(--bg2); border: 1px solid var(--line); border-radius: 16px; overflow: hidden; }
.ps-pop { animation: pop .2s cubic-bezier(.2,.7,.3,1); }
.ps-drawer-h { display: flex; align-items: flex-start; justify-content: space-between; padding: 22px; border-bottom: 1px solid var(--line); }
.ps-eyebrow { font-size: 11px; color: var(--dim); letter-spacing: 1.6px; text-transform: uppercase; }
.ps-drawer-title { font-family: ${FONT.display}; font-size: 18px; font-weight: 700; margin-top: 3px; letter-spacing: -.3px; }
.ps-x { width: 33px; height: 33px; border-radius: 9px; border: 1px solid var(--line); background: var(--surface); color: var(--muted); cursor: pointer; display: flex; align-items: center; justify-content: center; transition: .14s; }
.ps-x:hover { color: var(--text); background: var(--surface2); }
.ps-coh { padding: 16px 22px; background: var(--bg); border-bottom: 1px solid var(--line); }
.ps-coh-top { display: flex; align-items: center; justify-content: space-between; }
.ps-coh-h { display: flex; align-items: center; gap: 7px; font-size: 12.5px; font-weight: 600; }
.ps-coh-score { font-family: ${FONT.mono}; font-weight: 600; font-size: 14px; }
.ps-coh-bar { height: 6px; border-radius: 4px; background: var(--surface2); margin: 11px 0 9px; overflow: hidden; }
.ps-coh-bar div { height: 100%; border-radius: 4px; transition: width .35s cubic-bezier(.2,.7,.3,1), background .35s; }
.ps-coh-iss { font-size: 11.5px; color: var(--amber); display: flex; flex-direction: column; gap: 2px; }
.ps-coh-ok { font-size: 11.5px; color: var(--muted); }
.ps-tabs { display: flex; gap: 4px; padding: 13px 22px 0; border-bottom: 1px solid var(--line); }
.ps-tab { text-transform: capitalize; background: transparent; border: none; color: var(--muted); font-size: 13px; font-weight: 600; padding: 9px 13px; cursor: pointer; border-bottom: 2px solid transparent; margin-bottom: -1px; transition: .14s; font-family: inherit; }
.ps-tab:hover { color: var(--text); }
.ps-tab.active { color: var(--lilac); border-bottom-color: var(--violet); }
.ps-drawer-body { flex: 1; overflow-y: auto; padding: 20px 22px; }
.ps-field { display: flex; flex-direction: column; gap: 6px; margin-bottom: 15px; flex: 1; min-width: 0; }
.ps-flabel { font-size: 11px; color: var(--muted); font-weight: 600; display: flex; align-items: center; gap: 4px; }
.ps-in { background: var(--bg); border: 1px solid var(--line); border-radius: 9px; padding: 10px 12px; color: var(--text); font-size: 13px; outline: none; width: 100%; font-family: inherit; resize: vertical; transition: .14s; }
.ps-in:focus { border-color: var(--violet); box-shadow: 0 0 0 3px color-mix(in srgb, var(--violet) 13%, transparent); }
.ps-in.mono { font-family: ${FONT.mono}; font-size: 11.5px; }
select.ps-in { cursor: pointer; }
.ps-row { display: flex; gap: 12px; }
.ps-seclabel { display: flex; align-items: center; gap: 9px; margin: 20px 0 13px; color: var(--muted); font-size: 11px; font-weight: 700; letter-spacing: .6px; text-transform: uppercase; }
.ps-seclabel svg { color: var(--violet); }
.ps-secline { flex: 1; height: 1px; background: var(--lineSoft); }
/* The "based on IP" pill that rides in a field's own label. Deliberately quiet
   when off: it is a modifier on the field beside it, not a control of its own. */
/* The count beside a tab label. Comes from the same filter that builds the
   list, so it can never disagree with the rows underneath it. */
/* A token is shown once. The block that shows it is deliberately loud, because
   scrolling past it means minting another one. */
.ps-token-new { border: 1px solid var(--violetDim); background: var(--bg); border-radius: 10px;
  padding: 12px; margin-bottom: 14px; }
.ps-token-value { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
.ps-token-value code { flex: 1; min-width: 0; overflow-x: auto; white-space: nowrap;
  font-size: 12px; color: var(--lilac); background: var(--panel); border-radius: 6px; padding: 7px 9px; }
.ps-token-table { margin-bottom: 12px; }
.ps-token-table td, .ps-token-table th { padding: 7px 8px; font-size: 12px; }
.ps-token-add { display: flex; gap: 8px; align-items: center; }
.ps-token-add .ps-in { flex: 1; min-width: 0; }
.ps-tab-n { display: inline-block; margin-left: 6px; padding: 0 5px; border-radius: 999px;
  background: var(--bg); color: var(--muted); font-size: 10px; font-weight: 700;
  font-family: var(--mono, monospace); }
.ps-tab.active .ps-tab-n { background: var(--violetDim); color: var(--lilac); }
.ps-locked { min-height: 100vh; display: grid; place-items: center; background: var(--bg-0); padding: 24px; }
.ps-locked-card { width: 100%; max-width: 420px; display: flex; flex-direction: column; gap: 12px;
  background: var(--bg-1); border: 1px solid var(--bd-1); border-radius: 14px; padding: 28px; }
.ps-locked-ic { width: 44px; height: 44px; border-radius: 12px; display: grid; place-items: center;
  background: var(--violetDim); color: var(--lilac); }
.ps-locked-card h1 { margin: 4px 0 0; font-size: 18px; font-weight: 700; color: var(--text); }
.ps-locked-card p { margin: 0; font-size: 13px; line-height: 1.6; color: var(--muted); }
.ps-locked-card code { font-size: 12px; color: var(--lilac); }
.ps-locked-err { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--red); }
.ps-locked-note { font-size: 11.5px; color: var(--dim); }
.ps-keep { display: grid; grid-template-columns: repeat(auto-fill, minmax(165px, 1fr)); gap: 6px; margin-bottom: 12px; }
.ps-keep-item { display: flex; align-items: center; gap: 8px; text-align: left; padding: 8px 10px;
  border: 1px solid var(--line); border-radius: 9px; background: var(--bg); color: var(--muted);
  font-size: 12px; font-weight: 600; font-family: inherit; cursor: pointer; transition: .14s; }
.ps-keep-item:hover { border-color: var(--muted); }
.ps-keep-item.on { color: var(--text); border-color: var(--violetDim); }
.ps-keep-box { display: grid; place-items: center; width: 15px; height: 15px; flex: none;
  border: 1px solid var(--line); border-radius: 4px; color: var(--lilac); }
.ps-keep-item.on .ps-keep-box { background: var(--violetDim); border-color: transparent; }
.ps-ipsw { display: inline-flex; align-items: center; gap: 3px; margin-left: auto; vertical-align: middle;
  border: 1px solid var(--line); background: var(--bg); color: var(--muted);
  font-size: 9.5px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase;
  padding: 1px 6px; border-radius: 999px; cursor: pointer; font-family: inherit; transition: .14s; }
.ps-ipsw:hover:not(:disabled) { color: var(--text); border-color: var(--muted); }
.ps-ipsw.on { background: var(--violetDim); border-color: transparent; color: var(--lilac); }
.ps-ipsw:disabled { opacity: .45; cursor: not-allowed; }
.ps-in:disabled, .ps-in[disabled] { opacity: .5; cursor: not-allowed; }
.ps-seg { display: flex; background: var(--bg); border: 1px solid var(--line); border-radius: 9px; padding: 2px; }
.ps-seg button { flex: 1; border: none; background: transparent; color: var(--muted); font-size: 11.5px; font-weight: 600; padding: 6px; border-radius: 7px; cursor: pointer; transition: .14s; font-family: inherit; }
.ps-seg button.on { background: var(--violetDim); color: var(--lilac); }
.ps-toggle { display: flex; align-items: center; gap: 9px; background: var(--bg); border: 1px solid var(--line); border-radius: 9px; padding: 8px 11px; cursor: pointer; }
.ps-toggle .knob { width: 32px; height: 18px; border-radius: 10px; background: var(--line); position: relative; transition: .18s; flex-shrink: 0; }
.ps-toggle .knob::after { content: ""; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--muted); transition: .18s; }
.ps-toggle.on .knob { background: var(--violet); }
.ps-toggle.on .knob::after { left: 16px; background: #fff; }
.ps-toggle em { font-style: normal; font-size: 12px; color: var(--text); }
.ps-toggle.sm { padding: 6px 10px; }
.ps-toggle.sm .knob { width: 26px; height: 15px; }
.ps-toggle.sm .knob::after { width: 11px; height: 11px; }
.ps-toggle.sm.on .knob::after { left: 13px; }
.ps-proxy-toggle { display: flex; align-items: center; justify-content: space-between; padding: 13px 15px; background: var(--bg); border: 1px solid var(--line); border-radius: 11px; margin-bottom: 16px; }
.ps-cookie-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 9px; }
.ps-probe { margin-top: 12px; border: 1px solid var(--lineSoft); border-radius: 10px; background: var(--bg); overflow: hidden; }
.ps-probe-head { display: flex; align-items: baseline; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--lineSoft); font-size: 12px; color: var(--muted); }
.ps-probe-score { font-family: ${FONT.display}; font-size: 20px; font-weight: 700; }
.ps-check { display: flex; align-items: flex-start; gap: 8px; padding: 7px 14px; font-size: 11.5px; color: var(--red); line-height: 1.5; }
.ps-check.ok { color: var(--muted); }
.ps-check svg { flex-shrink: 0; margin-top: 2px; }
.ps-check.ok svg { color: var(--mint); }
.ps-check em { font-style: normal; color: var(--dim); }
.ps-spin { animation: ps-rot 1s linear infinite; }
@keyframes ps-rot { to { transform: rotate(360deg); } }
.ps-btn.active { border-color: var(--violet); color: var(--lilac); background: color-mix(in srgb, var(--violet) 10%, transparent); }
.ps-filter-wrap { position: relative; }
.ps-filter-scrim { position: fixed; inset: 0; z-index: 20; }
.ps-filter-pop { position: absolute; right: 0; top: calc(100% + 8px); z-index: 21; width: 300px;
  background: var(--surface); border: 1px solid var(--line); border-radius: 12px;
  box-shadow: 0 16px 40px rgba(0,0,0,.45); padding: 14px; animation: ps-pop .12s ease; }
.ps-filter-head { display: flex; align-items: center; justify-content: space-between;
  font-size: 12px; font-weight: 700; color: var(--text); margin-bottom: 10px; }
.ps-filter-clear { background: none; border: none; color: var(--lilac); font-size: 11px;
  cursor: pointer; font-family: inherit; }
.ps-filter-clear:hover { text-decoration: underline; }
.ps-filter-row { margin-bottom: 12px; }
.ps-filter-label { display: block; font-size: 10.5px; text-transform: uppercase;
  letter-spacing: .06em; color: var(--muted); margin-bottom: 6px; }
.ps-filter-opts { display: flex; flex-wrap: wrap; gap: 6px; }
.ps-chip { background: var(--bg); border: 1px solid var(--line); border-radius: 8px;
  padding: 5px 10px; font-size: 12px; color: var(--muted); cursor: pointer;
  font-family: inherit; transition: .12s; }
.ps-chip:hover { border-color: #33334f; color: var(--text); }
.ps-chip.on { background: var(--violet); border-color: var(--violet); color: #fff; }
.ps-chiprow { display: flex; flex-wrap: wrap; gap: 7px; }
.ps-chip-btn { display: inline-flex; align-items: center; gap: 5px; background: var(--bg);
  border: 1px solid var(--line); border-radius: 8px; padding: 6px 11px; font-size: 12px;
  color: var(--muted); cursor: pointer; font-family: inherit; transition: .12s; }
.ps-chip-btn:hover { border-color: #33334f; color: var(--text); }
.ps-chip-btn.on { background: var(--violetDim); border-color: var(--violet); color: var(--lilac); }
.ps-pager { display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 16px 0 4px; flex-wrap: wrap; }
.ps-pager-info { font-size: 12px; color: var(--muted); margin: 0 6px; }
.ps-pager-info b { color: var(--text); font-weight: 600; }
@keyframes ps-pop { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
.ps-cookie-actions .ps-btn.disabled { opacity: .5; cursor: progress; }
.ps-hint code { font-family: ${FONT.mono}; font-size: 11px; color: var(--text); }
/* Block, not flex. A hint is a sentence with an icon in front of it, and
   under display:flex every inline <b> or <code> inside becomes its own
   flex item - the sentence renders as a row of narrow columns instead of
   a line of text. Taking the icon out of the flow with position keeps the
   prose as prose, however much markup it contains. */
.ps-hint > svg:first-child { position: absolute; left: 13px; top: 12px; }
.ps-hint { display: block; position: relative; font-size: 11.5px; color: var(--muted); margin-top: 12px; line-height: 1.55; background: var(--bg); border: 1px solid var(--lineSoft); padding: 11px 13px 11px 33px; border-radius: 10px; }
.ps-hint svg { flex-shrink: 0; margin-top: 1px; color: var(--violet); }
.ps-drawer-foot { display: flex; justify-content: flex-end; gap: 10px; padding: 17px 22px; border-top: 1px solid var(--line); }

/* ---- shared view header (Folders, Settings) ---- */
.ps-view-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 20px; flex-wrap: wrap; margin: 6px 0 20px; }
.ps-view-title { font-family: ${FONT.display}; font-size: 22px; font-weight: 700; margin: 0; letter-spacing: -.3px; }
.ps-view-sub { color: var(--muted); font-size: 12.5px; line-height: 1.55; margin: 5px 0 0; max-width: 520px; }
.ps-in.sm { padding: 7px 10px; font-size: 12.5px; }
.ps-btn.danger { background: color-mix(in srgb, var(--red) 12%, transparent); border-color: color-mix(in srgb, var(--red) 40%, transparent); color: var(--red); }
.ps-btn.danger:hover { background: color-mix(in srgb, var(--red) 18%, transparent); }
.ps-iconbtn.sm { width: 30px; height: 30px; border-radius: 8px; }
/* A credential cell: masked by default, revealed on demand. This table is the
   kind of thing people screen-share, so a password should take one deliberate
   click to show rather than being on display by default. */
.ps-iconbtn.xs { width: 22px; height: 22px; border-radius: 6px; border-color: transparent; background: transparent; }
.ps-secret { display: inline-flex; align-items: center; gap: 4px; }
.ps-secret code { letter-spacing: .5px; }
.ps-iconbtn.sm.danger:hover { color: var(--red); border-color: var(--red); }
.ps-seg.wrap { flex-wrap: wrap; }
.ps-seg.wrap button { flex: 0 0 auto; padding: 7px 12px; }

/* ---- Folders manager ---- */
.ps-newfold { display: flex; align-items: center; gap: 8px; background: var(--surface); border: 1px solid var(--line); border-radius: 11px; padding: 6px 8px 6px 12px; }
.ps-newfold input { border: none; background: transparent; min-width: 180px; }
.ps-newfold input:focus { outline: none; }
.ps-fold-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 14px; }
.ps-fold-card { background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 15px 16px; display: flex; flex-direction: column; gap: 11px; transition: border-color .15s, transform .15s; }
.ps-fold-card:hover { border-color: color-mix(in srgb, var(--violet) 40%, transparent); transform: translateY(-2px); }
.ps-fold-top { display: flex; align-items: center; gap: 10px; }
.ps-fold-ic { width: 32px; height: 32px; border-radius: 9px; background: var(--violetDim); display: flex; align-items: center; justify-content: center; flex: none; }
.ps-fold-name { background: none; border: none; color: var(--text); font-family: ${FONT.display}; font-size: 15px; font-weight: 650; cursor: pointer; padding: 0; text-align: left; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ps-fold-name:hover { color: var(--lilac); }
.ps-fold-stats { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; font-size: 11.5px; color: var(--muted); }
.ps-fold-stats b { color: var(--text); font-weight: 700; }
.ps-fold-run, .ps-fold-coh { display: inline-flex; align-items: center; gap: 4px; }
.ps-fold-coh svg { color: var(--mint); }
.ps-fold-actions { display: flex; align-items: center; gap: 6px; border-top: 1px solid var(--lineSoft); padding-top: 11px; }

/* ---- Settings ---- */
.ps-set-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 16px; align-items: start; }
.ps-set-card { background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 18px 18px 16px; }
.ps-set-h { display: flex; align-items: center; gap: 8px; font-family: ${FONT.display}; font-size: 14px; font-weight: 650; margin-bottom: 14px; }
.ps-set-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.ps-set-label { font-size: 13px; font-weight: 600; color: var(--text); }
.ps-set-note { font-size: 11.5px; color: var(--muted); line-height: 1.55; }
.ps-set-note code { font-family: ${FONT.mono}; font-size: 11px; color: var(--text); }
.ps-set-missing { font-size: 10px; color: var(--amber); font-weight: 600; }
.ps-set-counts { display: flex; gap: 16px; flex-wrap: wrap; margin-top: 14px; padding-top: 13px; border-top: 1px solid var(--lineSoft); font-size: 12px; color: var(--muted); }
.ps-set-counts b { color: var(--text); font-weight: 700; }
.ps-folders-manage { background: none; border: none; color: var(--violet); font-size: 10.5px; font-weight: 600; cursor: pointer; font-family: inherit; padding: 0; }
.ps-folders-manage:hover { color: var(--lilac); text-decoration: underline; }

@media (max-width: 860px) {
  .ps-side { position: fixed; z-index: 40; transform: translateX(-100%); }
  .ps-search { max-width: 40vw; }
  .ps-row { flex-direction: column; gap: 0; }
  .ps-eng-title { font-size: 24px; }
}
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: .001ms !important; animation-iteration-count: 1 !important; transition-duration: .001ms !important; }
}
`;
