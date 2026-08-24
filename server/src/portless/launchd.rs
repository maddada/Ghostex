use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::paths::GxserverPaths;
use crate::platform::resources;

use super::sync::*;
use super::types::*;

pub(crate) const PORTLESS_SERVICE_LABEL: &str = "sh.portless.proxy";
const PORTLESS_SERVICE_PLIST_PATH: &str = "/Library/LaunchDaemons/sh.portless.proxy.plist";
const PORTLESS_SERVICE_TLD: &str = "localhost";

pub(crate) fn inspect_installed_portless_service(
    expectation: &PortlessServiceExpectation,
) -> Result<PortlessServiceInspection> {
    let plist = read_installed_portless_service_plist()?;
    let reachability = plist.as_ref().map(|_| PortlessServiceReachability {
        manager_running: None,
        proxy_reachable: Some(probe_portless_proxy_reachable(expectation.proxy_port)),
    });
    inspect_portless_service_from_plist_text(
        plist.as_deref(),
        expectation,
        reachability.unwrap_or_default(),
    )
}

pub(crate) fn inspect_portless_service_from_plist_text(
    plist_text: Option<&str>,
    expectation: &PortlessServiceExpectation,
    reachability: PortlessServiceReachability,
) -> Result<PortlessServiceInspection> {
    let Some(plist_text) = plist_text else {
        return Ok(PortlessServiceInspection {
            classification: PortlessServiceClassification::Missing,
            mismatch_count: 0,
        });
    };
    let plist = parse_portless_launchd_plist(plist_text)?;
    Ok(classify_portless_launchd_service(
        &plist,
        expectation,
        reachability,
    ))
}

#[cfg(target_os = "macos")]
fn read_installed_portless_service_plist() -> Result<Option<String>> {
    match fs::read_to_string(PORTLESS_SERVICE_PLIST_PATH) {
        Ok(plist) => Ok(Some(plist)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| "read installed Portless launchd plist"),
    }
}

#[cfg(not(target_os = "macos"))]
fn read_installed_portless_service_plist() -> Result<Option<String>> {
    Ok(None)
}

fn classify_portless_launchd_service(
    plist: &PortlessLaunchdPlist,
    expectation: &PortlessServiceExpectation,
    reachability: PortlessServiceReachability,
) -> PortlessServiceInspection {
    let node_matches = plist
        .program_arguments
        .first()
        .map(|arg| {
            path_value_matches_any(arg, &expectation.expected_node_paths, &expectation.home_dir)
        })
        .unwrap_or(false);
    let cli_matches = plist
        .program_arguments
        .get(1)
        .map(|arg| {
            path_value_matches_any(arg, &expectation.expected_cli_paths, &expectation.home_dir)
        })
        .unwrap_or(false);
    let state_dir_matches = plist
        .environment
        .get("PORTLESS_STATE_DIR")
        .map(|value| {
            normalize_path_value_for_comparison(value, &expectation.home_dir)
                == expectation.expected_state_dir
        })
        .unwrap_or(false);
    let ghostex_marked = node_matches || cli_matches || state_dir_matches;
    if !ghostex_marked {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::Standalone,
            mismatch_count: 0,
        };
    }

    let mut mismatch_count = 0_usize;
    mismatch_count += (plist.label.as_deref() != Some(PORTLESS_SERVICE_LABEL)) as usize;
    mismatch_count += (!portless_program_has_proxy_start(&plist.program_arguments)) as usize;
    mismatch_count += (!node_matches) as usize;
    mismatch_count += (!cli_matches) as usize;
    mismatch_count += (!state_dir_matches) as usize;
    mismatch_count +=
        (!portless_env_port_matches(&plist.environment, expectation.proxy_port)) as usize;
    mismatch_count +=
        (!portless_env_protocol_matches(&plist.environment, expectation.protocol)) as usize;
    mismatch_count +=
        (!portless_env_tld_matches(&plist.environment, PORTLESS_SERVICE_TLD)) as usize;
    mismatch_count += (!portless_env_lan_matches(&plist.environment, false)) as usize;
    mismatch_count += (!portless_env_wildcard_matches(&plist.environment, false)) as usize;
    mismatch_count += (!portless_env_sync_hosts_matches(&plist.environment, false)) as usize;
    mismatch_count +=
        (!portless_launchd_output_path_matches(plist.standard_out_path.as_deref())) as usize;
    mismatch_count +=
        (!portless_launchd_output_path_matches(plist.standard_error_path.as_deref())) as usize;
    mismatch_count +=
        (!portless_args_port_matches(&plist.program_arguments, expectation.proxy_port)) as usize;
    mismatch_count +=
        (!portless_args_protocol_matches(&plist.program_arguments, expectation.protocol)) as usize;
    mismatch_count +=
        (!portless_args_tld_matches(&plist.program_arguments, PORTLESS_SERVICE_TLD)) as usize;
    mismatch_count += (!portless_args_lan_matches(&plist.program_arguments, false)) as usize;
    mismatch_count += (!portless_args_wildcard_matches(&plist.program_arguments, false)) as usize;

    if mismatch_count > 0 {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexConfigMismatch,
            mismatch_count,
        };
    }

    if reachability.manager_running == Some(false) || reachability.proxy_reachable == Some(false) {
        return PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexFailed,
            mismatch_count,
        };
    }

    PortlessServiceInspection {
        classification: PortlessServiceClassification::GhostexActive,
        mismatch_count,
    }
}

