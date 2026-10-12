() => {
  const raw = document.body ? document.body.innerText : '';
  let d;
  try { d = JSON.parse(raw); } catch (e) { return null; }
  const tls = d.tls || d;
  const ja3 = tls.ja3 || d.ja3 || d.fingerprint || null;
  const ja3Hash = tls.ja3_hash || d.ja3_hash || d.hash || d.ja3_digest || null;
  const ja4 = tls.ja4 || d.ja4 || (d.ja4_r ? String(d.ja4_r).split('_')[0] : null);

  // Cipher count from the ja3 string's 2nd field (GREASE is stripped there per
  // spec, so this is the "real" suite count).
  let cipherCount = 0;
  if (ja3 && ja3.includes(',')) {
    cipherCount = (ja3.split(',')[1] || '').split('-').filter(Boolean).length;
  } else if (tls.ciphers) {
    cipherCount = tls.ciphers.length;
  }

  // GREASE (RFC 8701) is stripped from ja3/ja4, so look at the raw response:
  // services that list ciphers name it ("TLS_GREASE…") or show a 0x?a?a value.
  // greaseKnown tells us whether the service exposed ciphers at all — if it
  // only gave a ja3 string, absence of GREASE means nothing.
  const greaseKnown = /cipher/i.test(raw) || /grease/i.test(raw);
  const grease = /grease/i.test(raw) || /0x[0-9a-f]a[0-9a-f]a/i.test(raw);

  return {
    ja3, ja3Hash, ja4,
    http2: (d.http2 && (d.http2.akamai_fingerprint_hash || d.http2.akamai_fingerprint)) || null,
    grease, greaseKnown, cipherCount,
  };
}
