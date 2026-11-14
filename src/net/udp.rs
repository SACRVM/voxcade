//! The network game on a UDP socket (`docs/multiplayer.md`, stage 4,
//! "What travels"): one socket, not blocking, emptied every frame; the
//! host on port 26500 or the next free up to 26515, a guest on any.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use anyhow::{Context, Result};

/// The port a host listens on (UDP), and the last it tries when that one
/// is taken (a second host on one machine).
pub const PORT: u16 = 26500;
const LAST_PORT: u16 = 26515;

/// Datagrams read from a socket in one frame at most.
const DRAIN: usize = 4096;

pub struct Udp {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl Udp {
    fn on(socket: UdpSocket) -> Result<Self> {
        socket.set_nonblocking(true).context("making the socket not wait")?;
        windows::quiet_resets(&socket);
        Ok(Self { socket, buf: vec![0; 2048] })
    }

    /// A socket on any port that may broadcast: the search for games on
    /// the LAN.
    pub fn broadcast() -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0)).context("opening a socket")?;
        socket.set_broadcast(true).context("letting the socket broadcast")?;
        Self::on(socket)
    }

    /// Searches the LAN for games: a broadcast to every port a host may
    /// listen on, to every network this machine is on (a laptop with two
    /// sends the plain broadcast out of one only).
    pub fn search(&self) {
        let mut to = vec![std::net::Ipv4Addr::BROADCAST];
        to.extend(interfaces().into_iter().filter(|(name, ip, _)| private(*ip) && !virtual_(name)).map(|(_, ip, mask)| std::net::Ipv4Addr::from(u32::from(ip) | !u32::from(mask))));
        for ip in to {
            for port in PORT..=LAST_PORT {
                self.send(SocketAddr::from((ip, port)), &super::wire::find());
            }
        }
    }

    /// A host's socket on `port` (the default's next free one if taken,
    /// up to 26515): where it listens.
    pub fn host(port: Option<u16>) -> Result<(Self, u16)> {
        let ports = match port {
            Some(p) => p..=p,
            None => PORT..=LAST_PORT,
        };
        let mut last = None;
        for p in ports {
            match UdpSocket::bind(("0.0.0.0", p)) {
                Ok(socket) => return Ok((Self::on(socket)?, p)),
                Err(e) => last = Some(e),
            }
        }
        Err(last.map_or_else(|| anyhow::anyhow!("no port"), anyhow::Error::from)).context("no port free for the game (26500 to 26515, UDP)")
    }

    /// Where the socket is found on this machine (a test's).
    #[cfg(test)]
    pub(crate) fn here(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.socket.local_addr().map_or(0, |a| a.port())))
    }

    /// A guest's socket on any port.
    pub fn guest() -> Result<Self> {
        Self::on(UdpSocket::bind(("0.0.0.0", 0)).context("opening a socket")?)
    }

    pub fn send(&self, to: SocketAddr, bytes: &[u8]) {
        // A full buffer or an unreachable machine: the datagram is lost, as
        // any may be.
        let _ = self.socket.send_to(bytes, to);
    }

    /// What came since the last call.
    pub fn drain(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        let mut out = Vec::new();
        for _ in 0..DRAIN {
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, from)) => out.push((from, self.buf[..n].to_vec())),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                // Windows says a datagram sent earlier was refused: the next
                // read goes on.
                Err(_) => continue,
            }
        }
        out
    }
}

/// An address to join: `host[:port]`, the port 26500 unless given.
pub fn address(text: &str) -> Result<SocketAddr> {
    let with_port = if text.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { text.to_string() } else { format!("{text}:{PORT}") };
    with_port.to_socket_addrs().with_context(|| format!("no machine '{text}'"))?.find(SocketAddr::is_ipv4).with_context(|| format!("'{text}' has no IPv4 address"))
}

