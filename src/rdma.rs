use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, ToSocketAddrs, UdpSocket},
    path::Path,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const DEFAULT_RDMA_MIN_SIZE: u64 = 64 * 1024 * 1024;
pub const DEFAULT_RDMA_CHUNK_SIZE: usize = 64 * 1024;
pub const DEFAULT_RDMA_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_RDMA_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);

const PROTOCOL_VERSION: u16 = 1;
const MAGIC: &[u8; 8] = b"PRDMA001";
const HEADER_LEN: usize = 40;
const KIND_DATA: u8 = 1;
const KIND_DONE: u8 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RdmaMode {
    #[default]
    Auto,
    Off,
    Require,
}

impl FromStr for RdmaMode {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" | "on" | "true" | "yes" | "1" => Ok(Self::Auto),
            "off" | "false" | "no" | "0" | "disabled" => Ok(Self::Off),
            "require" | "required" | "force" => Ok(Self::Require),
            other => Err(format!(
                "invalid RDMA mode '{other}' (expected auto, off, or require)"
            )),
        }
    }
}

impl fmt::Display for RdmaMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Off => f.write_str("off"),
            Self::Require => f.write_str("require"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RdmaTransferOptions {
    pub bind_addr: Option<IpAddr>,
    pub helper_command: String,
    pub chunk_size: usize,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum RdmaCopyResult {
    Copied { bytes: u64, chunks: u64 },
    Unavailable { reason: String, cache_for_run: bool },
}

impl RdmaCopyResult {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            cache_for_run: false,
        }
    }

    pub fn setup_unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
            cache_for_run: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaSendRequest {
    pub protocol_version: u16,
    pub source_path: String,
    pub destination_addr: String,
    pub destination_port: u16,
    pub token_hex: String,
    pub file_size: u64,
    pub chunk_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdmaSendReport {
    pub protocol_version: u16,
    pub bytes_sent: u64,
    pub chunks_sent: u64,
}

#[derive(Debug, Clone)]
pub struct RdmaReceiveReport {
    pub bytes_received: u64,
    pub chunks_received: u64,
}

pub fn new_send_request(
    source_path: String,
    destination_addr: Ipv4Addr,
    destination_port: u16,
    token: [u8; 16],
    file_size: u64,
    chunk_size: usize,
) -> RdmaSendRequest {
    RdmaSendRequest {
        protocol_version: PROTOCOL_VERSION,
        source_path,
        destination_addr: destination_addr.to_string(),
        destination_port,
        token_hex: hex_encode(&token),
        file_size,
        chunk_size,
    }
}

pub fn run_send_stdio() -> Result<()> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .context("read RDMA helper request")?;
    let request: RdmaSendRequest =
        serde_json::from_slice(&input).context("parse RDMA helper request")?;

    let mut stdout = io::stdout();
    let report = send_file_with_keepalive(&request, || {
        stdout
            .write_all(b"\n")
            .context("write RDMA helper keepalive")?;
        stdout.flush().context("flush RDMA helper keepalive")
    })?;
    serde_json::to_writer(&mut stdout, &report).context("write RDMA helper report")?;
    stdout
        .write_all(b"\n")
        .context("write RDMA helper report newline")?;
    Ok(())
}

pub fn rdma_transport_available() -> bool {
    has_entries(Path::new("/sys/class/infiniband")) && platform_rdma_transport_available()
}

#[cfg(target_os = "linux")]
fn platform_rdma_transport_available() -> bool {
    linux::rsocket_available()
}

#[cfg(not(target_os = "linux"))]
fn platform_rdma_transport_available() -> bool {
    false
}

pub fn infer_local_ipv4_for_remote(host: &str, port: u16) -> Result<Ipv4Addr> {
    let mut last_err = None;
    for remote in (host, port)
        .to_socket_addrs()
        .with_context(|| format!("resolve remote host for RDMA: {host}"))?
    {
        let bind_addr = match remote {
            std::net::SocketAddr::V4(_) => "0.0.0.0:0",
            std::net::SocketAddr::V6(_) => continue,
        };
        match UdpSocket::bind(bind_addr).and_then(|socket| {
            socket.connect(remote)?;
            socket.local_addr()
        }) {
            Ok(std::net::SocketAddr::V4(local)) if !local.ip().is_unspecified() => {
                return Ok(*local.ip());
            }
            Ok(_) => {}
            Err(err) => last_err = Some(err),
        }
    }

    if let Some(err) = last_err {
        Err(err).context("infer local IPv4 address for RDMA")
    } else {
        bail!("remote host did not resolve to an IPv4 address usable for RDMA")
    }
}

pub fn ipv4_bind_addr(
    addr: Option<IpAddr>,
    remote_host: &str,
    remote_port: u16,
) -> Result<Ipv4Addr> {
    match addr {
        Some(IpAddr::V4(addr)) if !addr.is_unspecified() => Ok(addr),
        Some(IpAddr::V4(addr)) => {
            bail!("RDMA bind address must be a specific local IPv4 address, got {addr}")
        }
        Some(IpAddr::V6(addr)) => bail!("RDMA fast path requires an IPv4 bind address, got {addr}"),
        None => infer_local_ipv4_for_remote(remote_host, remote_port),
    }
}

pub fn random_token() -> [u8; 16] {
    let mut token = [0_u8; 16];
    if fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut token))
        .is_ok()
    {
        return token;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    token.copy_from_slice(&(now ^ pid.rotate_left(17)).to_be_bytes());
    token
}