pub(crate) fn portless_state_for_service_inspection(
    existing: Option<&PortlessState>,
    protocol: PortlessProtocol,
    inspection: &PortlessServiceInspection,
) -> PortlessState {
    let enabled = existing.map(|state| state.enabled).unwrap_or(true);
    let disabled = is_portless_disabled_state(existing);
    let (setup_ownership, mut setup_status, runtime_status) = match inspection.classification {
        PortlessServiceClassification::Missing => (
            PortlessSetupOwnership::Missing,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::Standalone => (
            PortlessSetupOwnership::Standalone,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::GhostexConfigMismatch => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ),
        PortlessServiceClassification::GhostexFailed => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Failed,
            PortlessRuntimeStatus::Failed,
        ),
        PortlessServiceClassification::GhostexActive => (
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        ),
    };
    if disabled {
        setup_status = PortlessSetupStatus::Disabled;
    }
    PortlessState {
        enabled,
        protocol,
        setup_ownership,
        setup_status,
        runtime_status,
    }
}

pub(crate) fn expected_portless_service_config(
    paths: &GxserverPaths,
    protocol: PortlessProtocol,
) -> PortlessServiceExpectation {
    let home_dir = paths.home_dir.clone();
    let expected_node_paths =
        normalize_and_dedupe_paths(expected_portless_node_candidates(), &home_dir);
    let expected_cli_paths =
        normalize_and_dedupe_paths(expected_portless_cli_candidates(), &home_dir);
    PortlessServiceExpectation {
        home_dir,
        expected_node_paths,
        expected_cli_paths,
        expected_state_dir: normalize_path_for_comparison(&paths.portless_state_dir),
        protocol,
        proxy_port: portless_service_port_for_protocol(protocol),
    }
}

fn expected_portless_node_candidates() -> Vec<PathBuf> {
    resources::code_server_node_candidates()
}

fn expected_portless_cli_candidates() -> Vec<PathBuf> {
    resources::portless_cli_candidates()
}

fn normalize_and_dedupe_paths(paths: Vec<PathBuf>, home_dir: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for path in paths {
        let normalized = normalize_path_value_for_comparison(&path.to_string_lossy(), home_dir);
        if seen.insert(normalized.clone()) {
            output.push(normalized);
        }
    }
    output
}

pub(crate) fn portless_service_port_for_protocol(protocol: PortlessProtocol) -> u16 {
    match protocol {
        PortlessProtocol::Https => 443,
        PortlessProtocol::Http => 80,
    }
}

fn parse_portless_launchd_plist(plist_text: &str) -> Result<PortlessLaunchdPlist> {
    let label = parse_plist_string_for_key(plist_text, "Label")?;
    let program_arguments =
        parse_plist_string_array_for_key(plist_text, "ProgramArguments")?.unwrap_or_default();
    let environment =
        parse_plist_string_dict_for_key(plist_text, "EnvironmentVariables")?.unwrap_or_default();
    let standard_out_path = parse_plist_string_for_key(plist_text, "StandardOutPath")?;
    let standard_error_path = parse_plist_string_for_key(plist_text, "StandardErrorPath")?;
    Ok(PortlessLaunchdPlist {
        label,
        program_arguments,
        environment,
        standard_out_path,
        standard_error_path,
    })
}

fn parse_plist_string_for_key(plist_text: &str, key: &str) -> Result<Option<String>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "string") else {
        return Ok(None);
    };
    xml_unescape(block).map(Some)
}

