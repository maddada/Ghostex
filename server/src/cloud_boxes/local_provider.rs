//! The `local` provider at work: Docker commands from `ghostex_gx_cloud::local`, run on this
//! computer. Every function blocks; call them from `spawn_blocking`.

use std::path::{Path, PathBuf};

use ghostex_gx_cloud::local;
use ghostex_gx_cloud::metadata::BoxMetadata;
use ghostex_gx_protocol::cloud_box::{
    CloudBoxConnectInfo, BOX_GHOSTEX_BIN, CLOUD_BOX_TRANSPORT_DIRECT,
};
use serde_json::Value;

use super::docker::{run_docker, BOOT_TIMEOUT, CREATE_TIMEOUT, QUICK_TIMEOUT};

/// Docker's server architecture, or why Docker can't be used.
pub(super) fn docker_arch(home: &Path) -> Result<String, String> {
    let output = run_docker(home, &local::server_arch_args(), None, QUICK_TIMEOUT)?;
    if !output.success {
        let message = output.failure_message();
        return Err(
            if message.contains("Cannot connect") || message.contains("failed to connect") {
                "Docker isn't running. Start Docker Desktop, OrbStack or colima.".to_string()
            } else {
                message
            },
        );
    }
    Ok(output.stdout.trim().to_string())
}

/// The Linux gxserver package folder for `arch` (`arm64`, `x64`): the
/// `GHOSTEX_CLOUD_BOXES_LOCAL_PACKAGE` folder, else the one the app bundle stages beside its own
/// gxserver (`Web/gxserver-linux-<arch>`), the same package remote computers get.
pub(super) fn package_dir(arch: &str) -> Result<PathBuf, String> {
    let has_gxserver = |dir: &Path| dir.join("bin").join("gxserver").is_file();
    if let Some(dir) = std::env::var_os(local::PACKAGE_DIR_ENV).map(PathBuf::from) {
        return if has_gxserver(&dir) {
            Ok(dir)
        } else {
            Err(format!("{} has no bin/gxserver.", dir.display()))
        };
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    // <Web>/gxserver/bin/gxserver → <Web>/gxserver-linux-<arch>
    let candidate = exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|web| web.join(format!("gxserver-linux-{arch}")));
    match candidate {
        Some(dir) if has_gxserver(&dir) => Ok(dir),
        _ => Err(format!(
            "This Ghostex has no Linux gxserver package for {arch}. Set {} to a package folder (build/remote-gxserver-linux/{arch}/package).",
            local::PACKAGE_DIR_ENV
        )),
    }
}

pub(super) fn list_containers(home: &Path) -> Result<Vec<local::LocalContainer>, String> {
    let ids: Vec<String> = run_docker(home, &local::list_ids_args(), None, QUICK_TIMEOUT)?
        .into_stdout()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let json = run_docker(home, &local::inspect_args(&ids), None, QUICK_TIMEOUT)?.into_stdout()?;
    local::parse_inspect(&json)
}

pub(super) fn find_container(
    home: &Path,
    box_id: &str,
) -> Result<Option<local::LocalContainer>, String> {
    Ok(list_containers(home)?
        .into_iter()
        .find(|container| container.metadata.box_id == box_id))
}

/// `docker run`: the container, idle until boot. Returns the container id.
pub(super) fn create_container(
    home: &Path,
    meta: &BoxMetadata,
    image: &str,
    vcpus: u64,
    mem_mib: u64,
) -> Result<String, String> {
    let output = run_docker(
        home,
        &local::create_args(meta, image, vcpus, mem_mib),
        None,
        CREATE_TIMEOUT,
    )?;
    Ok(output.into_stdout()?.trim().to_string())
}

pub(super) fn install_package(home: &Path, box_id: &str, package: &Path) -> Result<(), String> {
    run_docker(
        home,
        &local::install_package_args(box_id, &package.to_string_lossy()),
        None,
        CREATE_TIMEOUT,
    )?
    .into_stdout()
    .map(|_| ())
}

pub(super) fn start(home: &Path, box_id: &str) -> Result<(), String> {
    run_docker(home, &local::start_args(box_id), None, QUICK_TIMEOUT)?
        .into_stdout()
        .map(|_| ())
}

pub(super) fn stop(home: &Path, box_id: &str) -> Result<(), String> {
    run_docker(home, &local::stop_args(box_id), None, QUICK_TIMEOUT)?
        .into_stdout()
        .map(|_| ())
}