fn has_entries(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.any(|entry| entry.is_ok()))
        .unwrap_or(false)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode_16(value: &str) -> Result<[u8; 16]> {
    if value.len() != 32 {
        bail!("RDMA token must be 32 hex characters");
    }
    let mut out = [0_u8; 16];
    for (idx, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[idx] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex digit in RDMA token"),
    }
}

fn encode_packet(kind: u8, token: &[u8; 16], offset: u64, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > u32::MAX as usize {
        bail!("RDMA packet payload too large");
    }
    let mut packet = Vec::with_capacity(HEADER_LEN + payload.len());
    packet.extend_from_slice(MAGIC);
    packet.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    packet.push(kind);
    packet.push(0);
    packet.extend_from_slice(token);
    packet.extend_from_slice(&offset.to_be_bytes());
    packet.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    packet.extend_from_slice(payload);
    Ok(packet)
}

struct DecodedPacket<'a> {
    kind: u8,
    token: [u8; 16],
    offset: u64,
    payload: &'a [u8],
}

fn decode_packet(buf: &[u8]) -> Result<DecodedPacket<'_>> {
    if buf.len() < HEADER_LEN {
        bail!("short RDMA packet header");
    }
    if &buf[..MAGIC.len()] != MAGIC {
        bail!("invalid RDMA packet magic");
    }
    let version = u16::from_be_bytes([buf[8], buf[9]]);
    if version != PROTOCOL_VERSION {
        bail!("unsupported RDMA packet protocol version {version}");
    }
    let kind = buf[10];
    let mut token = [0_u8; 16];
    token.copy_from_slice(&buf[12..28]);
    let offset = u64::from_be_bytes(buf[28..36].try_into().expect("slice size"));
    let payload_len = u32::from_be_bytes(buf[36..40].try_into().expect("slice size")) as usize;
    if HEADER_LEN + payload_len != buf.len() {
        bail!("RDMA packet payload length mismatch");
    }
    Ok(DecodedPacket {
        kind,
        token,
        offset,
        payload: &buf[HEADER_LEN..],
    })
}

