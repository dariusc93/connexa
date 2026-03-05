/// Error that is sharable between threads
#[derive(Clone)]
pub struct ArcError<E> {
    inner_error: std::sync::Arc<E>,
}

impl<E> From<E> for ArcError<E> {
    fn from(inner_error: E) -> Self {
        Self::new(inner_error)
    }
}

impl<E> ArcError<E> {
    pub fn new(inner_error: E) -> Self {
        Self {
            inner_error: std::sync::Arc::new(inner_error),
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Debug for ArcError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner_error, f)
    }
}

impl<E: std::fmt::Display> std::fmt::Display for ArcError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner_error, f)
    }
}

impl<E: std::error::Error> std::error::Error for ArcError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner_error.source()
    }
}

#[cfg(feature = "rendezvous")]
pub mod rendezvous {
    // Note: this for creating an Error representing `ErrorCode` since `ErrorCode` does not impl `Error`.
    // Ideally, this is to provide context for the error response, but may be used in the future for
    // a custom Error implementation

    use libp2p::rendezvous::ErrorCode;
    use std::error::Error as StdError;
    use std::fmt::Display;

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum Error {
        InvalidNamespace,
        InvalidSignedPeerRecord,
        InvalidTtl,
        InvalidCookie,
        NotAuthorized,
        InternalError,
        Unavailable,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Error::InvalidNamespace => write!(f, "Invalid namespace"),
                Error::InvalidSignedPeerRecord => write!(f, "Invalid signed peer record"),
                Error::InvalidTtl => write!(f, "Invalid ttl"),
                Error::InvalidCookie => write!(f, "Invalid cookie"),
                Error::NotAuthorized => write!(f, "Not authorized"),
                Error::InternalError => write!(f, "Internal error"),
                Error::Unavailable => write!(f, "Unavailable"),
            }
        }
    }

    impl StdError for Error {}

    impl From<ErrorCode> for Error {
        fn from(code: ErrorCode) -> Self {
            match code {
                ErrorCode::InvalidNamespace => Error::InvalidNamespace,
                ErrorCode::InvalidSignedPeerRecord => Error::InvalidSignedPeerRecord,
                ErrorCode::InvalidTtl => Error::InvalidTtl,
                ErrorCode::InvalidCookie => Error::InvalidCookie,
                ErrorCode::NotAuthorized => Error::NotAuthorized,
                ErrorCode::InternalError => Error::InternalError,
                ErrorCode::Unavailable => Error::Unavailable,
            }
        }
    }
}
