//! The sizes each provider offers and Ghostex's about-$ per running hour for them (the picker
//! footer's "≈ $/h", the chip's estimate).

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
        // About-$ from the 2026-11-02 research page (4 vCPU / 8 GiB ≈ $0.034/h).
        // CDXC:CloudBoxes 2026-11-21 WHY: an Instabox sandbox has one fixed size; a create asking
        // for 2 CPU · 4 GB was refused ("sandbox sizes are fixed: this sandbox gets 4 vCPU, 8 GB
        // memory…") after `ghostex cloud sizes` and the pickers had offered it. Only that size is
        // listed, and asking for another is refused before a box is made (`refused_size`).
        CloudProviderId::Instabox => vec![size(CloudProviderId::Instabox, 4, 8, 0.034, true)],
        // Boat's list prices (docs.boat.dev/pricing, 2026-11-02): small, default, large, xlarge.
        CloudProviderId::Boat => vec![
            size(CloudProviderId::Boat, 2, 4, 0.018, false),
            size(CloudProviderId::Boat, 4, 8, 0.036, true),
            size(CloudProviderId::Boat, 8, 16, 0.072, false),
            size(CloudProviderId::Boat, 16, 32, 0.200, false),
        ],
        CloudProviderId::Other(_) => Vec::new(),
    }
}

/// A provider's size by id, or its default when `id` is absent or unknown.
pub fn size_or_default(provider: &CloudProviderId, id: Option<&str>) -> Option<CloudBoxSize> {
    let all = sizes(provider);
    id.and_then(|id| all.iter().find(|size| size.id == id).cloned())
        .or_else(|| all.iter().find(|size| size.is_default).cloned())
}

/// Why a size someone asked for by name can't be had at `provider`, or `None` when it's offered.
/// (A size saved earlier on a snapshot or a project still falls back to the default.)
pub fn refused_size(provider: &CloudProviderId, id: &str) -> Option<String> {
    let all = sizes(provider);
    if all.is_empty() || all.iter().any(|size| size.id == id) {
        return None;
    }
    Some(match (provider, all.as_slice()) {
        (CloudProviderId::Instabox, [only]) => format!(
            "Instabox: sandbox sizes are fixed: every box gets {}; leave the size out.",
            only.label
        ),
        _ => format!(
            "There is no {id} size here; the sizes are {}.",
            all.iter()
                .map(|size| size.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

/// About-$ for `seconds` of running at `usd_per_hour`.
pub fn estimate_usd(seconds: u64, usd_per_hour: f64) -> f64 {
    (seconds as f64 / 3600.0) * usd_per_hour
}
