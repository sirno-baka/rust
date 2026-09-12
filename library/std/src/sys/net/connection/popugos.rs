use super::each_addr;
use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::vec::Vec;
use crate::net::{
    Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs,
};
use crate::sys::pal::abi;
use crate::sys::{cvt, unsupported};
use crate::time::Duration;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicU16, Ordering};

const AF_INET: u32 = 2;
const SOCK_STREAM: u32 = 1;
const SOCK_DGRAM: u32 = 2;
const IPPROTO_TCP: u32 = 6;
const IPPROTO_UDP: u32 = 17;

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrIn {
    family: u16,
    port: u16,
    addr: u32,
    zero: [u8; 8],
}

impl SockAddrIn {
    fn from_v4(addr: &SocketAddrV4) -> Self {
        Self {
            family: AF_INET as u16,
            port: addr.port().to_be(),
            // Felix's sockaddr ABI stores the IPv4 value as a big-endian
            // numeric u32 (the same convention used by smoltcp::Ipv4Address::from_bits).
            addr: u32::from_be_bytes(addr.ip().octets()),
            zero: [0; 8],
        }
    }

    fn to_std(self) -> SocketAddr {
        let ip = Ipv4Addr::from(self.addr.to_be_bytes());
        SocketAddr::V4(SocketAddrV4::new(ip, u16::from_be(self.port)))
    }
}

fn with_sockaddr<T>(addr: &SocketAddr, f: impl FnOnce(*const u8, u32) -> io::Result<T>) -> io::Result<T> {
    match addr {
        SocketAddr::V4(addr) => {
            let raw = SockAddrIn::from_v4(addr);
            f(core::ptr::from_ref(&raw).cast(), size_of::<SockAddrIn>() as u32)
        }
        SocketAddr::V6(_) => unsupported(),
    }
}

fn read_socket_addr(
    fd: i32,
    call: unsafe fn(i32, *mut u8, *mut u32) -> i32,
) -> io::Result<SocketAddr> {
    let mut raw = SockAddrIn {
        family: 0,
        port: 0,
        addr: 0,
        zero: [0; 8],
    };
    let mut len = size_of::<SockAddrIn>() as u32;
    cvt(unsafe { call(fd, core::ptr::from_mut(&mut raw).cast(), &mut len) })?;
    if len < 8 || raw.family as u32 != AF_INET {
        return Err(io::Error::from_raw_os_error(97));
    }
    Ok(raw.to_std())
}

struct Socket {
    fd: i32,
}

impl Socket {
    fn new(ty: u32, protocol: u32) -> io::Result<Self> {
        let fd = cvt(unsafe { abi::socket(AF_INET, ty, protocol) })?;
        Ok(Self { fd })
    }

    pub(crate) unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { fd }
    }

    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    pub(crate) fn into_raw_fd(self) -> i32 {
        let socket = ManuallyDrop::new(self);
        socket.fd
    }

    fn duplicate(&self) -> io::Result<Self> {
        cvt(unsafe { abi::duplicate(self.fd) }).map(|fd| unsafe { Self::from_raw_fd(fd) })
    }

    fn set_nonblocking(&self, enabled: bool) -> io::Result<()> {
        let flags = cvt(unsafe { abi::fcntl(self.fd, abi::F_GETFL, 0) })? as u32;
        let next = if enabled {
            flags | abi::O_NONBLOCK
        } else {
            flags & !abi::O_NONBLOCK
        };
        cvt(unsafe { abi::fcntl(self.fd, abi::F_SETFL, next) }).map(drop)
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = unsafe { abi::close(self.fd) };
    }
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Socket").field("fd", &self.fd).finish()
    }
}

pub struct TcpStream {
    inner: Socket,
}