fn parse_plist_string_array_for_key(plist_text: &str, key: &str) -> Result<Option<Vec<String>>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "array") else {
        return Ok(None);
    };
    parse_xml_string_elements(block).map(Some)
}

fn parse_plist_string_dict_for_key(
    plist_text: &str,
    key: &str,
) -> Result<Option<BTreeMap<String, String>>> {
    let Some(after_key) = find_plist_key_end(plist_text, key)? else {
        return Ok(None);
    };
    let Some(block) = xml_element_block(&plist_text[after_key..], "dict") else {
        return Ok(None);
    };
    parse_xml_key_string_dict(block).map(Some)
}

fn find_plist_key_end(plist_text: &str, wanted_key: &str) -> Result<Option<usize>> {
    let mut offset = 0_usize;
    while let Some(start) = plist_text[offset..].find("<key>") {
        let key_start = offset + start + "<key>".len();
        let Some(end) = plist_text[key_start..].find("</key>") else {
            return Ok(None);
        };
        let key_end = key_start + end;
        let key = xml_unescape(&plist_text[key_start..key_end])?;
        let after_key = key_end + "</key>".len();
        if key == wanted_key {
            return Ok(Some(after_key));
        }
        offset = after_key;
    }
    Ok(None)
}

fn xml_element_block<'a>(input: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = input.find(&open)? + open.len();
    let end = input[start..].find(&close)?;
    Some(&input[start..start + end])
}

fn parse_xml_string_elements(block: &str) -> Result<Vec<String>> {
    let mut values = Vec::new();
    let mut offset = 0_usize;
    while let Some(start) = block[offset..].find("<string>") {
        let value_start = offset + start + "<string>".len();
        let Some(end) = block[value_start..].find("</string>") else {
            break;
        };
        let value_end = value_start + end;
        values.push(xml_unescape(&block[value_start..value_end])?);
        offset = value_end + "</string>".len();
    }
    Ok(values)
}

fn parse_xml_key_string_dict(block: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let mut offset = 0_usize;
    while let Some(start) = block[offset..].find("<key>") {
        let key_start = offset + start + "<key>".len();
        let Some(key_end_offset) = block[key_start..].find("</key>") else {
            break;
        };
        let key_end = key_start + key_end_offset;
        let key = xml_unescape(&block[key_start..key_end])?;
        let after_key = key_end + "</key>".len();
        let Some(value_block) = xml_element_block(&block[after_key..], "string") else {
            offset = after_key;
            continue;
        };
        values.insert(key, xml_unescape(value_block)?);
        offset = after_key
            + block[after_key..]
                .find("</string>")
                .map(|end| end + "</string>".len())
                .unwrap_or(0);
    }
    Ok(values)
}

fn xml_unescape(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some(entity_start) = remaining.find('&') {
        output.push_str(&remaining[..entity_start]);
        let entity_tail = &remaining[entity_start + 1..];
        let Some(entity_end) = entity_tail.find(';') else {
            bail!("Invalid XML entity in Portless launchd plist.");
        };
        let entity = &entity_tail[..entity_end];
        let replacement = match entity {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => bail!("Unsupported XML entity in Portless launchd plist."),
        };
        output.push_str(replacement);
        remaining = &entity_tail[entity_end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn portless_program_has_proxy_start(program_arguments: &[String]) -> bool {
    portless_proxy_start_args(program_arguments).is_some()
}

fn portless_proxy_start_args(program_arguments: &[String]) -> Option<&[String]> {
    program_arguments
        .windows(2)
        .position(|window| window[0] == "proxy" && window[1] == "start")
        .map(|index| &program_arguments[index + 2..])
}

fn portless_env_port_matches(environment: &BTreeMap<String, String>, expected_port: u16) -> bool {
    environment
        .get("PORTLESS_PORT")
        .and_then(|value| parse_portless_port_value(value))
        == Some(expected_port)
}

fn portless_env_protocol_matches(
    environment: &BTreeMap<String, String>,
    expected_protocol: PortlessProtocol,
) -> bool {
    environment
        .get("PORTLESS_HTTPS")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_protocol == PortlessProtocol::Https)
}

fn portless_env_tld_matches(environment: &BTreeMap<String, String>, expected_tld: &str) -> bool {
    environment
        .get("PORTLESS_TLD")
        .map(|value| value.trim().eq_ignore_ascii_case(expected_tld))
        .unwrap_or(expected_tld == PORTLESS_SERVICE_TLD)
}

fn portless_env_lan_matches(environment: &BTreeMap<String, String>, expected_lan: bool) -> bool {
    let lan_matches = environment
        .get("PORTLESS_LAN")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_lan);
    let lan_ip_absent = environment
        .get("PORTLESS_LAN_IP")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    lan_matches && (expected_lan || lan_ip_absent)
}

fn portless_env_wildcard_matches(
    environment: &BTreeMap<String, String>,
    expected_wildcard: bool,
) -> bool {
    environment
        .get("PORTLESS_WILDCARD")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_wildcard)
}

