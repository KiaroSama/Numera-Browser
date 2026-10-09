use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Seen = Arc<Mutex<Vec<(String, BTreeMap<String, String>)>>>;
pub struct HintSite {
    listener: TcpListener,
    seen: Seen,
    stop: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
fn serve(mut stream: TcpStream, seen: &Seen) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..count]);
        if request.len() > 16_384 {
            return Err(io::Error::other("Hint fixture request is too large"));
        }
    }
    let request = std::str::from_utf8(&request).map_err(io::Error::other)?;
    let mut lines = request.split("\r\n");
    let path = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| io::Error::other("Hint fixture request missing"))?
        .to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(key, value)| {
            let key = key.to_ascii_lowercase();
            key.starts_with("sec-ch-ua")
                .then_some((key, value.trim().to_owned()))
        })
        .collect();
    seen.lock()
        .map_err(|_| io::Error::other("Hint fixture state poisoned"))?
        .push((path, headers));
    let body = "<!doctype html><meta charset=utf-8><title>hint fixture</title>";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nAccept-CH: Sec-CH-UA-Platform-Version, Sec-CH-UA-Arch, Sec-CH-UA-Bitness, Sec-CH-UA-Model, Sec-CH-UA-Full-Version, Sec-CH-UA-Full-Version-List, Sec-CH-UA-WoW64\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()
}
impl HintSite {
    pub fn new() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let accepting = listener.try_clone()?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let worker_seen = seen.clone();
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                if *worker_stop
                    .0
                    .lock()
                    .map_err(|_| io::Error::other("Hint fixture stop poisoned"))?
                {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Hint fixture wall deadline",
                    ));
                }
                match accepting.accept() {
                    Ok((stream, _)) => serve(stream, &worker_seen)?,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        let stopped = worker_stop
                            .0
                            .lock()
                            .map_err(|_| io::Error::other("Hint fixture stop poisoned"))?;
                        let _ = worker_stop
                            .1
                            .wait_timeout(stopped, Duration::from_millis(10))
                            .map_err(|_| io::Error::other("Hint fixture stop poisoned"))?;
                    }
                    Err(error) => return Err(error),
                }
            }
        });
        Ok(Self {
            listener,
            seen,
            stop,
            worker: Some(worker),
        })
    }
    pub fn url(&self, path: &str) -> io::Result<String> {
        Ok(format!("http://{}{}", self.listener.local_addr()?, path))
    }
    pub fn headers(&self, path: &str) -> io::Result<BTreeMap<String, String>> {
        self.seen
            .lock()
            .map_err(|_| io::Error::other("Hint fixture state poisoned"))?
            .iter()
            .find(|(key, _)| key == path)
            .map(|(_, headers)| headers.clone())
            .ok_or_else(|| io::Error::other("Expected hint fixture request was not received"))
    }
    pub fn finish(&mut self) -> io::Result<()> {
        *self
            .stop
            .0
            .lock()
            .map_err(|_| io::Error::other("Hint fixture stop poisoned"))? = true;
        self.stop.1.notify_all();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("Hint fixture worker failed"))??;
        }
        Ok(())
    }
}
impl Drop for HintSite {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("Hint fixture cleanup failed: {error}");
        }
    }
}
