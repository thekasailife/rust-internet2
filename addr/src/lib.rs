// Internet2 addresses with support for Tor vv3
//
// Written in 2019-2021 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//     Martin Habovstiak <martin.habovstiak@gmail.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

//! Universal internet addresses that support IPv4, IPv6 and Tor

#![recursion_limit = "256"]
// Coding conventions
#![deny(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    unused_mut,
    unused_imports,
    dead_code,
    missing_docs
)]

#[allow(unused_imports)]
#[macro_use]
extern crate amplify;
#[cfg(feature = "stringly_conversions")]
#[macro_use]
extern crate stringly_conversions_crate as stringly_conversions;
#[cfg(feature = "strict_encoding")]
extern crate strict_encoding;
#[cfg(feature = "serde")]
#[macro_use]
extern crate serde_crate as serde;

#[cfg(feature = "strict_encoding")]
mod encoding;

use std::cmp::Ordering;
#[cfg(feature = "tor")]
use std::convert::TryFrom;
use std::fmt;
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6,
};
use std::num::ParseIntError;
use std::str::FromStr;

#[cfg(feature = "tor")]
use torut::onion::{OnionAddressV3, TorPublicKeyV3};

/// Address type do not support ONION address format and can be used only with
/// IPv4 or IPv6 addresses
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display, Error
)]
#[display(doc_comments)]
pub struct NoOnionSupportError;

/// Errors during address string parse process
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display, Error, From
)]
#[display(doc_comments)]
pub enum AddrParseError {
    /// Wrong port number; must be a 16-bit unsigned integer number
    #[from(ParseIntError)]
    WrongPortNumber,

    /// Can't recognize IPv4, v6 or Onion v2/v3 address in string "{_0}"
    WrongAddrFormat(String),

    /// Wrong format of socket address string "{_0}"; use
    /// <inet_address>\[:<port>\]
    WrongSocketFormat(String),

    /// Wrong format of extended socket address string "{_0}"; use
    /// <transport>://<inet_address>\[:<port>\]
    WrongSocketExtFormat(String),

    /// Unknown transport protocol "{_0}"
    UnknownProtocolError(String),

    /// Tor addresses are not supported; consider compiling with `tor` feature
    NeedsTorFeature,
}

/// A universal address covering IPv4, IPv6 and Tor in a single byte sequence
/// of 32 bytes.
///
/// Holds either:
/// * IPv4-to-IPv6 address
/// * IPv6 address
/// * Tor Onion address (V3 only)
///
/// NB: we are using [`TorPublicKeyV3`] instead of `OnionAddressV3`, since
/// `OnionAddressV3` keeps checksum and other information which can be
/// reconstructed from [`TorPublicKeyV3`]. The 2-byte checksum in
/// `OnionAddressV3` is designed for human-readable part that checks that the
/// address was typed in correctly. In computer-stored digital data it may be
/// deterministically regenerated and does not add any additional security.
///
/// Tor addresses are distinguished by the fact that last 16 bits
/// must be set to 0
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(
    all(feature = "serde", feature = "serde_str_helpers"),
    derive(Serialize, Deserialize),
    serde(
        try_from = "serde_str_helpers::DeserBorrowStr",
        into = "String",
        crate = "serde_crate"
    )
)]
#[cfg_attr(
    all(feature = "serde", not(feature = "serde_str_helpers")),
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
#[non_exhaustive] // Required since we use feature-gated enum variants
pub enum InetAddr {
    /// IP address of V4 standard
    IPv4(Ipv4Addr),

    /// IP address of V6 standard
    IPv6(Ipv6Addr),

    /// Tor address of V3 standard
    #[cfg(feature = "tor")]
    Tor(TorPublicKeyV3),
}

