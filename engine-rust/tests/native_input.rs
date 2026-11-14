use numera_core::native_input::Input;
use std::{
    io::{self, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[test]
fn redirected_input_preserves_utf8_split_frames_eof_and_cancellation() -> io::Result<()> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let (reader, mut writer) = io::pipe()?;
    let mut input = Input::new(reader, cancelled.clone());
    writer.write_all("α\nsecond".as_bytes())?;
    drop(writer);
    let mut bytes = Vec::new();
    let mut part = [0u8; 2];
    loop {
        let count = input.read(&mut part)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&part[..count]);
    }
    assert_eq!(bytes, "α\nsecond".as_bytes());
    let (reader, writer) = io::pipe()?;
    let mut input = Input::new(reader, cancelled.clone());
    let start = Instant::now();
    std::thread::scope(|scope| {
        let (ready, observed) = std::sync::mpsc::sync_channel(1);
        let task = scope.spawn(move || {
            ready.send(()).expect("Read owner handshake failed");
            input.read(&mut part)
        });
        observed
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("Read owner did not start");
        cancelled.store(true, Ordering::Release);
        assert_eq!(
            task.join()
                .expect("Read owner panicked")
                .expect_err("Cancelled empty pipe blocked")
                .kind(),
            io::ErrorKind::ConnectionAborted
        );
    });
    assert!(start.elapsed().as_secs_f64() < 1.0);
    drop(writer);
    #[cfg(windows)]
    {
        let null = std::fs::File::open("NUL")?;
        let mut input = Input::new(null, Arc::new(AtomicBool::new(false)));
        assert_eq!(
            input.read(&mut part)?,
            0,
            "NUL did not behave as empty stdin"
        );
    }
    Ok(())
}
