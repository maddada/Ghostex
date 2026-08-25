use std::{fs, io::Read, io::Write, path::Path, time::Duration};

use uuid::Uuid;

use super::{
    validate_sha256, ExtensionCatalog, ExtensionCatalogSnapshot, ExtensionCatalogSource,
    ExtensionError, ExtensionResult,
};

pub(crate) const DEFAULT_EXTENSIONS_CATALOG_URL: &str =
    "https://github.com/maddada/Ghostex-extensions/releases/download/store/catalog.json";
const CATALOG_CACHE_FILE: &str = "catalog-cache.json";
const CATALOG_LIMIT_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn catalog_url() -> String {
    std::env::var("GHOSTEX_EXTENSIONS_CATALOG_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_EXTENSIONS_CATALOG_URL.to_string())
}

pub(crate) fn fetch_catalog(extensions_dir: &Path) -> ExtensionResult<ExtensionCatalogSnapshot> {
    let url = catalog_url();
    match fetch_remote_catalog(&url) {
        Ok(catalog) => {
            write_catalog_cache(extensions_dir, &catalog)?;
            Ok(ExtensionCatalogSnapshot {
                catalog,
                source: ExtensionCatalogSource::Remote,
                url,
            })
        }
        Err(remote_error) => {
            let cache_path = extensions_dir.join(CATALOG_CACHE_FILE);
            let bytes = fs::read(&cache_path).map_err(|cache_error| {
                ExtensionError::internal(format!(
                    "Could not fetch extension catalog from {url}: {remote_error}. No last-good cache was available at {}: {cache_error}",
                    cache_path.display()
                ))
            })?;
            let catalog = parse_catalog(&bytes).map_err(|cache_error| {
                ExtensionError::internal(format!(
                    "Could not fetch extension catalog from {url}: {remote_error}. The last-good cache at {} is invalid: {cache_error}",
                    cache_path.display()
                ))
            })?;
            Ok(ExtensionCatalogSnapshot {
                catalog,
                source: ExtensionCatalogSource::Cache,
                url,
            })
        }
    }
}

pub(crate) fn catalog_zip_url(catalog_url: &str, zip_path: &str) -> ExtensionResult<String> {
    let zip_path = Path::new(zip_path);
    if zip_path.as_os_str().is_empty()
        || zip_path.is_absolute()
        || zip_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ExtensionError::bad_request(
            "Extension catalog zip path must stay inside the catalog release.",
        ));
    }
    let base = url::Url::parse(catalog_url).map_err(|error| {
        ExtensionError::bad_request(format!("Invalid extension catalog URL: {error}"))
    })?;
    let zip_path = zip_path.to_str().ok_or_else(|| {
        ExtensionError::bad_request("Extension catalog zip path is not valid UTF-8.")
    })?;
    let resolved = base.join(zip_path).map_err(|error| {
        ExtensionError::bad_request(format!("Invalid extension zip URL: {error}"))
    })?;
    if !matches!(resolved.scheme(), "http" | "https") {
        return Err(ExtensionError::bad_request(
            "Extension zip URL must use HTTP or HTTPS.",
        ));
    }
    Ok(resolved.to_string())
}

fn fetch_remote_catalog(url: &str) -> ExtensionResult<ExtensionCatalog> {
    let parsed = url::Url::parse(url)
        .map_err(|error| ExtensionError::bad_request(format!("Invalid catalog URL: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ExtensionError::bad_request(
            "Extension catalog URL must use HTTP or HTTPS.",
        ));
    }
    let response = ureq::get(url)
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|error| {
            ExtensionError::internal(format!("Extension catalog request failed: {error}"))
        })?;
    let mut reader = response.into_reader().take(CATALOG_LIMIT_BYTES + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not read extension catalog response: {error}"
        ))
    })?;
    if bytes.len() as u64 > CATALOG_LIMIT_BYTES {
        return Err(ExtensionError::bad_request(format!(
            "Extension catalog exceeds the {} byte limit.",
            CATALOG_LIMIT_BYTES
        )));
    }
    parse_catalog(&bytes)
}

fn parse_catalog(bytes: &[u8]) -> ExtensionResult<ExtensionCatalog> {
    let catalog: ExtensionCatalog = serde_json::from_slice(bytes).map_err(|error| {
        ExtensionError::bad_request(format!("Extension catalog is not valid JSON: {error}"))
    })?;
    if catalog.schema_version != 1 {
        return Err(ExtensionError::bad_request(format!(
            "Unsupported extension catalog schema version {}.",
            catalog.schema_version
        )));
    }
    let mut ids = std::collections::BTreeSet::new();
    for entry in &catalog.extensions {
        entry.manifest.validate(None, None)?;
        if !ids.insert(entry.manifest.name.as_str()) {
            return Err(ExtensionError::bad_request(format!(
                "Extension catalog contains duplicate id {:?}.",
                entry.manifest.name
            )));
        }
        validate_sha256(&entry.sha256)?;
        if entry.zip.trim().is_empty()
            || entry.readme.trim().is_empty()
            || entry.changelog.trim().is_empty()
        {
            return Err(ExtensionError::bad_request(format!(
                "Extension catalog entry {} is missing store metadata.",
                entry.manifest.name
            )));
        }
    }
    Ok(catalog)
}

fn write_catalog_cache(extensions_dir: &Path, catalog: &ExtensionCatalog) -> ExtensionResult<()> {
    fs::create_dir_all(extensions_dir).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not create extensions directory {}: {error}",
            extensions_dir.display()
        ))
    })?;
    let bytes = serde_json::to_vec_pretty(catalog).map_err(|error| {
        ExtensionError::internal(format!(
            "Could not serialize extension catalog cache: {error}"
        ))
    })?;
    let destination = extensions_dir.join(CATALOG_CACHE_FILE);
    let temp_path = extensions_dir.join(format!(".catalog-cache-{}.tmp", Uuid::new_v4()));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp_path, &destination)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(ExtensionError::internal(format!(
            "Could not write extension catalog cache {}: {error}",
            destination.display()
        )));
    }
    Ok(())
}