impl TcpStream {
    pub(crate) unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { inner: unsafe { Socket::from_raw_fd(fd) } }
    }

    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.inner.as_raw_fd()
    }

    pub(crate) fn into_raw_fd(self) -> i32 {
        self.inner.into_raw_fd()
    }

    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        each_addr(addr, |addr| {
            let socket = Socket::new(SOCK_STREAM, IPPROTO_TCP)?;
            with_sockaddr(addr, |raw, len| {
                cvt(unsafe { abi::connect(socket.fd, raw, len) }).map(drop)
            })?;
            Ok(TcpStream { inner: socket })
        })
    }

    pub fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> io::Result<TcpStream> {
        if timeout == Duration::ZERO {
            return Err(io::Error::ZERO_TIMEOUT);
        }
        // Felix currently has no socket timeout/poll-connect ABI exposed here.
        let _ = addr;
        unsupported()
    }

    pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> { unsupported() }
    pub fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> { unsupported() }
    pub fn read_timeout(&self) -> io::Result<Option<Duration>> { unsupported() }
    pub fn write_timeout(&self) -> io::Result<Option<Duration>> { unsupported() }
    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> { unsupported() }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        cvt(unsafe { abi::recv(self.inner.fd, buf.as_mut_ptr(), buf.len()) }).map(|n| n as usize)
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::default_read_vectored(|buf| self.read(buf), bufs)
    }

    pub fn is_read_vectored(&self) -> bool { false }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::send(self.inner.fd, buf.as_ptr(), buf.len()) }).map(|n| n as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::default_write_vectored(|buf| self.write(buf), bufs)
    }

    pub fn is_write_vectored(&self) -> bool { false }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        read_socket_addr(self.inner.fd, abi::getpeername)
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_socket_addr(self.inner.fd, abi::getsockname)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => 0,
            Shutdown::Write => 1,
            Shutdown::Both => 2,
        };
        cvt(unsafe { abi::shutdown(self.inner.fd, how) }).map(drop)
    }

    pub fn duplicate(&self) -> io::Result<TcpStream> {
        self.inner.duplicate().map(|inner| TcpStream { inner })
    }

    pub fn set_linger(&self, _: Option<Duration>) -> io::Result<()> { unsupported() }
    pub fn linger(&self) -> io::Result<Option<Duration>> { unsupported() }
    pub fn set_keepalive(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn keepalive(&self) -> io::Result<bool> { unsupported() }
    pub fn set_nodelay(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn nodelay(&self) -> io::Result<bool> { unsupported() }
    pub fn set_ttl(&self, _: u32) -> io::Result<()> { unsupported() }
    pub fn ttl(&self) -> io::Result<u32> { unsupported() }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpStream").field("inner", &self.inner).finish()
    }
}

pub struct TcpListener {
    inner: Socket,
}

