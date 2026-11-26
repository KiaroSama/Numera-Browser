//! Explicit test connection scope. Policy sees an allocated owned socket before
//! any connect syscall; ordinary RAII closes a denied allocation.
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io,
    net::{SocketAddr, TcpStream},
    time::Duration,
};
pub fn connect(
    address: SocketAddr,
    budget: Duration,
    policy: impl FnOnce(&Socket, SocketAddr) -> io::Result<()>,
) -> io::Result<TcpStream> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    policy(&socket, address)?;
    socket.connect_timeout(&address.into(), budget)?;
    Ok(socket.into())
}
#[cfg(unix)]
pub fn raw(socket: &Socket) -> i32 {
    use std::os::fd::AsRawFd;
    socket.as_raw_fd()
}
#[cfg(windows)]
pub fn raw(socket: &Socket) -> usize {
    use std::os::windows::io::AsRawSocket;
    socket.as_raw_socket() as usize
}
#[cfg(unix)]
#[allow(unsafe_code)]
pub fn closed(fd: i32) -> bool {
    // SAFETY: fcntl accepts a scalar descriptor, including an invalid one.
    unsafe {
        libc::fcntl(fd, libc::F_GETFD) == -1
            && io::Error::last_os_error().raw_os_error() == Some(libc::EBADF)
    }
}
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn closed(socket: usize) -> bool {
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn getsockopt(socket: usize, level: i32, name: i32, value: *mut u8, size: *mut i32) -> i32;
        fn WSAGetLastError() -> i32;
    }
    let mut value = 0i32;
    let mut size = std::mem::size_of_val(&value) as i32;
    // SAFETY: valid scalar outputs; Winsock defines invalid-socket failure.
    unsafe {
        getsockopt(
            socket,
            0xffff,
            0x1008,
            (&mut value as *mut i32).cast(),
            &mut size,
        ) == -1
            && WSAGetLastError() == 10038
    }
}