pub(super) fn delete(home: &Path, box_id: &str) -> Result<(), String> {
    run_docker(home, &local::delete_args(box_id), None, QUICK_TIMEOUT)?
        .into_stdout()
        .map(|_| ())
}

/// The box gxserver's port and token (`ghostex server endpoint` on the box) and the loopback port
/// Docker published for the relay.
pub(super) fn connect_info(
    home: &Path,
    container: &local::LocalContainer,
) -> Result<CloudBoxConnectInfo, String> {
    let box_id = container.metadata.box_id.as_str();
    let output = run_docker(
        home,
        &local::exec_args(box_id, &[BOX_GHOSTEX_BIN, "server", "endpoint"]),
        None,
        QUICK_TIMEOUT,
    )?
    .into_stdout()?;
    let endpoint: Value = output
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line.trim()).ok())
        .ok_or_else(|| "The box's gxserver did not report its endpoint.".to_string())?;
    let host_port = container
        .relay_host_port
        .ok_or_else(|| "Docker published no port for the box.".to_string())?;
    Ok(CloudBoxConnectInfo {
        box_id: box_id.to_string(),
        transport: CLOUD_BOX_TRANSPORT_DIRECT.to_string(),
        tailcat_address: None,
        ssh_port: None,
        ssh_user: None,
        gxserver_port: endpoint
            .get("port")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        base_url: Some(format!("http://127.0.0.1:{host_port}")),
        gxserver_token: endpoint
            .get("authToken")
            .and_then(Value::as_str)
            .ok_or_else(|| "The box's gxserver has no token yet.".to_string())?
            .to_string(),
        gxserver_version: None,
    })
}

/// `docker exec [-i] <box> argv…` with stdin piped in when given.
pub(super) fn exec(
    home: &Path,
    box_id: &str,
    argv: &[&str],
    stdin: Option<&str>,
) -> Result<ghostex_gx_cloud::provider::ExecOutput, String> {
    let mut args = vec!["exec".to_string()];
    if stdin.is_some() {
        args.push("-i".to_string());
    }
    args.push(local::container_name(box_id));
    args.extend(argv.iter().map(|arg| arg.to_string()));
    let output = run_docker(home, &args, stdin.map(str::as_bytes), BOOT_TIMEOUT)?;
    Ok(ghostex_gx_cloud::provider::ExecOutput {
        exit_code: if output.success { 0 } else { 1 },
        stdout: output.stdout,
        stderr: output.stderr,
        timed_out: false,
    })
}

/// The `local` file API: writes `bytes` to `path` (mode 0600, parents made).
pub(super) fn write_file(
    home: &Path,
    box_id: &str,
    path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let args = local::write_file_args(box_id, path)
        .ok_or_else(|| format!("\"{path}\" is not a path a box file can be written to."))?;
    run_docker(home, &args, Some(bytes), QUICK_TIMEOUT)?
        .into_stdout()
        .map(|_| ())
}

/// `docker commit` of the box into its snapshot image.
pub(super) fn commit(
    home: &Path,
    box_id: &str,
    snapshot_id: &str,
    name: &str,
    created_at: &str,
) -> Result<(), String> {
    run_docker(
        home,
        &local::commit_args(box_id, snapshot_id, name, created_at),
        None,
        CREATE_TIMEOUT,
    )?
    .into_stdout()
    .map(|_| ())
}

/// The snapshot image, when Docker still has it.
pub(super) fn snapshot_exists(
    home: &Path,
    image: &str,
) -> Result<Option<ghostex_gx_cloud::provider::ProviderSnapshot>, String> {
    let output = run_docker(
        home,
        &[
            "image".to_string(),
            "inspect".to_string(),
            image.to_string(),
        ],
        None,
        QUICK_TIMEOUT,
    )?;
    if !output.success {
        return Ok(None);
    }
    Ok(local::parse_snapshot_images(&output.stdout)?
        .into_iter()
        .next()
        .map(|found| ghostex_gx_cloud::provider::ProviderSnapshot {
            provider_snapshot_id: image.to_string(),
            name: found.name,
            status: "ready".to_string(),
            source_box_id: found.source_box_id,
            size_bytes: found.size_bytes,
            created_at: found.created_at,
            error: None,
        }))
}

pub(super) fn delete_snapshot_image(home: &Path, image: &str) -> Result<(), String> {
    let output = run_docker(
        home,
        &["rmi".to_string(), image.to_string()],
        None,
        QUICK_TIMEOUT,
    )?;
    if output.success || output.failure_message().contains("No such image") {
        Ok(())
    } else {
        Err(output.failure_message())
    }
}