/// Where this machine is found on its networks: each private IPv4
/// address of a network that is up, the default route's first; no
/// loopback, link-local, container, VM or VPN addresses.
pub fn addresses() -> Vec<std::net::Ipv4Addr> {
    let mut out = Vec::new();
    // The default route's: a socket "connected" to an outside address
    // sends nothing, but knows which of its addresses it would send from.
    if let Ok(socket) = UdpSocket::bind(("0.0.0.0", 0))
        && socket.connect(("192.0.2.1", 9)).is_ok()
        && let Ok(SocketAddr::V4(local)) = socket.local_addr()
        && private(*local.ip())
    {
        out.push(*local.ip());
    }
    for (name, ip, _) in interfaces() {
        if private(ip) && !virtual_(&name) && !out.contains(&ip) {
            out.push(ip);
        }
    }
    out
}

/// A network of containers, virtual machines or a VPN, by its name.
fn virtual_(name: &str) -> bool {
    ["lo", "docker", "br-", "veth", "virbr", "vmnet", "vbox", "tun", "tap", "wg", "tailscale", "zt", "utun"].iter().any(|p| name.starts_with(p))
}

/// A private address of a home network (10/8, 172.16/12, 192.168/16).
fn private(ip: std::net::Ipv4Addr) -> bool {
    ip.is_private() && !ip.is_loopback() && !ip.is_link_local()
}

/// The networks this machine is on: each one's name, its address and mask.
#[cfg(unix)]
fn interfaces() -> Vec<(String, std::net::Ipv4Addr, std::net::Ipv4Addr)> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills a list the system owns until freeifaddrs;
    // every pointer in it is read only while it lives, each checked for
    // null, an address read as IPv4 only where its family says so.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut at = list;
        while !at.is_null() {
            let ifa = &*at;
            let up = ifa.ifa_flags & libc::IFF_UP as libc::c_uint != 0;
            if up && !ifa.ifa_addr.is_null() && !ifa.ifa_netmask.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET && !ifa.ifa_name.is_null() {
                let addr = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let mask = &*(ifa.ifa_netmask as *const libc::sockaddr_in);
                let ip = std::net::Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr));
                let mask = std::net::Ipv4Addr::from(u32::from_be(mask.sin_addr.s_addr));
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
                out.push((name, ip, mask));
            }
            at = ifa.ifa_next;
        }
        libc::freeifaddrs(list);
    }
    out
}

#[cfg(not(unix))]
fn interfaces() -> Vec<(String, std::net::Ipv4Addr, std::net::Ipv4Addr)> {
    Vec::new()
}

/// Windows tells a socket that a datagram it sent earlier found no
/// listener by failing its next read (and drops it after a network
/// reset): both off, so one machine gone leaves the others' socket alone
/// (rust-lang/rust#128072). No `SO_REUSEADDR` (std sets none): a second
/// host on one machine takes the next port.
#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawSocket;

    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAIoctl(s: usize, code: u32, inbuf: *const u32, inlen: u32, outbuf: *mut u8, outlen: u32, returned: *mut u32, overlapped: *mut u8, completion: *mut u8) -> i32;
    }

    /// `_WSAIOW(IOC_VENDOR, 12)` and `(IOC_VENDOR, 15)`.
    const SIO_UDP_CONNRESET: u32 = 0x9800_000C;
    const SIO_UDP_NETRESET: u32 = 0x9800_000F;

    pub fn quiet_resets(socket: &std::net::UdpSocket) {
        let off: u32 = 0;
        for code in [SIO_UDP_CONNRESET, SIO_UDP_NETRESET] {
            let mut returned = 0u32;
            // SAFETY: the socket is open for the call; the input is one u32
            // of 4 bytes, no output, no overlapped call.
            unsafe {
                WSAIoctl(socket.as_raw_socket() as usize, code, &off, 4, std::ptr::null_mut(), 0, &mut returned, std::ptr::null_mut(), std::ptr::null_mut());
            }
        }
    }
}

#[cfg(not(windows))]
mod windows {
    pub fn quiet_resets(_socket: &std::net::UdpSocket) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{Guest, Host, Notice};
    use std::time::{Duration, Instant};

