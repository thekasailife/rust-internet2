// LNP/BP Core Library implementing LNPBP specifications & standards
// Written in 2020 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

//! LNP transport level works with framed messages of defined size. This
//! messages can be put into different underlying transport protocols, including
//! streaming protocols (like TCP), or overlaid over application-level
//! protocols like HTTP, Websockets, SMTP (for high-latency communication
//! networks). Current mod implements such overlays and provides TCP with
//! the required framing functionality (this variant is called FTCP). It also
//! integrates with ZMQ such that the upper level can abstract for a particular
//! transport protocol used.

mod addr;
pub mod ftcp;
pub mod websocket;
#[cfg(feature = "zmq")]
pub mod zmqsocket;

pub use addr::{FramingProtocol, LocalAddr, RemoteAddr};
use tokio::io::ErrorKind;
#[cfg(feature = "zmq")]
pub use zmqsocket::ZMQ_CONTEXT;

/// Maximum size of the transport frame; chosen in compliance with LN specs
pub const MAX_FRAME_SIZE: usize =
    FRAME_PREFIX_SIZE + MAX_FRAME_PAYLOAD_SIZE + FRAME_SUFFIX_SIZE;

/// Size of the frame prefix which is not included into payload size, consisting
/// of the 2-bytes message size data and 16-byte MAC of the payload length
pub const FRAME_PREFIX_SIZE: usize = 2 + 16;

/// Size of the frame suffix represented by a 16-byte MAC of the frame payload
pub const FRAME_SUFFIX_SIZE: usize = 16;

/// Maximum size of the frame payload which may be expressed by two bytes
pub const MAX_FRAME_PAYLOAD_SIZE: usize = 0xFFFF;