impl PartialOrd for InetAddr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (InetAddr::IPv4(addr1), InetAddr::IPv4(addr2)) => {
                addr1.partial_cmp(addr2)
            }
            (InetAddr::IPv6(addr1), InetAddr::IPv6(addr2)) => {
                addr1.partial_cmp(addr2)
            }
            #[cfg(feature = "tor")]
            (InetAddr::Tor(addr1), InetAddr::Tor(addr2)) => {
                addr1.partial_cmp(addr2)
            }
            (InetAddr::IPv4(_), _) => Some(Ordering::Greater),
            (_, InetAddr::IPv4(_)) => Some(Ordering::Less),
            #[cfg(feature = "tor")]
            (InetAddr::IPv6(_), _) => Some(Ordering::Greater),
            #[cfg(feature = "tor")]
            (_, InetAddr::IPv6(_)) => Some(Ordering::Less),
        }
    }
}

impl Ord for InetAddr {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

// We need this since TorPublicKeyV3 does not implement Hash
#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for InetAddr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            InetAddr::IPv4(ipv4) => ipv4.hash(state),
            InetAddr::IPv6(ipv6) => ipv6.hash(state),
            #[cfg(feature = "tor")]
            InetAddr::Tor(torv3) => torv3.as_bytes().hash(state),
        }
    }
}

impl InetAddr {
    /// Returns an IPv6 address, constructed from IPv4 data; or, if Onion
    /// address is used, [`Option::None`]
    #[inline]
    pub fn to_ipv6(&self) -> Option<Ipv6Addr> {
        match self {
            InetAddr::IPv4(ipv4_addr) => Some(ipv4_addr.to_ipv6_mapped()),
            InetAddr::IPv6(ipv6_addr) => Some(*ipv6_addr),
            #[cfg(feature = "tor")]
            _ => None,
        }
    }

    /// Returns an IPv4 address, if any, or [`Option::None`]
    #[inline]
    pub fn to_ipv4(&self) -> Option<Ipv6Addr> {
        match self {
            InetAddr::IPv4(ipv4_addr) => Some(ipv4_addr.to_ipv6_mapped()),
            InetAddr::IPv6(ipv6_addr) => Some(*ipv6_addr),
            #[cfg(feature = "tor")]
            _ => None,
        }
    }

    /// Determines whether provided address is a Tor address. Always returns
    /// `fales` (the library is built without `tor` feature; use it to
    /// enable Tor addresses).
    #[cfg(not(feature = "tor"))]
    #[inline]
    pub fn is_tor(&self) -> bool { false }

    /// Always returns [`Option::None`] (the library is built without `tor`
    /// feature; use it to enable Tor addresses).
    #[cfg(not(feature = "tor"))]
    #[inline]
    pub fn to_onion_v2(&self) -> Option<()> { None }

    /// Always returns [`Option::None`] (the library is built without `tor`
    /// feature; use it to enable Tor addresses).
    #[cfg(not(feature = "tor"))]
    #[inline]
    pub fn to_onion(&self) -> Option<()> { None }

    /// Determines whether provided address is a Tor address
    #[cfg(feature = "tor")]
    #[inline]
    pub fn is_tor(&self) -> bool { matches!(self, InetAddr::Tor(_)) }

    /// Returns Onion v3 address, if any, or [`Option::None`]
    #[cfg(feature = "tor")]
    #[inline]
    pub fn to_onion(&self) -> Option<OnionAddressV3> {
        match self {
            InetAddr::IPv4(_) | InetAddr::IPv6(_) => None,
            InetAddr::Tor(key) => Some(OnionAddressV3::from(key)),
        }
    }
}

impl Default for InetAddr {
    #[inline]
    fn default() -> Self { InetAddr::IPv4(Ipv4Addr::from(0)) }
}

impl fmt::Display for InetAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InetAddr::IPv4(addr) => write!(f, "{}", addr),
            InetAddr::IPv6(addr) => write!(f, "{}", addr),
            #[cfg(feature = "tor")]
            InetAddr::Tor(addr) => write!(f, "{}", addr),
        }
    }
}

#[cfg(feature = "tor")]
impl TryFrom<InetAddr> for IpAddr {
    type Error = NoOnionSupportError;
    #[inline]
    fn try_from(addr: InetAddr) -> Result<Self, Self::Error> {
        Ok(match addr {
            InetAddr::IPv4(addr) => IpAddr::V4(addr),
            InetAddr::IPv6(addr) => IpAddr::V6(addr),
            #[cfg(feature = "tor")]
            InetAddr::Tor(_) => return Err(NoOnionSupportError),
        })
    }
}

