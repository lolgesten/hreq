use crate::h1;
use std::fmt;
use std::io;

#[cfg(feature = "tls")]
use rustls::TLSError;
#[cfg(feature = "tls")]
use webpki::InvalidDNSNameError;

#[derive(Debug)]
pub enum Error {
    User(String),
    Proto(String),
    Io(io::Error),
    Http11Parser(httparse::Error),
    H2(h2::Error),
    Http(http::Error),
    #[cfg(feature = "tls")]
    TlsError(TLSError),
    #[cfg(feature = "tls")]
    DnsName(InvalidDNSNameError),
}

impl Error {
    pub fn is_io(&self) -> bool {
        match self {
            Error::Io(_) => true,
            _ => false,
        }
    }

    pub fn into_io(self) -> Option<io::Error> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }

    pub fn is_timeout(&self) -> bool {
        if let Error::Io(e) = self {
            if e.kind() == io::ErrorKind::TimedOut {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            Error::Io(e) => match e.kind() {
                io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::Interrupted => true,
                _ => false,
            },
            _ => false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::User(v) => write!(f, "{}", v),
            Error::Proto(v) => write!(f, "proto: {}", v),
            Error::Io(v) => fmt::Display::fmt(v, f),
            Error::Http11Parser(v) => write!(f, "http11 parser: {}", v),
            Error::H2(v) => write!(f, "http2: {}", v),
            Error::Http(v) => write!(f, "http api: {}", v),
            #[cfg(feature = "tls")]
            Error::TlsError(v) => write!(f, "tls: {}", v),
            #[cfg(feature = "tls")]
            Error::DnsName(v) => write!(f, "dns name: {}", v),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<h1::Error> for Error {
    fn from(e: h1::Error) -> Self {
        match e {
            h1::Error::User(v) => Error::User(v),
            h1::Error::Proto(v) => Error::Proto(v),
            h1::Error::Io(v) => Error::Io(v),
            h1::Error::Http11Parser(v) => Error::Http11Parser(v),
            h1::Error::Http(v) => Error::Http(v),
        }
    }
}

impl From<h2::Error> for Error {
    fn from(e: h2::Error) -> Self {
        if e.is_io() {
            Error::Io(e.into_io().unwrap())
        } else {
            Error::H2(e)
        }
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::Http(e)
    }
}

#[cfg(feature = "tls")]
impl From<TLSError> for Error {
    fn from(e: TLSError) -> Self {
        Error::TlsError(e)
    }
}

#[cfg(feature = "tls")]
impl From<InvalidDNSNameError> for Error {
    fn from(e: InvalidDNSNameError) -> Self {
        Error::DnsName(e)
    }
}