impl TcpListener {
    pub(crate) unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { inner: unsafe { Socket::from_raw_fd(fd) } }
    }

    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.inner.as_raw_fd()
    }

    pub(crate) fn into_raw_fd(self) -> i32 {
        self.inner.into_raw_fd()
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        each_addr(addr, |addr| {
            let socket = Socket::new(SOCK_STREAM, IPPROTO_TCP)?;
            with_sockaddr(addr, |raw, len| {
                cvt(unsafe { abi::bind(socket.fd, raw, len) }).map(drop)
            })?;
            cvt(unsafe { abi::listen(socket.fd, 128) })?;
            Ok(TcpListener { inner: socket })
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_socket_addr(self.inner.fd, abi::getsockname)
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mut raw = SockAddrIn {
            family: 0,
            port: 0,
            addr: 0,
            zero: [0; 8],
        };
        let mut len = size_of::<SockAddrIn>() as u32;
        let fd = cvt(unsafe {
            abi::accept4(
                self.inner.fd,
                core::ptr::from_mut(&mut raw).cast(),
                &mut len,
                0,
            )
        })?;
        if raw.family as u32 != AF_INET {
            let _ = unsafe { abi::close(fd) };
            return Err(io::Error::from_raw_os_error(97));
        }
        Ok((
            TcpStream { inner: unsafe { Socket::from_raw_fd(fd) } },
            raw.to_std(),
        ))
    }

    pub fn duplicate(&self) -> io::Result<TcpListener> {
        self.inner.duplicate().map(|inner| TcpListener { inner })
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> { unsupported() }
    pub fn ttl(&self) -> io::Result<u32> { unsupported() }
    pub fn set_only_v6(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn only_v6(&self) -> io::Result<bool> { Ok(false) }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpListener").field("inner", &self.inner).finish()
    }
}

pub struct UdpSocket {
    inner: Socket,
}

impl UdpSocket {
    pub(crate) unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { inner: unsafe { Socket::from_raw_fd(fd) } }
    }

    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.inner.as_raw_fd()
    }

    pub(crate) fn into_raw_fd(self) -> i32 {
        self.inner.into_raw_fd()
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        each_addr(addr, |addr| {
            let socket = Socket::new(SOCK_DGRAM, IPPROTO_UDP)?;
            with_sockaddr(addr, |raw, len| {
                cvt(unsafe { abi::bind(socket.fd, raw, len) }).map(drop)
            })?;
            Ok(UdpSocket { inner: socket })
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        read_socket_addr(self.inner.fd, abi::getpeername)
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_socket_addr(self.inner.fd, abi::getsockname)
    }

    pub fn recv_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        // Felix's current recvfrom wrapper doesn't return the sender address.
        unsupported()
    }

    pub fn peek_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> { unsupported() }

    pub fn send_to(&self, _: &[u8], _: &SocketAddr) -> io::Result<usize> {
        // Felix's current sendto wrapper doesn't accept a destination address.
        unsupported()
    }

    pub fn duplicate(&self) -> io::Result<UdpSocket> {
        self.inner.duplicate().map(|inner| UdpSocket { inner })
    }

    pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> { unsupported() }
    pub fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> { unsupported() }
    pub fn read_timeout(&self) -> io::Result<Option<Duration>> { unsupported() }
    pub fn write_timeout(&self) -> io::Result<Option<Duration>> { unsupported() }
    pub fn set_broadcast(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn broadcast(&self) -> io::Result<bool> { unsupported() }
    pub fn set_multicast_loop_v4(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn multicast_loop_v4(&self) -> io::Result<bool> { unsupported() }
    pub fn set_multicast_ttl_v4(&self, _: u32) -> io::Result<()> { unsupported() }
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> { unsupported() }
    pub fn set_multicast_loop_v6(&self, _: bool) -> io::Result<()> { unsupported() }
    pub fn multicast_loop_v6(&self) -> io::Result<bool> { unsupported() }
    pub fn join_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> { unsupported() }
    pub fn join_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> { unsupported() }
    pub fn leave_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> { unsupported() }
    pub fn leave_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> { unsupported() }
    pub fn set_ttl(&self, _: u32) -> io::Result<()> { unsupported() }
    pub fn ttl(&self) -> io::Result<u32> { unsupported() }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> { Ok(None) }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        cvt(unsafe { abi::recv(self.inner.fd, buf.as_mut_ptr(), buf.len()) }).map(|n| n as usize)
    }

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> { unsupported() }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::send(self.inner.fd, buf.as_ptr(), buf.len()) }).map(|n| n as usize)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        each_addr(addr, |addr| {
            with_sockaddr(addr, |raw, len| {
                cvt(unsafe { abi::connect(self.inner.fd, raw, len) }).map(drop)
            })
        })
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpSocket").field("inner", &self.inner).finish()
    }
}

pub struct LookupHost {
    addrs: crate::vec::IntoIter<SocketAddr>,
}

impl Iterator for LookupHost {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<SocketAddr> {
        self.addrs.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.addrs.size_hint()
    }
}

static DNS_QUERY_ID: AtomicU16 = AtomicU16::new(0x4000);
const DNS_PORT: u16 = 53;
const DNS_TIMEOUT_MS: i32 = 3_000;
const POLLIN: i16 = 0x0001;

fn next_dns_query_id() -> u16 {
    let mut bytes = [0u8; 2];
    if unsafe { abi::getrandom(bytes.as_mut_ptr(), bytes.len(), 0) } == bytes.len() as i32 {
        return u16::from_ne_bytes(bytes);
    }
    DNS_QUERY_ID.fetch_add(1, Ordering::Relaxed)
}

fn dns_server() -> Ipv4Addr {
    let mut cfg = abi::IfConfig::default();
    if unsafe { abi::ifconfig_get(&mut cfg) } >= 0 && cfg.dns != 0 {
        return Ipv4Addr::from(cfg.dns.to_be_bytes());
    }

    // Static configurations made before Felix exposed a DNS field may not
    // have a resolver address. Keep a public resolver as a last-resort
    // fallback instead of making hostname lookup impossible.
    Ipv4Addr::new(1, 1, 1, 1)
}

fn build_dns_query(name: &str, id: u16, out: &mut [u8; 512]) -> io::Result<usize> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid DNS name"));
    }

    out.fill(0);
    out[0..2].copy_from_slice(&id.to_be_bytes());
    out[2] = 0x01; // RD: recursion desired
    out[4..6].copy_from_slice(&1u16.to_be_bytes()); // QDCOUNT

    let mut pos = 12usize;
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 || pos + 1 + label.len() + 5 > out.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid DNS label"));
        }
        out[pos] = label.len() as u8;
        pos += 1;
        out[pos..pos + label.len()].copy_from_slice(label.as_bytes());
        pos += label.len();
    }

    out[pos] = 0;
    pos += 1;
    out[pos..pos + 2].copy_from_slice(&1u16.to_be_bytes()); // QTYPE=A
    pos += 2;
    out[pos..pos + 2].copy_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
    pos += 2;
    Ok(pos)
}