#[cfg(not(feature = "tor"))]
impl From<InetAddr> for IpAddr {
    #[inline]
    fn from(addr: InetAddr) -> Self {
        match addr {
            InetAddr::IPv4(addr) => IpAddr::V4(addr),
            InetAddr::IPv6(addr) => IpAddr::V6(addr),
        }
    }
}

impl From<IpAddr> for InetAddr {
    #[inline]
    fn from(value: IpAddr) -> Self {
        match value {
            IpAddr::V4(v4) => InetAddr::from(v4),
            IpAddr::V6(v6) => InetAddr::from(v6),
        }
    }
}

impl From<Ipv4Addr> for InetAddr {
    #[inline]
    fn from(addr: Ipv4Addr) -> Self { InetAddr::IPv4(addr) }
}

impl From<Ipv6Addr> for InetAddr {
    #[inline]
    fn from(addr: Ipv6Addr) -> Self { InetAddr::IPv6(addr) }
}

#[cfg(feature = "tor")]
impl From<TorPublicKeyV3> for InetAddr {
    #[inline]
    fn from(value: TorPublicKeyV3) -> Self { InetAddr::Tor(value) }
}

#[cfg(feature = "tor")]
impl From<OnionAddressV3> for InetAddr {
    #[inline]
    fn from(addr: OnionAddressV3) -> Self {
        InetAddr::Tor(addr.get_public_key())
    }
}

#[cfg(feature = "stringly_conversions")]
impl_try_from_stringly_standard!(InetAddr);
#[cfg(feature = "stringly_conversions")]
impl_into_stringly_standard!(InetAddr);

impl FromStr for InetAddr {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[cfg(feature = "tor")]
        match (IpAddr::from_str(s), OnionAddressV3::from_str(s)) {
            (Ok(_), Ok(_)) => {
                Err(AddrParseError::WrongAddrFormat(s.to_owned()))
            }
            (Ok(ip_addr), _) => Ok(Self::from(ip_addr)),
            (_, Ok(onionv3)) => Ok(Self::from(onionv3)),
            _ => Err(AddrParseError::WrongAddrFormat(s.to_owned())),
        }

        #[cfg(not(feature = "tor"))]
        match IpAddr::from_str(s) {
            Ok(ip_addr) => Ok(InetAddr::from(ip_addr)),
            _ => Err(AddrParseError::NeedsTorFeature),
        }
    }
}

// Yes, I checked that onion addresses don't need to optimize ownership of input
// String.
#[cfg(feature = "parse_arg")]
impl parse_arg::ParseArgFromStr for InetAddr {
    fn describe_type<W: std::fmt::Write>(mut writer: W) -> std::fmt::Result {
        #[cfg(not(feature = "tor"))]
        {
            write!(writer, "IPv4 or IPv6 address")
        }
        #[cfg(feature = "tor")]
        {
            write!(writer, "IPv4, IPv6, or Tor (onion) address")
        }
    }
}

impl From<[u8; 4]> for InetAddr {
    #[inline]
    fn from(value: [u8; 4]) -> Self { InetAddr::from(Ipv4Addr::from(value)) }
}

impl From<[u8; 16]> for InetAddr {
    #[inline]
    fn from(value: [u8; 16]) -> Self { InetAddr::from(Ipv6Addr::from(value)) }
}

impl From<[u16; 8]> for InetAddr {
    #[inline]
    fn from(value: [u16; 8]) -> Self { InetAddr::from(Ipv6Addr::from(value)) }
}

/// Transport protocols that may be part of [`InetSocketAddrExt`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", rename = "lowercase")
)]
#[non_exhaustive]
#[repr(u8)]
pub enum Transport {
    /// Normal TCP
    #[display("tcp")]
    Tcp = 1,

    /// Normal UDP
    #[display("udp")]
    Udp = 2,

    /// Multipath TCP version
    #[display("mtcp")]
    Mtcp = 3,

