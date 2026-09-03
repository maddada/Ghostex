pub mod authorized_keys;
pub mod http;
pub mod identity;
pub mod pair_device;
pub mod paired_devices;
pub mod pairing_code;
pub mod probe_cache;
pub mod repository;
pub mod ssh_enable;
pub mod ssh_status;
pub mod tailscale;

// `http` exposes only the crate-internal endpoint handler, so its re-export
// matches that visibility rather than widening it to `pub`.
pub use authorized_keys::*;
pub(crate) use http::*;
pub use identity::*;
pub use pair_device::*;
pub use paired_devices::*;
pub use pairing_code::*;
pub use probe_cache::*;
pub use repository::*;
pub use ssh_enable::*;
pub use ssh_status::*;
pub use tailscale::*;