fn packet_payload_len(header: &[u8]) -> Result<usize> {
    if header.len() < HEADER_LEN {
        bail!("short RDMA packet header");
    }
    if &header[..MAGIC.len()] != MAGIC {
        bail!("invalid RDMA packet magic");
    }
    let version = u16::from_be_bytes([header[8], header[9]]);
    if version != PROTOCOL_VERSION {
        bail!("unsupported RDMA packet protocol version {version}");
    }
    Ok(u32::from_be_bytes(header[36..40].try_into().expect("slice size")) as usize)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use nix::libc;
    use std::{ffi::CStr, mem, os::fd::RawFd, sync::OnceLock};

    type RsocketFn = unsafe extern "C" fn(libc::c_int, libc::c_int, libc::c_int) -> libc::c_int;
    type RbindFn =
        unsafe extern "C" fn(libc::c_int, *const libc::sockaddr, libc::socklen_t) -> libc::c_int;
    type RlistenFn = unsafe extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int;
    type RacceptFn =
        unsafe extern "C" fn(libc::c_int, *mut libc::sockaddr, *mut libc::socklen_t) -> libc::c_int;
    type RconnectFn =
        unsafe extern "C" fn(libc::c_int, *const libc::sockaddr, libc::socklen_t) -> libc::c_int;
    type RcloseFn = unsafe extern "C" fn(libc::c_int) -> libc::c_int;
    type RrecvFn =
        unsafe extern "C" fn(libc::c_int, *mut libc::c_void, usize, libc::c_int) -> libc::ssize_t;
    type RsendFn =
        unsafe extern "C" fn(libc::c_int, *const libc::c_void, usize, libc::c_int) -> libc::ssize_t;
    type RpollFn =
        unsafe extern "C" fn(*mut libc::pollfd, libc::nfds_t, libc::c_int) -> libc::c_int;
    type RgetsocknameFn =
        unsafe extern "C" fn(libc::c_int, *mut libc::sockaddr, *mut libc::socklen_t) -> libc::c_int;
    type RgetsockoptFn = unsafe extern "C" fn(
        libc::c_int,
        libc::c_int,
        libc::c_int,
        *mut libc::c_void,
        *mut libc::socklen_t,
    ) -> libc::c_int;
    type RfcntlFn = unsafe extern "C" fn(libc::c_int, libc::c_int, ...) -> libc::c_int;

    struct RsocketApi {
        rsocket: RsocketFn,
        rbind: RbindFn,
        rlisten: RlistenFn,
        raccept: RacceptFn,
        rconnect: RconnectFn,
        rclose: RcloseFn,
        rrecv: RrecvFn,
        rsend: RsendFn,
        rpoll: RpollFn,
        rgetsockname: RgetsocknameFn,
        rgetsockopt: RgetsockoptFn,
        rfcntl: RfcntlFn,
    }

    static RSOCKET_API: OnceLock<std::result::Result<RsocketApi, String>> = OnceLock::new();

    pub(super) fn rsocket_available() -> bool {
        rsocket_api().is_ok()
    }

    fn rsocket_api() -> Result<&'static RsocketApi> {
        RSOCKET_API
            .get_or_init(load_rsocket_api)
            .as_ref()
            .map_err(|err| anyhow!(err.clone()))
    }

    fn load_rsocket_api() -> std::result::Result<RsocketApi, String> {
        let handle = open_rdmacm()?;
        Ok(RsocketApi {
            rsocket: load_symbol(handle, b"rsocket\0")?,
            rbind: load_symbol(handle, b"rbind\0")?,
            rlisten: load_symbol(handle, b"rlisten\0")?,
            raccept: load_symbol(handle, b"raccept\0")?,
            rconnect: load_symbol(handle, b"rconnect\0")?,
            rclose: load_symbol(handle, b"rclose\0")?,
            rrecv: load_symbol(handle, b"rrecv\0")?,
            rsend: load_symbol(handle, b"rsend\0")?,
            rpoll: load_symbol(handle, b"rpoll\0")?,
            rgetsockname: load_symbol(handle, b"rgetsockname\0")?,
            rgetsockopt: load_symbol(handle, b"rgetsockopt\0")?,
            rfcntl: load_symbol(handle, b"rfcntl\0")?,
        })
    }

    fn open_rdmacm() -> std::result::Result<*mut libc::c_void, String> {
        for name in [b"librdmacm.so.1\0".as_slice(), b"librdmacm.so\0".as_slice()] {
            let handle =
                unsafe { libc::dlopen(name.as_ptr() as *const libc::c_char, libc::RTLD_NOW) };
            if !handle.is_null() {
                return Ok(handle);
            }
        }
        Err(format!("load librdmacm for RDMA rsockets: {}", dl_error()))
    }

    fn load_symbol<T: Copy>(
        handle: *mut libc::c_void,
        symbol: &'static [u8],
    ) -> std::result::Result<T, String> {
        let ptr = unsafe { libc::dlsym(handle, symbol.as_ptr() as *const libc::c_char) };
        if ptr.is_null() {
            let name = String::from_utf8_lossy(&symbol[..symbol.len().saturating_sub(1)]);
            return Err(format!("load librdmacm symbol {name}: {}", dl_error()));
        }
        debug_assert_eq!(mem::size_of::<T>(), mem::size_of::<*mut libc::c_void>());
        Ok(unsafe { mem::transmute_copy(&ptr) })
    }

    fn dl_error() -> String {
        let err = unsafe { libc::dlerror() };
        if err.is_null() {
            "unknown dynamic loader error".to_string()
        } else {
            unsafe { CStr::from_ptr(err) }
                .to_string_lossy()
                .into_owned()
        }
    }

    fn is_would_block(err: &io::Error) -> bool {
        matches!(
            err.raw_os_error(),
            Some(code)
                if code == libc::EAGAIN
                    || code == libc::EWOULDBLOCK
                    || code == libc::EINPROGRESS
                    || code == libc::EALREADY
        )
    }

    pub struct RdmaReceiver {
        socket: Rsocket,
        bind_addr: Ipv4Addr,
        port: u16,
        token: [u8; 16],
        expected_size: u64,
        chunk_size: usize,
        timeout: Duration,
    }

    impl RdmaReceiver {
        pub fn bind(
            bind_addr: Ipv4Addr,
            expected_size: u64,
            chunk_size: usize,
            timeout: Duration,
        ) -> Result<Self> {
            if !rdma_transport_available() {
                bail!("rdma-core rsocket transport was not detected");
            }

            let socket = Rsocket::open()?;
            let addr = sockaddr_in(bind_addr, 0);
            socket
                .bind(&addr)
                .with_context(|| format!("bind rdma-core rsocket to {bind_addr}:0"))?;
            socket.listen(1).context("listen on rdma-core rsocket")?;

            let port = socket.local_port()?;
            Ok(Self {
                socket,
                bind_addr,
                port,
                token: random_token(),
                expected_size,
                chunk_size: chunk_size.max(1),
                timeout,
            })
        }

        pub fn bind_addr(&self) -> Ipv4Addr {
            self.bind_addr
        }

        pub fn port(&self) -> u16 {
            self.port
        }

        pub fn token(&self) -> [u8; 16] {
            self.token
        }

        pub fn receive_to_path(
            self,
            destination: &Path,
            cancel: Arc<AtomicBool>,
        ) -> Result<RdmaReceiveReport> {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(destination)
                .with_context(|| format!("open RDMA destination: {}", destination.display()))?;
            file.set_len(self.expected_size).with_context(|| {
                format!("preallocate RDMA destination: {}", destination.display())
            })?;

            let mut buf = vec![0_u8; HEADER_LEN + self.chunk_size];
            let mut received = 0_u64;
            let mut chunks = 0_u64;
            let mut saw_done = false;

            while !saw_done {
                let stream = self
                    .socket
                    .accept(&cancel, self.timeout)
                    .context("accept RDMA rsocket connection")?;
                let stream_start = received;
                let mut last_activity = Instant::now();

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        bail!("RDMA receive cancelled");
                    }

                    stream
                        .recv_exact_with_timeout(
                            &mut buf[..HEADER_LEN],
                            &cancel,
                            self.timeout,
                            &mut last_activity,
                        )
                        .context("receive RDMA packet header")?;
                    let payload_len = packet_payload_len(&buf[..HEADER_LEN])?;
                    if payload_len > self.chunk_size {
                        bail!(
                            "RDMA packet payload exceeds configured chunk size ({} > {})",
                            payload_len,
                            self.chunk_size
                        );
                    }
                    let packet_len = HEADER_LEN + payload_len;
                    if payload_len > 0 {
                        stream
                            .recv_exact_with_timeout(
                                &mut buf[HEADER_LEN..packet_len],
                                &cancel,
                                self.timeout,
                                &mut last_activity,
                            )
                            .context("receive RDMA packet payload")?;
                    }
                    let packet = decode_packet(&buf[..packet_len])?;
                    if packet.token != self.token {
                        if received == stream_start {
                            break;
                        }
                        continue;
                    }

                    match packet.kind {
                        KIND_DATA => {
                            if packet.offset != received {
                                bail!(
                                    "out-of-order RDMA packet: expected offset {}, got {}",
                                    received,
                                    packet.offset
                                );
                            }
                            let next = received
                                .checked_add(packet.payload.len() as u64)
                                .ok_or_else(|| anyhow!("RDMA byte count overflow"))?;
                            if next > self.expected_size {
                                bail!(
                                    "RDMA sender exceeded expected file size ({} > {})",
                                    next,
                                    self.expected_size
                                );
                            }
                            file.write_all(packet.payload)
                                .context("write RDMA payload")?;
                            received = next;
                            chunks += 1;
                        }
                        KIND_DONE => {
                            if packet.offset != received {
                                bail!(
                                    "RDMA done offset mismatch: expected {}, got {}",
                                    received,
                                    packet.offset
                                );
                            }
                            saw_done = true;
                            break;
                        }
                        other => bail!("unknown RDMA packet kind {other}"),
                    }
                }
            }

            if received != self.expected_size {
                bail!(
                    "RDMA transfer incomplete: expected {} bytes, received {}",
                    self.expected_size,
                    received
                );
            }
            file.flush().context("flush RDMA destination")?;
            Ok(RdmaReceiveReport {
                bytes_received: received,
                chunks_received: chunks,
            })
        }
    }

    struct Rsocket {
        fd: RawFd,
        api: &'static RsocketApi,
    }

    impl Rsocket {
        fn open() -> Result<Self> {
            let api = rsocket_api()?;
            let fd = unsafe { (api.rsocket)(libc::AF_INET, libc::SOCK_STREAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error()).context("open rdma-core rsocket");
            }
            let socket = Self { fd, api };
            socket.set_nonblocking()?;
            Ok(socket)
        }

        fn fd(&self) -> RawFd {
            self.fd
        }

        fn bind(&self, addr: &libc::sockaddr_in) -> Result<()> {
            let rc = unsafe {
                (self.api.rbind)(
                    self.fd(),
                    addr as *const _ as *const libc::sockaddr,
                    mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                return Err(io::Error::last_os_error()).context("bind rdma-core rsocket");
            }
            Ok(())
        }

        fn listen(&self, backlog: libc::c_int) -> Result<()> {
            let rc = unsafe { (self.api.rlisten)(self.fd(), backlog) };
            if rc < 0 {
                return Err(io::Error::last_os_error()).context("listen on rdma-core rsocket");
            }
            Ok(())
        }

        fn accept(&self, cancel: &AtomicBool, timeout: Duration) -> Result<Self> {
            let started = Instant::now();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    bail!("RDMA receive cancelled");
                }
                if started.elapsed() >= timeout {
                    bail!("RDMA accept timed out after {}s", timeout.as_secs());
                }
                if !self.poll_events(libc::POLLIN, Duration::from_secs(1))? {
                    continue;
                }
                let fd = unsafe {
                    (self.api.raccept)(self.fd(), std::ptr::null_mut(), std::ptr::null_mut())
                };
                if fd < 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    if is_would_block(&err) {
                        continue;
                    }
                    return Err(err).context("accept RDMA rsocket connection");
                }
                let socket = Self { fd, api: self.api };
                socket.set_nonblocking()?;
                return Ok(socket);
            }
        }

        fn local_port(&self) -> Result<u16> {
            let mut addr: libc::sockaddr_in = unsafe { mem::zeroed() };
            let mut len = mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            let rc = unsafe {
                (self.api.rgetsockname)(
                    self.fd(),
                    &mut addr as *mut _ as *mut libc::sockaddr,
                    &mut len,
                )
            };
            if rc < 0 {
                return Err(io::Error::last_os_error()).context("read RDMA socket port");
            }
            Ok(u16::from_be(addr.sin_port))
        }

        fn connect_with_timeout<F>(
            &self,
            destination: &libc::sockaddr_in,
            timeout: Duration,
            keepalive: &mut F,
        ) -> Result<()>
        where
            F: FnMut() -> Result<()>,
        {
            let started = Instant::now();
            let rc = unsafe {
                (self.api.rconnect)(
                    self.fd(),
                    destination as *const _ as *const libc::sockaddr,
                    mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                )
            };
            if rc == 0 {
                return Ok(());
            }

            let err = io::Error::last_os_error();
            if !is_would_block(&err) {
                return Err(err).context("connect RDMA rsocket");
            }

            loop {
                if started.elapsed() >= timeout {
                    bail!(
                        "RDMA rsocket connect timed out after {}s",
                        timeout.as_secs()
                    );
                }
                keepalive()?;
                if !self.poll_events(libc::POLLOUT, Duration::from_secs(1))? {
                    continue;
                }
                let socket_error = self.socket_error()?;
                if socket_error == 0 {
                    return Ok(());
                }
                if matches!(
                    socket_error,
                    code if code == libc::EINPROGRESS
                        || code == libc::EALREADY
                        || code == libc::EAGAIN
                        || code == libc::EWOULDBLOCK
                ) {
                    continue;
                }
                return Err(io::Error::from_raw_os_error(socket_error))
                    .context("connect RDMA rsocket");
            }
        }

        fn poll_events(&self, events: libc::c_short, timeout: Duration) -> Result<bool> {
            let millis = timeout.as_millis().min(libc::c_int::MAX as u128) as libc::c_int;
            let mut pollfd = libc::pollfd {
                fd: self.fd(),
                events,
                revents: 0,
            };
            loop {
                let rc = unsafe { (self.api.rpoll)(&mut pollfd, 1, millis) };
                if rc < 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(err).context("poll RDMA rsocket");
                }
                if rc == 0 {
                    return Ok(false);
                }
                if (pollfd.revents & events) != 0 {
                    return Ok(true);
                }
                if (pollfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
                    let socket_error = match self.socket_error() {
                        Ok(0) | Err(_) => libc::ECONNRESET,
                        Ok(code) => code,
                    };
                    return Err(io::Error::from_raw_os_error(socket_error))
                        .context("poll RDMA rsocket");
                }
                return Ok(false);
            }
        }

        fn socket_error(&self) -> Result<libc::c_int> {
            let mut value: libc::c_int = 0;
            let mut len = mem::size_of_val(&value) as libc::socklen_t;
            let rc = unsafe {
                (self.api.rgetsockopt)(
                    self.fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    &mut value as *mut _ as *mut libc::c_void,
                    &mut len,
                )
            };
            if rc < 0 {
                return Err(io::Error::last_os_error()).context("read RDMA socket error");
            }
            Ok(value)
        }

        fn set_nonblocking(&self) -> Result<()> {
            let flags = unsafe { (self.api.rfcntl)(self.fd(), libc::F_GETFL) };
            if flags < 0 {
                return Err(io::Error::last_os_error())
                    .context("read RDMA rsocket descriptor flags");
            }
            let rc =
                unsafe { (self.api.rfcntl)(self.fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) };
            if rc < 0 {
                return Err(io::Error::last_os_error()).context("set RDMA rsocket nonblocking");
            }
            Ok(())
        }

        fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
            let n = unsafe {
                (self.api.rrecv)(
                    self.fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    0,
                )
            };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(n as usize)
        }

        fn recv_exact_with_timeout(
            &self,
            mut buf: &mut [u8],
            cancel: &AtomicBool,
            timeout: Duration,
            last_activity: &mut Instant,
        ) -> Result<()> {
            while !buf.is_empty() {
                if cancel.load(Ordering::Relaxed) {
                    bail!("RDMA receive cancelled");
                }
                if !self.poll_events(libc::POLLIN, Duration::from_secs(1))? {
                    if last_activity.elapsed() >= timeout {
                        bail!(
                            "RDMA receive timed out after {}s of inactivity",
                            timeout.as_secs()
                        );
                    }
                    continue;
                }
                match self.recv(buf) {
                    Ok(0) => bail!("RDMA sender closed connection"),
                    Ok(n) => {
                        *last_activity = Instant::now();
                        let (_, rest) = buf.split_at_mut(n);
                        buf = rest;
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) if is_would_block(&err) => continue,
                    Err(err) => return Err(err).context("receive RDMA rsocket bytes"),
                }
            }
            Ok(())
        }

        fn send_all_with_timeout<F>(
            &self,
            mut buf: &[u8],
            timeout: Duration,
            keepalive: &mut F,
        ) -> Result<()>
        where
            F: FnMut() -> Result<()>,
        {
            let mut last_progress = Instant::now();
            while !buf.is_empty() {
                let n = unsafe {
                    (self.api.rsend)(self.fd(), buf.as_ptr() as *const libc::c_void, buf.len(), 0)
                };
                if n < 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    if is_would_block(&err) {
                        if last_progress.elapsed() >= timeout {
                            bail!("RDMA rsocket send timed out after {}s", timeout.as_secs());
                        }
                        keepalive()?;
                        let _ = self.poll_events(libc::POLLOUT, Duration::from_secs(1))?;
                        continue;
                    }
                    return Err(err).context("send RDMA rsocket bytes");
                }
                if n == 0 {
                    bail!("RDMA rsocket sent zero bytes");
                }
                last_progress = Instant::now();
                buf = &buf[n as usize..];
            }
            Ok(())
        }
    }

    impl Drop for Rsocket {
        fn drop(&mut self) {
            unsafe {
                (self.api.rclose)(self.fd);
            }
        }
    }

    pub fn send_file_with_keepalive<F>(
        request: &RdmaSendRequest,
        mut keepalive: F,
    ) -> Result<RdmaSendReport>
    where
        F: FnMut() -> Result<()>,
    {
        if !rdma_transport_available() {
            bail!("rdma-core rsocket transport was not detected");
        }

        if request.protocol_version != PROTOCOL_VERSION {
            bail!(
                "unsupported RDMA helper protocol version {}",
                request.protocol_version
            );
        }
        let token = hex_decode_16(&request.token_hex)?;
        let destination_addr: Ipv4Addr = request.destination_addr.parse().with_context(|| {
            format!(
                "parse RDMA destination address {}",
                request.destination_addr
            )
        })?;
        let chunk_size = request.chunk_size.max(1);
        let mut file = fs::File::open(&request.source_path)
            .with_context(|| format!("open RDMA source file: {}", request.source_path))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("stat RDMA source file: {}", request.source_path))?;
        if metadata.len() != request.file_size {
            bail!(
                "RDMA source size changed before transfer: expected {}, got {}",
                request.file_size,
                metadata.len()
            );
        }

        let socket = Rsocket::open()?;
        let destination = sockaddr_in(destination_addr, request.destination_port);
        keepalive()?;
        let mut last_keepalive = Instant::now();
        let mut emit_keepalive = || {
            if last_keepalive.elapsed() >= DEFAULT_RDMA_KEEPALIVE_INTERVAL {
                keepalive()?;
                last_keepalive = Instant::now();
            }
            Ok(())
        };
        socket.connect_with_timeout(&destination, DEFAULT_RDMA_TIMEOUT, &mut emit_keepalive)?;
        let mut read_buf = vec![0_u8; chunk_size];
        let mut offset = 0_u64;
        let mut chunks = 0_u64;

        loop {
            let n = file
                .read(&mut read_buf)
                .with_context(|| format!("read RDMA source file: {}", request.source_path))?;
            if n == 0 {
                break;
            }
            let packet = encode_packet(KIND_DATA, &token, offset, &read_buf[..n])?;
            socket.send_all_with_timeout(&packet, DEFAULT_RDMA_TIMEOUT, &mut emit_keepalive)?;
            offset += n as u64;
            chunks += 1;
        }

        if offset != request.file_size {
            bail!(
                "RDMA source read length changed: expected {}, read {}",
                request.file_size,
                offset
            );
        }
        let done = encode_packet(KIND_DONE, &token, offset, &[])?;
        socket.send_all_with_timeout(&done, DEFAULT_RDMA_TIMEOUT, &mut emit_keepalive)?;

        Ok(RdmaSendReport {
            protocol_version: PROTOCOL_VERSION,
            bytes_sent: offset,
            chunks_sent: chunks,
        })
    }

    fn sockaddr_in(addr: Ipv4Addr, port: u16) -> libc::sockaddr_in {
        let mut out: libc::sockaddr_in = unsafe { mem::zeroed() };
        out.sin_family = libc::AF_INET as libc::sa_family_t;
        out.sin_port = port.to_be();
        out.sin_addr = libc::in_addr {
            s_addr: u32::from_ne_bytes(addr.octets()),
        };
        out
    }
}