    /// More efficient UDP version under developent by Google and consortium of
    /// other internet companies
    #[display("quic")]
    Quic = 4,
    /* There are other rarely used protocols. Do not see any reason to add
     * them to the crate for now, but it may appear in the future,
     * so keeping them for referencing purposes: */
    /*
    UdpLite,
    Sctp,
    Dccp,
    Rudp,
    */
}

impl Default for Transport {
    #[inline]
    fn default() -> Self { Transport::Tcp }
}

impl FromStr for Transport {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "tcp" => Transport::Tcp,
            "udp" => Transport::Udp,
            "mtcp" => Transport::Mtcp,
            "quic" => Transport::Quic,
            _ => {
                return Err(AddrParseError::UnknownProtocolError(s.to_owned()))
            }
        })
    }
}

/// Internet socket address, which consists of [`InetAddr`] IP or Tor address
/// and a port number (without protocol specification, i.e. TCP/UDP etc). If you
/// need to include transport-level protocol information into the socket
/// details, pls check [`InetSocketAddrExt`]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(
    all(feature = "serde", feature = "serde_str_helpers"),
    derive(Serialize, Deserialize),
    serde(
        try_from = "serde_str_helpers::DeserBorrowStr",
        into = "String",
        crate = "serde_crate"
    )
)]
#[cfg_attr(
    all(feature = "serde", not(feature = "serde_str_helpers")),
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub struct InetSocketAddr {
    /// Address part of the socket
    pub address: InetAddr,

    /// Port of the socket
    pub port: u16,
}

#[cfg(feature = "stringly_conversions")]
impl_try_from_stringly_standard!(InetSocketAddr);
#[cfg(feature = "stringly_conversions")]
impl_into_stringly_standard!(InetSocketAddr);

impl InetSocketAddr {
    /// Constructs new socket address from an internet address and a port
    /// information
    #[inline]
    pub fn new(address: InetAddr, port: u16) -> Self { Self { address, port } }

    /// Determines whether provided address is a Tor address
    #[inline]
    pub fn is_tor(&self) -> bool { self.address.is_tor() }
}

impl fmt::Display for InetSocketAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.address, self.port)
    }
}

impl FromStr for InetSocketAddr {
    type Err = AddrParseError;

    #[allow(unreachable_code)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(socket_addr) = SocketAddrV6::from_str(s) {
            return Ok(Self::new(
                (*socket_addr.ip()).into(),
                socket_addr.port(),
            ));
        } else if let Ok(socket_addr) = SocketAddrV4::from_str(s) {
            return Ok(Self::new(
                (*socket_addr.ip()).into(),
                socket_addr.port(),
            ));
        } else {
            #[cfg(not(feature = "tor"))]
            return Err(AddrParseError::NeedsTorFeature);
        }

        let mut vals = s.split(':');
        match (vals.next(), vals.next(), vals.next()) {
            (Some(addr), Some(port), None) => Ok(Self {
                address: addr.parse()?,
                port: u16::from_str(port)?,
            }),
            (Some(addr), None, _) => Ok(Self {
                address: addr.parse()?,
                port: 0,
            }),
            _ => Err(AddrParseError::WrongSocketFormat(s.to_owned())),
        }
    }
}

#[cfg(feature = "tor")]
impl TryFrom<InetSocketAddr> for SocketAddr {
    type Error = NoOnionSupportError;
    #[inline]
    fn try_from(socket_addr: InetSocketAddr) -> Result<Self, Self::Error> {
        Ok(Self::new(
            IpAddr::try_from(socket_addr.address)?,
            socket_addr.port,
        ))
    }
}

#[cfg(not(feature = "tor"))]
impl From<InetSocketAddr> for SocketAddr {
    #[inline]
    fn from(socket_addr: InetSocketAddr) -> Self {
        Self::new(IpAddr::from(socket_addr.address), socket_addr.port)
    }
}

impl From<SocketAddr> for InetSocketAddr {
    #[inline]
    fn from(addr: SocketAddr) -> Self {
        Self::new(addr.ip().into(), addr.port())
    }
}

