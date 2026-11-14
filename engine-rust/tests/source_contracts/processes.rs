use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
};
pub(super) struct Wire {
    reader: BufReader<TcpStream>,
}
impl Wire {
    fn blocking(stream: TcpStream) -> Self {
        // Darwin accepts inherit O_NONBLOCK from this bounded listener.
        // The line protocol uses blocking reads with its own finite timeout.
        stream
            .set_nonblocking(false)
            .expect("blocking event stream");
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .expect("bounded event wait");
        Self {
            reader: BufReader::new(stream),
        }
    }
    pub fn say(&mut self, event: &str) {
        writeln!(self.reader.get_mut(), "{event}").expect("send event");
        self.reader.get_mut().flush().expect("flush event");
    }
    pub fn wait(&mut self) {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("wait release");
        assert_eq!(line.trim(), "release");
    }
    fn expect(&mut self, event: &str) {
        let mut line = String::new();
        assert!(
            self.reader.read_line(&mut line).expect("read event") > 0,
            "child exited before {event}"
        );
        assert_eq!(line.trim(), event);
    }
    fn quiet(&mut self) {
        use std::io::Read;
        self.reader
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(200)))
            .expect("short timeout");
        let mut b = [0u8];
        let r = self.reader.read(&mut b);
        assert!(
            r.is_err_and(
                |e| [std::io::ErrorKind::TimedOut, std::io::ErrorKind::WouldBlock]
                    .contains(&e.kind())
            ),
            "operation completed inside another process's transaction"
        );
        self.reader
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(15)))
            .expect("restore timeout");
    }
    pub fn cloned(&self) -> Self {
        Self {
            reader: BufReader::new(
                self.reader
                    .get_ref()
                    .try_clone()
                    .expect("event stream clone"),
            ),
        }
    }
}
#[cfg(unix)]
#[allow(unsafe_code)]
fn nonblocking(stream: &TcpStream) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: the stream retains this live descriptor; F_GETFL accepts only
    // the scalar descriptor/command and has no pointer or vararg output.
    let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "event socket flags unavailable");
    flags & libc::O_NONBLOCK != 0
}
pub(super) struct Peers {
    wire: BTreeMap<String, Wire>,
    children: Vec<Child>,
}
impl Peers {
    pub fn expect(&mut self, role: &str, event: &str) {
        self.wire.get_mut(role).expect("role").expect(event);
    }
    pub fn release(&mut self, role: &str) {
        self.wire.get_mut(role).expect("role").say("release");
    }
    pub fn quiet(&mut self, role: &str) {
        self.wire.get_mut(role).expect("role").quiet();
    }
    fn finish(&mut self) {
        for child in self.children.drain(..) {
            let result = child.wait_with_output().expect("child exit");
            assert!(
                result.status.success(),
                "native child failed: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
impl Drop for Peers {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
pub(super) fn processes(
    key: &str,
    setup: impl FnOnce(&Path),
    child: impl FnOnce(&Path, &str, &mut Wire),
    parent: impl FnOnce(&Path, &mut Peers),
) {
    if std::env::var("NUMERA_SOURCE_CONTRACT_CASE").ok().as_deref() == Some(key) {
        let root =
            PathBuf::from(std::env::var_os("NUMERA_SOURCE_CONTRACT_ROOT").expect("child root"));
        let role = std::env::var("NUMERA_SOURCE_CONTRACT_ROLE").expect("child role");
        let port = std::env::var("NUMERA_SOURCE_CONTRACT_PORT").expect("child port");
        let stream =
            TcpStream::connect(format!("127.0.0.1:{port}")).expect("native parent rendezvous");
        let mut wire = Wire::blocking(stream);
        wire.say(&role);
        child(&root, &role, &mut wire);
        return;
    }
    let tmp = common::TempDirectory::new("contract-native-processes").expect("root");
    setup(tmp.path());
    let listener = TcpListener::bind("127.0.0.1:0").expect("rendezvous");
    listener.set_nonblocking(true).expect("bounded accept");
    let port = listener.local_addr().expect("port").port();
    let current = thread::current();
    let test = current.name().expect("reported test name");
    let mut peers = Peers {
        wire: BTreeMap::new(),
        children: Vec::new(),
    };
    for role in ["holder", "later"] {
        peers.children.push(
            Command::new(std::env::current_exe().expect("test executable"))
                .args(["--exact", test, "--nocapture", "--test-threads=1"])
                .env("NUMERA_SOURCE_CONTRACT_CASE", key)
                .env("NUMERA_SOURCE_CONTRACT_ROOT", tmp.path())
                .env("NUMERA_SOURCE_CONTRACT_ROLE", role)
                .env("NUMERA_SOURCE_CONTRACT_PORT", port.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("native child"),
        );
    }
    let limit = std::time::Instant::now() + Duration::from_secs(15);
    while peers.wire.len() < 2 {
        match listener.accept() {
            Ok((stream, _)) => {
                #[cfg(unix)]
                if key == "batch-rollback-race" {
                    // Replay Darwin's inherited flag on every Unix host and
                    // prove the actual kernel state changes before any read.
                    stream.set_nonblocking(true).expect("inherited socket flag");
                    assert!(nonblocking(&stream));
                }
                let mut wire = Wire::blocking(stream);
                #[cfg(unix)]
                if key == "batch-rollback-race" {
                    assert!(!nonblocking(wire.reader.get_ref()));
                }
                let mut role = String::new();
                wire.reader.read_line(&mut role).expect("child handshake");
                peers.wire.insert(role.trim().to_owned(), wire);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < limit,
                    "native child failed to connect"
                );
                thread::sleep(Duration::from_millis(2));
            }
            Err(e) => panic!("rendezvous: {e}"),
        }
    }
    parent(tmp.path(), &mut peers);
    peers.finish();
    tmp.cleanup().expect("owned native process root cleanup");
}
pub(super) fn seed(root: &Path) {
    let p = made(root, json!({"name":"a"}));
    fs::write(root.join("seed.json"), encode(&p)).expect("seed coordinates");
}
pub(super) fn seeded(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("seed.json")).expect("seed bytes"))
        .expect("seed JSON")
}
pub(super) fn blocking_interleave(peers: &mut Peers) {
    peers.expect("holder", "inside");
    peers.release("later");
    peers.expect("later", "trying");
    peers.quiet("later");
    peers.release("holder");
    peers.expect("holder", "done");
    peers.expect("later", "done");
}
impl Peers {
    pub fn next(&mut self, role: &str) -> String {
        let mut line = String::new();
        assert!(
            self.wire
                .get_mut(role)
                .expect("role")
                .reader
                .read_line(&mut line)
                .expect("next event")
                > 0
        );
        line.trim().to_owned()
    }
}
