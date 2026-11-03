async ({idUrl, subTmpl, resultTmpl, n}) => {
  let id;
  try { id = (await (await fetch(idUrl, {cache:'no-store'})).text()).trim(); }
  catch (e) { return {error: 'could not reach the DNS test service'}; }
  if (!id) return {error: 'no test id issued'};
  // Fire N lookups at unique subdomains. We don't care about the responses
  // (no-cors, failures ignored) — the point is to make the browser *resolve*
  // each name so the service can see which resolver did it.
  const jobs = [];
  for (let i = 1; i <= n; i++) {
    const u = subTmpl.replace('{i}', i).replace('{id}', id);
    jobs.push(fetch(u, {mode:'no-cors', cache:'no-store'}).catch(() => {}));
  }
  await Promise.all(jobs);
  try {
    const rows = await (await fetch(resultTmpl.replace('{id}', id), {cache:'no-store'})).json();
    return {id, rows};
  } catch (e) { return {error: 'could not read the DNS result', id}; }
}
