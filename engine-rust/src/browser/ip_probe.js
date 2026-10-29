async (urls) => {
  for (const url of urls) {
    try {
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) continue;
      const text = (await res.text()).trim();
      let data;
      try { data = JSON.parse(text); } catch { data = { ip: text }; }
      const ip = data.ip || data.query || data.IPv4;
      if (!ip) continue;
      return {
        source: url,
        ip,
        country: data.country_code || data.country || data.countryCode || null,
        city: data.city || null,
        timezone: data.timezone?.id || data.timezone || null,
      };
    } catch (e) { /* try the next one */ }
  }
  return null;
}