impl From<SocketAddrV4> for InetSocketAddr {
    #[inline]
    fn from(addr: SocketAddrV4) -> Self {
        Self::new((*addr.ip()).into(), addr.port())
    }
}

impl From<SocketAddrV6> for InetSocketAddr {
    #[inline]
    fn from(addr: SocketAddrV6) -> Self {
        Self::new((*addr.ip()).into(), addr.port())
    }
}

/// Internet socket address of [`InetSocketAddr`] type, extended with a
/// transport-level protocol information (see [`Transport`])
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(
    all(feature = "serde", feature = "serde_str_helpers"),
    derive(Serialize, Deserialize),
    serde(
        try_from = "serde_str_helpers::DeserBorrowStr",
        into = "String",
        crate = "serde_crate"
    )
)]
#[cfg_attr(
    all(feature = "serde", not(feature = "serde_str_helpers")),
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub struct InetSocketAddrExt(
    /// Transport-level protocol details (like TCP, UDP etc)
    pub Transport,
    /// Details of the socket address, i.e internet address and port
    /// information
    pub InetSocketAddr,
);

#[cfg(feature = "stringly_conversions")]
impl_try_from_stringly_standard!(InetSocketAddrExt);
#[cfg(feature = "stringly_conversions")]
impl_into_stringly_standard!(InetSocketAddrExt);

impl InetSocketAddrExt {
    /// Constructs [`InetSocketAddrExt`] for a given internet address and TCP
    /// port
    #[inline]
    pub fn tcp(address: InetAddr, port: u16) -> Self {
        Self(Transport::Tcp, InetSocketAddr::new(address, port))
    }

    /// Constructs [`InetSocketAddrExt`] for a given internet address and UDP
    /// port
    #[inline]
    pub fn udp(address: InetAddr, port: u16) -> Self {
        Self(Transport::Udp, InetSocketAddr::new(address, port))
    }
}

impl fmt::Display for InetSocketAddrExt {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.0, self.1)
    }
}

