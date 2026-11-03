(fonts) => {
  const out = [];
  try {
    const host = document.body || document.documentElement;
    const span = document.createElement('span');
    span.style.cssText = 'position:absolute;left:-9999px;top:-9999px;font-size:72px;white-space:nowrap;';
    span.textContent = 'mmmmmmmmmmlli WwMiI0129@';
    host.appendChild(span);
    const bases = {};
    for (const g of ['monospace', 'serif', 'sans-serif']) { span.style.fontFamily = g; bases[g] = span.offsetWidth; }
    for (const f of fonts) {
      for (const g of ['monospace', 'serif', 'sans-serif']) {
        span.style.fontFamily = '"' + f + '",' + g;
        if (span.offsetWidth !== bases[g]) { out.push(f); break; }
      }
    }
    host.removeChild(span);
  } catch (e) {}
  return out;
}
