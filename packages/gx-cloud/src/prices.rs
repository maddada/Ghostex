//! The sizes each provider offers and Ghostex's about-$ per running hour for them (the picker
//! footer's "≈ $/h", the chip's estimate). Instabox and Boat rows arrive with their adapters.

use ghostex_gx_protocol::cloud_box::{CloudBoxSize, CloudProviderId};

fn size(
    provider: CloudProviderId,
    vcpus: u64,
    mem_gib: u64,
    usd_per_hour: f64,
    is_default: bool,
) -> CloudBoxSize {
    CloudBoxSize {
        id: CloudBoxSize::id_for(vcpus, mem_gib * 1024),
        provider,
        vcpus,
        mem_mib: mem_gib * 1024,
        label: format!("{vcpus} CPU · {mem_gib} GB"),
        usd_per_hour: Some(usd_per_hour),
        is_default,
    }
}

/// The sizes of one provider, default first.
pub fn sizes(provider: &CloudProviderId) -> Vec<CloudBoxSize> {
    match provider {
        // Docker limits only; it costs nothing.
        CloudProviderId::Local => vec![
            size(CloudProviderId::Local, 2, 4, 0.0, true),
            size(CloudProviderId::Local, 4, 8, 0.0, false),
        ],
        _ => Vec::new(),
    }
}

/// A provider's size by id, or its default when `id` is absent or unknown.
pub fn size_or_default(provider: &CloudProviderId, id: Option<&str>) -> Option<CloudBoxSize> {
    let all = sizes(provider);
    id.and_then(|id| all.iter().find(|size| size.id == id).cloned())
        .or_else(|| all.iter().find(|size| size.is_default).cloned())
}

/// About-$ for `seconds` of running at `usd_per_hour`.
pub fn estimate_usd(seconds: u64, usd_per_hour: f64) -> f64 {
    (seconds as f64 / 3600.0) * usd_per_hour
}