    /// This machine's address of a socket bound to every address.
    fn here(udp: &Udp) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], udp.socket.local_addr().unwrap().port()))
    }

    /// Datagrams up to the protocol's largest go from one socket to another
    /// on this machine whole; a socket with nothing come drains nothing at
    /// once (it never waits).
    #[test]
    fn datagrams_go_between_sockets_on_this_machine() {
        let (a, mut b) = (Udp::guest().unwrap(), Udp::guest().unwrap());
        let started = Instant::now();
        assert!(b.drain().is_empty() && started.elapsed() < Duration::from_millis(100), "drain does not wait");
        let sent: Vec<Vec<u8>> = [1, 13, 600, crate::net::MAX_DATAGRAM].iter().map(|&n| (0..n).map(|i| (i * 7 + n) as u8).collect()).collect();
        for d in &sent {
            a.send(here(&b), d);
        }
        let mut came = Vec::new();
        while came.len() < sent.len() && started.elapsed() < Duration::from_secs(5) {
            came.extend(b.drain().into_iter().map(|(from, d)| {
                assert_eq!(from.port(), a.socket.local_addr().unwrap().port());
                d
            }));
            std::thread::sleep(Duration::from_millis(1));
        }
        came.sort();
        let mut want = sent.clone();
        want.sort();
        assert_eq!(came, want);
    }

    /// An address to join: the port 26500 unless given; a name of this
    /// machine; a word that is no port is part of the name.
    #[test]
    fn an_address_takes_the_default_port() {
        assert_eq!(address("127.0.0.1").unwrap(), SocketAddr::from(([127, 0, 0, 1], PORT)));
        assert_eq!(address("127.0.0.1:27001").unwrap().port(), 27001);
        assert_eq!(address("localhost:27002").unwrap(), SocketAddr::from(([127, 0, 0, 1], 27002)));
        assert!(address("127.0.0.1:notaport").is_err());
    }

    /// A host and a guest, each with its own socket on this machine, as
    /// `run --host` and `--join 127.0.0.1` have them: the guest is let in,
    /// gets the run so far in pieces whole and the host's ticks after it.
    #[test]
    fn a_guest_joins_a_host_over_sockets_on_this_machine() {
        let (mut host_socket, mut guest_socket) = (Udp::guest().unwrap(), Udp::guest().unwrap());
        let mut host: Host<SocketAddr> = Host::new(7, "host", 2);
        let mut guest = Guest::new(here(&host_socket), [5; 16], 1, "guest", crate::net::machine::COMMIT, 0);
        // The run so far: several pieces.
        let run: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let (mut got, mut played, mut ticks) = (None, 0, 0u64);
        let started = Instant::now();
        while (got.is_none() || played < 60) && started.elapsed() < Duration::from_secs(10) {
            let now = started.elapsed().as_micros() as u64;
            for (to, d) in guest.outgoing(now) {
                guest_socket.send(to, &d);
            }
            for (from, d) in host_socket.drain() {
                host.receive(now, from, &d);
            }
            for notice in host.notices() {
                if let Notice::Joining { index, .. } = notice {
                    host.admit(index, run.clone());
                }
            }
            while ticks < now / 16_667 {
                host.tick(Vec::new()).unwrap();
                ticks += 1;
            }
            for (to, d) in host.outgoing(now) {
                host_socket.send(to, &d);
            }
            for (from, d) in guest_socket.drain() {
                guest.receive(now, from, &d);
            }
            if got.is_none() {
                got = guest.take(1);
            }
            while guest.next_bundle().is_some() {
                played += 1;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(guest.welcome().is_some() && guest.gone().is_none(), "let in");
        assert_eq!(got.as_deref(), Some(run.as_slice()), "the run so far, whole");
        assert!(played >= 60, "the host's ticks: {played}");
        assert_eq!(host.guests().len(), 1);
    }
}
