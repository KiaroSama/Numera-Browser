use numera_core::{network::relay::Relay, ownership::capture, proxy::Proxy};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const REQUEST: &[u8] = b"GET http://destination.invalid:8080/wire?x=1 HTTP/1.1\r\nHost: destination.invalid:8080\r\nConnection: close\r\n\r\n";
const ORIGIN: &[u8] =
    b"GET /wire?x=1 HTTP/1.1\r\nHost: destination.invalid:8080\r\nConnection: close\r\n\r\n";
const RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x00\xffok";

struct Peer {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<io::Result<Vec<u8>>>>,
}
impl Peer {
    fn start(listener: &TcpListener, case: &Value) -> Result<Self, Box<dyn std::error::Error>> {
        let events = case["events"]
            .as_array()
            .ok_or("Handshake events missing")?
            .clone();
        let refused = !case["error"].is_null();
        let listener = listener.try_clone()?;
        let (stop, stopped) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error),
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::other("Handshake fixture accept deadline"));
                }
                if !matches!(
                    stopped.recv_timeout(Duration::from_millis(2)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    return Err(io::Error::other("Handshake fixture cancelled"));
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_nodelay(true)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut wire = Vec::new();
            for event in events {
                if Instant::now() >= deadline {
                    return Err(io::Error::other("Handshake transcript deadline"));
                }
                if let Some(sent) = event.get("send") {
                    let expected: Vec<u8> =
                        serde_json::from_value(sent.clone()).map_err(io::Error::other)?;
                    let mut received = vec![0; expected.len()];
                    stream.read_exact(&mut received)?;
                    if received != expected {
                        return Err(io::Error::other("SOCKS handshake transcript bytes differ"));
                    }
                    wire.extend_from_slice(&received);
                } else {
                    let bytes: Vec<u8> =
                        serde_json::from_value(event["recv"].clone()).map_err(io::Error::other)?;
                    if bytes.is_empty() {
                        stream.shutdown(Shutdown::Write)?;
                    } else {
                        stream.write_all(&bytes)?;
                    }
                }
            }
            if refused {
                if stream.read(&mut [0; 1])? != 0 {
                    return Err(io::Error::other(
                        "Refused SOCKS handshake forwarded request bytes",
                    ));
                }
            } else {
                let mut request = vec![0; ORIGIN.len()];
                stream.read_exact(&mut request)?;
                if request != ORIGIN {
                    return Err(io::Error::other(
                        "SOCKS handshake lost origin request bytes",
                    ));
                }
                stream.write_all(RESPONSE)?;
            }
            Ok(wire)
        });
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
    fn finish(&mut self) -> io::Result<Vec<u8>> {
        self.worker
            .take()
            .ok_or_else(|| io::Error::other("Handshake fixture already joined"))?
            .join()
            .map_err(|_| io::Error::other("Handshake fixture panicked"))?
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if self.worker.is_some()
            && let Err(error) = self.finish()
        {
            eprintln!("Handshake fixture cleanup failed: {error}");
        }
    }
}

#[test]
fn socks_handshake_failures_preserve_original_diagnostics_and_wire()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("relay-handshake-original")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.relay import _socks4_connect,_socks5_connect
from run_logging import run_log
class Socket:
 def __init__(self,reply):self.reply,self.events=reply,[]
 def sendall(self,data):self.events.append({'send':list(data)})
 def recv(self,count):
  part,self.reply=self.reply[:min(count,1)],self.reply[min(count,1):]
  self.events.append({'recv':list(part)})
  return part
cases=[]
def add(name,kind,reply,user=None,password=None):
 sock=Socket(reply)
 error=None
 try:
  if kind=='socks4':_socks4_connect(sock,'destination.invalid',8080,user)
  else:_socks5_connect(sock,'destination.invalid',8080,user,password)
 except Exception as failure:error=str(failure)
 cases.append({'name':name,'kind':kind,'user':user,'password':password,'events':sock.events,'error':error})
bound=b'\x00\x5a'+b'\x00'*6
for length in range(8):add('socks4-eof-'+str(length),'socks4',bound[:length],'tenant')
for code in (90,91,92,93,94):add('socks4-reply-'+str(code),'socks4',bytes([0,code])+b'\x00'*6,'tenant')
add('socks4-version','socks4',b'\x04\x5a'+b'\x00'*6)
for user in (None,'','tenant','é','u'*255,'u'*256):add('socks4-user-'+str(len(cases)),'socks4',bound,user)
reply=b'\x05\x00\x05\x00\x00\x01'+b'\x00'*6
for length in range(len(reply)):add('socks5-eof-'+str(length),'socks5',reply[:length])
for atyp,address in ((1,b'\x00'*4),(4,b'\x00'*16),(3,b'\x04name')):
 add('socks5-address-'+str(atyp),'socks5',bytes([5,0,5,0,0,atyp])+address+b'\x00\x00')
