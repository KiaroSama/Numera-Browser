() => new Promise((resolve) => {
  const found = new Set();
  let pc;
  try { pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] }); }
  catch (e) { return resolve({ supported: false, ips: [] }); }
  const done = () => { try { pc.close(); } catch (e) {} resolve({ supported: true, ips: [...found] }); };
  pc.onicecandidate = (e) => {
    if (!e.candidate) return done();
    const m = /([0-9]{1,3}(\.[0-9]{1,3}){3}|[a-f0-9]{1,4}(:[a-f0-9]{0,4}){2,7})/i
      .exec(e.candidate.candidate || '');
    if (m && !m[0].endsWith('.local')) found.add(m[0]);
  };
  try { pc.createDataChannel('probe'); pc.createOffer().then((o) => pc.setLocalDescription(o)); }
  catch (e) { return done(); }
  setTimeout(done, 3000);
})