impl FromStr for InetSocketAddrExt {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut vals = s.split("://");
        if let (Some(transport), Some(addr), None) =
            (vals.next(), vals.next(), vals.next())
        {
            Ok(Self(transport.parse()?, addr.parse()?))
        } else {
            Err(AddrParseError::WrongSocketExtFormat(s.to_owned()))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // TODO: Add tests for Tor

    #[test]
    fn test_inet_addr() {
        let ip4a = "127.0.0.1".parse().unwrap();
        let ip6a = "::1".parse().unwrap();

        let ip4 = InetAddr::IPv4(ip4a);
        let ip6 = InetAddr::IPv6(ip6a);
        assert_eq!(
            ip4.to_ipv6().unwrap(),
            Ipv6Addr::from_str("::ffff:127.0.0.1").unwrap()
        );
        assert_eq!(ip6.to_ipv6().unwrap(), ip6a);
        assert_eq!(InetAddr::from(IpAddr::V4(ip4a)), ip4);
        assert_eq!(InetAddr::from(IpAddr::V6(ip6a)), ip6);
        assert_eq!(InetAddr::from(ip4a), ip4);
        assert_eq!(InetAddr::from(ip6a), ip6);

        assert_eq!(InetAddr::default(), InetAddr::from_str("0.0.0.0").unwrap());

        #[cfg(feature = "tor")]
        assert_eq!(IpAddr::try_from(ip4).unwrap(), IpAddr::V4(ip4a));
        #[cfg(feature = "tor")]
        assert_eq!(IpAddr::try_from(ip6).unwrap(), IpAddr::V6(ip6a));

        #[cfg(not(feature = "tor"))]
        assert_eq!(IpAddr::from(ip4.clone()), IpAddr::V4(ip4a));
        #[cfg(not(feature = "tor"))]
        assert_eq!(IpAddr::from(ip6.clone()), IpAddr::V6(ip6a));

        assert_eq!(InetAddr::from_str("127.0.0.1").unwrap(), ip4);
        assert_eq!(InetAddr::from_str("::1").unwrap(), ip6);
        assert_eq!(format!("{}", ip4), "127.0.0.1");
        assert_eq!(format!("{}", ip6), "::1");

        assert!(!ip4.is_tor());
        assert!(!ip6.is_tor());
    }

    #[test]
    fn test_transport() {
        assert_eq!(format!("{}", Transport::Tcp), "tcp");
        assert_eq!(format!("{}", Transport::Udp), "udp");
        assert_eq!(format!("{}", Transport::Quic), "quic");
        assert_eq!(format!("{}", Transport::Mtcp), "mtcp");

        assert_eq!(Transport::from_str("tcp").unwrap(), Transport::Tcp);
        assert_eq!(Transport::from_str("Tcp").unwrap(), Transport::Tcp);
        assert_eq!(Transport::from_str("TCP").unwrap(), Transport::Tcp);
        assert_eq!(Transport::from_str("udp").unwrap(), Transport::Udp);
        assert_eq!(Transport::from_str("quic").unwrap(), Transport::Quic);
        assert_eq!(Transport::from_str("mtcp").unwrap(), Transport::Mtcp);
        assert!(Transport::from_str("xtp").is_err());
    }

    #[test]
    fn test_inet_socket_addr() {
        let ip4a = "127.0.0.1".parse().unwrap();
        let ip6a = "::1".parse().unwrap();
        let socket4a = "127.0.0.1:6865".parse().unwrap();
        let socket6a = "[::1]:6865".parse().unwrap();

        let ip4 = InetSocketAddr::new(ip4a, 6865);
        let ip6 = InetSocketAddr::new(ip6a, 6865);
        assert_eq!(InetSocketAddr::from(SocketAddr::V4(socket4a)), ip4);
        assert_eq!(InetSocketAddr::from(SocketAddr::V6(socket6a)), ip6);
        assert_eq!(InetSocketAddr::from(socket4a), ip4);
        assert_eq!(InetSocketAddr::from(socket6a), ip6);

        assert_eq!(
            InetSocketAddr::default(),
            InetSocketAddr::from_str("0.0.0.0:0").unwrap()
        );

        #[cfg(feature = "tor")]
        assert_eq!(
            SocketAddr::try_from(ip4).unwrap(),
            SocketAddr::V4(socket4a)
        );
        #[cfg(feature = "tor")]
        assert_eq!(
            SocketAddr::try_from(ip6).unwrap(),
            SocketAddr::V6(socket6a)
        );

        #[cfg(not(feature = "tor"))]
        assert_eq!(SocketAddr::from(ip4.clone()), SocketAddr::V4(socket4a));
        #[cfg(not(feature = "tor"))]
        assert_eq!(SocketAddr::from(ip6.clone()), SocketAddr::V6(socket6a));

        assert_eq!(InetSocketAddr::from_str("127.0.0.1:6865").unwrap(), ip4);
        assert_eq!(InetSocketAddr::from_str("[::1]:6865").unwrap(), ip6);
        assert_eq!(format!("{}", ip4), "127.0.0.1:6865");
        assert_eq!(format!("{}", ip6), "::1:6865");

        assert!(!ip4.is_tor());
        assert!(!ip6.is_tor());
    }

    #[test]
    fn test_inet_socket_addr_ext() {
        let ip4a = "127.0.0.1".parse().unwrap();
        let ip6a = "::1".parse().unwrap();

        let ip4 = InetSocketAddrExt::tcp(ip4a, 6865);
        let ip6 = InetSocketAddrExt::udp(ip6a, 6865);

        assert_eq!(
            InetSocketAddrExt::default(),
            InetSocketAddrExt::from_str("tcp://0.0.0.0:0").unwrap()
        );

        #[cfg(feature = "tor")]
        assert_eq!(
            InetSocketAddrExt::from_str("tcp://127.0.0.1:6865").unwrap(),
            ip4
        );
        #[cfg(feature = "tor")]
        assert_eq!(
            InetSocketAddrExt::from_str("udp://[::1]:6865").unwrap(),
            ip6
        );
        assert_eq!(format!("{}", ip4), "tcp://127.0.0.1:6865");
        assert_eq!(format!("{}", ip6), "udp://::1:6865");
    }
}