for code in range(1,10):add('socks5-refusal-'+str(code),'socks5',bytes([5,0,5,code,0,1]))
for version,method in ((4,0),(5,255),(5,1),(5,3)):add('socks5-method-'+str(method),'socks5',bytes([version,method]))
for user,password in ((None,None),('',''),('tenant','synthetic'),('é','密'),('u'*255,'p'*255),('u'*256,''),('u','p'*256)):
 add('socks5-auth-'+str(len(cases)),'socks5',b'\x05\x02\x01\x00'+reply[2:],user,password)
for suffix in (b'',b'\x01',b'\x01\x01'):
 add('socks5-auth-refusal-'+str(len(cases)),'socks5',b'\x05\x02'+suffix,'tenant','synthetic')
for atyp,address in ((4,b'\x00'*15),(3,b''),(3,b'\x04nam'),(3,b'\x04name\x00')):
 add('socks5-bound-eof-'+str(len(cases)),'socks5',bytes([5,0,5,0,0,atyp])+address)
add('socks5-unknown-address','socks5',b'\x05\x00\x05\x00\x00\x09')
with run_log('relay-handshake-oracle',parent/'logs'):
 print(json.dumps(cases,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path());
    let observed = match capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(20),
            idle: Duration::from_secs(15),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    ) {
        Ok(observed) => observed,
        Err(mut failure) => {
            if failure.settle(Duration::from_secs(5)).is_err() {
                std::mem::forget(temporary);
            }
            return Err(failure.into());
        }
    };
    assert_eq!(observed.exit, 0, "Original SOCKS handshake observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 63, "Handshake observation coverage changed");
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let upstream = listener.local_addr()?;
    let mut relays: BTreeMap<_, (Relay, Option<String>)> = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    for case in cases {
        assert!(
            Instant::now() < deadline,
            "Handshake corpus deadline exceeded"
        );
        let kind = case["kind"].as_str().ok_or("Kind missing")?;
        let key = (
            kind.to_owned(),
            case["user"].as_str().map(str::to_owned),
            case["password"].as_str().map(str::to_owned),
        );
        let (relay, last_error) = match relays.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let proxy = Proxy::from_record(
                    &json!({
                        "server":format!("{kind}://{upstream}"),
                        "username":case["user"],"password":case["password"],
                    }),
                    false,
                )?;
                entry.insert((Relay::start(&proxy)?, None::<String>))
            }
        };
        // The original relay retains its last failure across later successful requests.
        if let Some(error) = case["error"].as_str() {
            *last_error = Some(error.to_owned());
        }
        let mut peer = Peer::start(&listener, &case)?;
        let url = reqwest::Url::parse(&relay.url())?;
        let address = ("127.0.0.1", url.port().ok_or("Relay port missing")?);
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut client = TcpStream::connect(address)?;
            client.set_read_timeout(Some(Duration::from_secs(3)))?;
            client.set_write_timeout(Some(Duration::from_secs(2)))?;
            client.write_all(REQUEST)?;
            let mut response = Vec::new();
            client.take(8192).read_to_end(&mut response)?;
            if case["error"].is_null() {
                assert_eq!(
                    response, RESPONSE,
                    "Handshake response differs: {}",
                    case["name"]
                );
            } else {
                assert_eq!(response, b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 40\r\nConnection: close\r\n\r\nrelay could not reach the upstream proxy");
            }
            assert_eq!(
                relay.last_error().as_deref(),
                last_error.as_deref(),
                "SOCKS handshake diagnostic differs: {}",
                case["name"]
            );
            Ok(())
        })();
        if result.is_err() {
            relay.close(Duration::from_secs(2))?;
        }
        let wire = peer.finish();
        result?;
        let expected: Vec<u8> = case["events"]
            .as_array()
            .ok_or("Events missing")?
            .iter()
            .filter_map(|event| event.get("send"))
            .map(|bytes| serde_json::from_value::<Vec<u8>>(bytes.clone()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(wire?, expected, "SOCKS complete handshake wire differs");
    }
    for (relay, _) in relays.values_mut() {
        let url = reqwest::Url::parse(&relay.url())?;
        let address = ("127.0.0.1", url.port().ok_or("Relay port missing")?);
        relay.close(Duration::from_secs(2))?;
        assert!(
            TcpStream::connect(address).is_err(),
            "Closed relay listener survived"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