/// Transport protocol-level errors
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum Error {
    /// I/O socket error, generated by underlying socket implementation
    /// (POSIX or TCP). Error type is {_0:?}
    #[from]
    SocketIo(std::io::ErrorKind),

    /// ZMQ socket error, type {_0}
    #[cfg(feature = "zmq")]
    Zmq(zmqsocket::Error),

    /// Service is offline or not responding
    ServiceOffline,

    /// The function requires that the connecting socket must be present on the
    /// the same machine, i.e. it should be a raw POSIX socket or IPC & Inproc
    /// ZMQ socket
    RequiresLocalSocket,

    /// The provided frame size ({_0}) exceeds frame size limit of
    /// MAX_FRAME_SIZE bytes
    OversizedFrame(usize),

    /// Frame size {_0} is less than minimal (34 bytes)
    FrameTooSmall(usize),

    /// Frame structure broken: {_0}
    FrameBroken(&'static str),

    /// Frame payload length is not equal to the actual frame payload provided
    InvalidLength,

    /// Connections over Tor protocol are not yet supported
    TorNotSupportedYet,

    /// Read or write attempt exceeded socket timeout
    TimedOut,
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        match err.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => Error::TimedOut,
            kind => Error::SocketIo(kind),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RoutedFrame {
    pub src: Vec<u8>,
    pub dst: Vec<u8>,
    pub msg: Vec<u8>,
}

/// Marker trait for types that can provide a concrete implementation for both
/// frame parser implementing [`RecvFrame`] and frame composer implementing
/// [`SendFrame`]. These types must also implement [`Bipolar`], i.e. they must
/// be splittable into the receiving and sending half-types.
///
/// Any type implementing both [`AsReceiver`] and [`AsSender`], plust providing
/// [`Bipolar`] trait implementation has a blanket implementation of this trait
pub trait Duplex {
    fn as_receiver(&mut self) -> &mut dyn RecvFrame;
    fn as_sender(&mut self) -> &mut dyn SendFrame;
    fn split(self) -> (Box<dyn RecvFrame + Send>, Box<dyn SendFrame + Send>);
}

/// Frame receiving type which is able to parse raw data (streamed or framed by
/// an underlying overlaid protocol such as ZMQ, HTTP, Websocket).
///
/// For asynchronous version check [`AsyncRecvFrame`]
pub trait RecvFrame {
    /// Receive a single frame of data structured as a byte string. The frame
    /// contains LNP framing prefix, which is used by upstream session-level
    /// protocols.
    ///
    /// # Errors
    /// Returns only [`Error::SocketError`] if the overlaid protocol errors with
    /// I/O error type
    fn recv_frame(&mut self) -> Result<Vec<u8>, Error>;

    /// Receive `len` number of bytes and pack them as a frame. Should be used
    /// with caution!
    ///
    /// # Errors
    /// Returns only [`Error::SocketError`] if the overlaid protocol errors with
    /// I/O error type
    fn recv_raw(&mut self, len: usize) -> Result<Vec<u8>, Error>;

    /// Receive frame like with [`RecvFrame::recv_frame`], but only originating
    /// from the specified remote address.
    ///
    /// # Returns
    /// Tuple, consisting of two byte strings:
    /// * Received frame
    /// * Source of the frame (i.e. some id of the remote node that sent this
    ///   frame). The id is specific for the underlying overlaid protocol.
    ///
    /// # Errors
    /// Returns only [`Error::SocketError`] if the overlaid protocol errors with
    /// I/O error type
    ///
    /// # Panics
    /// Default implementation panics, since most of the framing protocols do
    /// not support multipeer sockets and [`RecFrame::recv_frame`] must be
    /// used instead (currently only ZMQ-based connections support this
    /// operation)
    fn recv_routed(&mut self) -> Result<RoutedFrame, Error> {
        // We panic here because this is a program architecture design
        // error and developer must be notified about it; the program using
        // this pattern can't work
        panic!("Multipeer sockets are not possible with the chosen transport")
    }
}

/// Frame sending type which is able to compose frame with a given raw data and
/// send it via an underlying overlaid protocol such as ZMQ, HTTP, Websocket.
///
/// For asynchronous version check [`AsyncSendFrame`]
pub trait SendFrame {
    /// Sends a single frame of data structured as a byte string. The frame must
    /// already contain LNP framing prefix with size data. The function must
    /// check that the provided data frame length is below the limit defined
    /// with [`MAX_FRAME_SIZE`] constant.
    ///
    /// # Returns
    /// In case of success, number of bytes send (NB: this is larger than the
    /// message payload size and is equal to the size of the provided `frame`
    /// argument)
    ///
    /// # Errors
    /// * [`Error::SocketError`] if the overlaid protocol errors with I/O error
    ///   type
    /// * [`Error::OversizedFrame`] if the provided data length exceeds
    ///   [`MAX_FRAME_SIZE`]
    ///
    /// [`MAX_FRAME_SIZE`]: super::MAX_FRAME_SIZE
    // We can't use `impl AsRev<[u8]>` here since with it the trait can't be
    // made into an object
    fn send_frame(&mut self, frame: &[u8]) -> Result<usize, Error>;

    /// Sends a single frame of data structured as a byte string. The frame must
    /// already contain LNP framing prefix with size data.
    ///
    /// NB: Unlike [`SendFrame::send_frame`], this function **does not** check
    /// that the provided data frame length is below the limit defined with
    /// [`MAX_FRAME_SIZE`] constant.
    ///
    /// # Returns
    /// In case of success, number of bytes send (NB: this is larger than the
    /// message payload size and is equal to the size of the provided
    /// `raw_frame` argument)
    ///
    /// # Errors
    /// * [`Error::SocketError`] if the overlaid protocol errors with I/O error
    ///   type
    ///
    /// [`MAX_FRAME_SIZE`]: super::MAX_FRAME_SIZE
    fn send_raw(&mut self, raw_frame: &[u8]) -> Result<usize, Error>;

    /// Sends a single frame of data structured as a byte string to a specific
    /// receiver with `remote_id`. Function works like [`RecvFrame::recv_frame`]
    /// and is used for the underlying protocols supporting multipeer
    /// connectivity. The frame must already contain LNP framing prefix with
    /// size data. The function must check that the provided data frame
    /// length is below the limit defined with [`MAX_FRAME_SIZE`] constant.
    ///
    /// # Returns
    /// In case of success, number of bytes send (NB: this is larger than the
    /// message payload size and is equal to the size of the provided `frame`
    /// argument)
    ///
    /// # Errors
    /// * [`Error::SocketError`] if the overlaid protocol errors with I/O error
    ///   type
    /// * [`Error::OversizedFrame`] if the provided data length exceeds
    ///   [`MAX_FRAME_SIZE`]
    ///
    /// # Panics
    /// Default implementation panics, since the most of framing protocols do
    /// not support multipeer sockets and [`SendFrame::send_frame`] must be
    /// used instead (currently only ZMQ-based connections support this
    /// operation)
    ///
    /// [`MAX_FRAME_SIZE`]: super::MAX_FRAME_SIZE
    #[allow(dead_code)]
    fn send_routed(
        &mut self,
        route: &[u8],
        address: &[u8],
        data: &[u8],
    ) -> Result<usize, Error> {
        // We panic here because this is a program architecture design
        // error and developer must be notified about it; the program using
        // this pattern can't work
        panic!("Multipeer sockets are not possible with the chosen transport")
    }
}

/// Async version of [`RecvFrame`] trait
#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncRecvFrame {
    /// Async version of [`RecvFrame::recv_frame`]; pls refer to it for the
    /// function documentation
    async fn async_recv_frame(&mut self) -> Result<Vec<u8>, Error>;

    /// Async version of [`RecvFrame::recv_raw`]; pls refer to it for the
    /// function documentation
    async fn async_recv_raw(&mut self, len: usize) -> Result<Vec<u8>, Error>;

    /// Async version of [`RecvFrame::recv_from`]; pls refer to it for the
    /// function documentation
    async fn async_recv_from(&mut self) -> Result<(Vec<u8>, Vec<u8>), Error> {
        // We panic here because this is a program architecture design
        // error and developer must be notified about it; the program using
        // this pattern can't work
        panic!("Multipeer sockets are not possible with the chosen transport")
    }
}

/// Async version of [`SendFrame`] trait
#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncSendFrame {
    /// Async version of [`SendFrame::send_frame`]; pls refer to it for the
    /// function documentation
    async fn async_send_frame(&mut self, frame: &[u8]) -> Result<usize, Error>;

    /// Async version of [`RecvFrame::send_raw`]; pls refer to it for the
    /// function documentation
    async fn async_send_raw(
        &mut self,
        raw_frame: &[u8],
    ) -> Result<usize, Error>;

    /// Async version of [`RecvFrame::send_to`]; pls refer to it for the
    /// function documentation
    async fn async_send_to(
        &mut self,
        remote_id: &[u8],
        frame: &[u8],
    ) -> Result<usize, Error> {
        // We panic here because this is a program architecture design
        // error and developer must be notified about it; the program using
        // this pattern can't work
        panic!("Multipeer sockets are not possible with the chosen transport")
    }
}