fn portless_env_sync_hosts_matches(
    environment: &BTreeMap<String, String>,
    expected_sync_hosts: bool,
) -> bool {
    environment
        .get("PORTLESS_SYNC_HOSTS")
        .and_then(|value| parse_portless_bool_value(value))
        == Some(expected_sync_hosts)
}

fn portless_launchd_output_path_matches(path: Option<&str>) -> bool {
    path.map(str::trim) == Some("/dev/null")
}

fn portless_args_port_matches(program_arguments: &[String], expected_port: u16) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    portless_arg_value(args, "--port", Some("-p")).and_then(parse_portless_port_value)
        == Some(expected_port)
}

fn portless_args_protocol_matches(
    program_arguments: &[String],
    expected_protocol: PortlessProtocol,
) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    if portless_args_contain(args, "--cert")
        || portless_args_contain(args, "--key")
        || portless_args_contain(args, "--no-tls") && portless_args_contain(args, "--https")
    {
        return false;
    }
    match expected_protocol {
        PortlessProtocol::Https => portless_args_contain(args, "--https"),
        PortlessProtocol::Http => portless_args_contain(args, "--no-tls"),
    }
}

fn portless_args_tld_matches(program_arguments: &[String], expected_tld: &str) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    if portless_args_contain(args, "--lan") || portless_args_contain(args, "--ip") {
        return false;
    }
    portless_arg_value(args, "--tld", None)
        .map(|value| value.trim().eq_ignore_ascii_case(expected_tld))
        .unwrap_or(expected_tld == PORTLESS_SERVICE_TLD)
}

fn portless_args_lan_matches(program_arguments: &[String], expected_lan: bool) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    let lan_enabled = portless_args_contain(args, "--lan") || portless_args_contain(args, "--ip");
    lan_enabled == expected_lan
}

fn portless_args_wildcard_matches(program_arguments: &[String], expected_wildcard: bool) -> bool {
    let Some(args) = portless_proxy_start_args(program_arguments) else {
        return false;
    };
    portless_args_contain(args, "--wildcard") == expected_wildcard
}

fn portless_args_contain(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| {
        arg == flag
            || arg
                .strip_prefix(flag)
                .is_some_and(|tail| tail.starts_with('='))
    })
}

fn portless_arg_value<'a>(
    args: &'a [String],
    long_flag: &str,
    short_flag: Option<&str>,
) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        if arg == long_flag || short_flag.is_some_and(|flag| arg == flag) {
            return args.get(index + 1).map(String::as_str);
        }
        if let Some(value) = arg.strip_prefix(&format!("{long_flag}=")) {
            return Some(value);
        }
    }
    None
}

fn parse_portless_bool_value(value: &str) -> Option<bool> {
    match value.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn parse_portless_port_value(value: &str) -> Option<u16> {
    let port = value.trim().parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

fn path_value_matches_any(value: &str, expected_paths: &[String], home_dir: &Path) -> bool {
    let normalized = normalize_path_value_for_comparison(value, home_dir);
    expected_paths
        .iter()
        .any(|expected| expected.as_str() == normalized)
}

fn normalize_path_value_for_comparison(value: &str, home_dir: &Path) -> String {
    let trimmed = value.trim();
    let path = if trimmed == "~" {
        home_dir.to_path_buf()
    } else if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        home_dir.join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    normalize_path_for_comparison(&path)
}

pub(crate) fn normalize_path_for_comparison(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized.to_string_lossy().to_string()
}

#[derive(Clone, Debug)]
pub(crate) struct PortlessServiceExpectation {
    pub(crate) home_dir: PathBuf,
    pub(crate) expected_node_paths: Vec<String>,
    pub(crate) expected_cli_paths: Vec<String>,
    pub(crate) expected_state_dir: String,
    pub(crate) protocol: PortlessProtocol,
    pub(crate) proxy_port: u16,
}

#[derive(Clone, Debug)]
struct PortlessLaunchdPlist {
    label: Option<String>,
    program_arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    standard_out_path: Option<String>,
    standard_error_path: Option<String>,
}