#[cfg(target_os = "linux")]
pub use linux::RdmaReceiver;

#[cfg(target_os = "linux")]
fn send_file_with_keepalive<F>(request: &RdmaSendRequest, keepalive: F) -> Result<RdmaSendReport>
where
    F: FnMut() -> Result<()>,
{
    linux::send_file_with_keepalive(request, keepalive)
}

#[cfg(target_os = "linux")]
pub fn send_file(request: &RdmaSendRequest) -> Result<RdmaSendReport> {
    send_file_with_keepalive(request, || Ok(()))
}

#[cfg(not(target_os = "linux"))]
pub struct RdmaReceiver;

#[cfg(not(target_os = "linux"))]
impl RdmaReceiver {
    pub fn bind(
        _bind_addr: Ipv4Addr,
        _expected_size: u64,
        _chunk_size: usize,
        _timeout: Duration,
    ) -> Result<Self> {
        bail!("RDMA fast path is only available on Linux")
    }
}

#[cfg(not(target_os = "linux"))]
fn send_file_with_keepalive<F>(_request: &RdmaSendRequest, _keepalive: F) -> Result<RdmaSendReport>
where
    F: FnMut() -> Result<()>,
{
    bail!("RDMA fast path is only available on Linux")
}

#[cfg(not(target_os = "linux"))]
pub fn send_file(_request: &RdmaSendRequest) -> Result<RdmaSendReport> {
    bail!("RDMA fast path is only available on Linux")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdma_mode_parses_env_values() {
        assert_eq!("auto".parse::<RdmaMode>().unwrap(), RdmaMode::Auto);
        assert_eq!("true".parse::<RdmaMode>().unwrap(), RdmaMode::Auto);
        assert_eq!("off".parse::<RdmaMode>().unwrap(), RdmaMode::Off);
        assert_eq!("require".parse::<RdmaMode>().unwrap(), RdmaMode::Require);
        assert!("maybe".parse::<RdmaMode>().is_err());
    }

    #[test]
    fn packet_round_trips() {
        let token = [7_u8; 16];
        let encoded = encode_packet(KIND_DATA, &token, 42, b"payload").expect("encode");
        let decoded = decode_packet(&encoded).expect("decode");
        assert_eq!(decoded.kind, KIND_DATA);
        assert_eq!(decoded.token, token);
        assert_eq!(decoded.offset, 42);
        assert_eq!(decoded.payload, b"payload");
    }
}
