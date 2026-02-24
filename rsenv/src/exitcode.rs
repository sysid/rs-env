//! Standard exit codes (BSD sysexits.h compatible)

/// Successful termination
pub const OK: i32 = 0;

/// Project not rsenv-managed (no vault found)
pub const UNMANAGED: i32 = 2;

/// Command line usage error
pub const USAGE: i32 = 64;

/// Data format error
pub const DATAERR: i32 = 65;

/// Cannot open input
pub const NOINPUT: i32 = 66;

/// Service unavailable
pub const UNAVAILABLE: i32 = 69;

/// Internal software error
pub const SOFTWARE: i32 = 70;

/// System error (e.g., can't fork)
pub const OSERR: i32 = 71;

/// Can't create output file
pub const CANTCREAT: i32 = 73;

/// Input/output error
pub const IOERR: i32 = 74;

/// Permission denied
pub const NOPERM: i32 = 77;

/// Configuration error
pub const CONFIG: i32 = 78;