fn skip_dns_name(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *data.get(pos)?;
        if len == 0 {
            return Some(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            data.get(pos + 1)?;
            return Some(pos + 2);
        }
        if len & 0xC0 != 0 || len > 63 {
            return None;
        }
        pos = pos.checked_add(1 + len as usize)?;
        if pos > data.len() {
            return None;
        }
    }
}

fn parse_dns_response(data: &[u8], id: u16, port: u16) -> io::Result<Vec<SocketAddr>> {
    if data.len() < 12 || u16::from_be_bytes([data[0], data[1]]) != id {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid DNS response"));
    }

    let flags = u16::from_be_bytes([data[2], data[3]]);
    if flags & 0x8000 == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "DNS packet is not a response"));
    }
    if flags & 0x0200 != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "truncated DNS response"));
    }
    let rcode = flags & 0x000f;
    if rcode != 0 {
        return Err(io::Error::new(io::ErrorKind::NotFound, "DNS name not found"));
    }

    let qdcount = u16::from_be_bytes([data[4], data[5]]) as usize;
    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
    let mut pos = 12usize;

    for _ in 0..qdcount {
        pos = skip_dns_name(data, pos)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad DNS question"))?;
        if pos + 4 > data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "short DNS question"));
        }
        pos += 4;
    }

    let mut addrs = Vec::new();
    for _ in 0..ancount {
        pos = skip_dns_name(data, pos)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad DNS answer name"))?;
        if pos + 10 > data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "short DNS answer"));
        }

        let rr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let rr_class = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "short DNS rdata"));
        }

        if rr_type == 1 && rr_class == 1 && rdlen == 4 {
            let ip = Ipv4Addr::new(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
            let addr = SocketAddr::V4(SocketAddrV4::new(ip, port));
            if !addrs.contains(&addr) {
                addrs.push(addr);
            }
        }
        pos += rdlen;
    }

    if addrs.is_empty() {
        Err(io::Error::new(io::ErrorKind::NotFound, "DNS response has no IPv4 address"))
    } else {
        Ok(addrs)
    }
}

fn lookup_dns(host: &str, port: u16) -> io::Result<LookupHost> {
    let id = next_dns_query_id();
    let mut query = [0u8; 512];
    let query_len = build_dns_query(host, id, &mut query)?;

    let socket = Socket::new(SOCK_DGRAM, IPPROTO_UDP)?;
    let server = SocketAddr::V4(SocketAddrV4::new(dns_server(), DNS_PORT));
    with_sockaddr(&server, |raw, len| {
        cvt(unsafe { abi::connect(socket.fd, raw, len) }).map(drop)
    })?;
    socket.set_nonblocking(true)?;

    let sent = cvt(unsafe { abi::send(socket.fd, query.as_ptr(), query_len) })? as usize;
    if sent != query_len {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "short DNS query send"));
    }

    let mut pollfd = abi::PollFd { fd: socket.fd, events: POLLIN, revents: 0 };
    let ready = cvt(unsafe { abi::poll(&mut pollfd, 1, DNS_TIMEOUT_MS) })?;
    if ready == 0 || pollfd.revents & POLLIN == 0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "DNS lookup timed out"));
    }

    let mut response = [0u8; 512];
    let received = cvt(unsafe { abi::recv(socket.fd, response.as_mut_ptr(), response.len()) })? as usize;
    let addrs = parse_dns_response(&response[..received], id, port)?;
    Ok(LookupHost { addrs: addrs.into_iter() })
}

pub fn lookup_host(host: &str, port: u16) -> io::Result<LookupHost> {
    if host.eq_ignore_ascii_case("localhost") {
        let mut addrs = Vec::with_capacity(1);
        addrs.push(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)));
        return Ok(LookupHost { addrs: addrs.into_iter() });
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        let mut addrs = Vec::with_capacity(1);
        addrs.push(SocketAddr::V4(SocketAddrV4::new(ip, port)));
        return Ok(LookupHost { addrs: addrs.into_iter() });
    }
    lookup_dns(host, port)
}
