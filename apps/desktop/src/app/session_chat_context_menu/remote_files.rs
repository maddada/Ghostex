use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

impl GhostexGpuiApp {
    pub(crate) fn open_remote_session_chat_file(
        &mut self,
        session_id: TerminalSessionId,
        path: &str,
        line: Option<u32>,
        column: Option<u32>,
        requested_view: Option<shared_settings::SharedChatFileOpenView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
            return;
        };
        self.open_remote_session_chat_file_for_key(
            key,
            path,
            line,
            column,
            requested_view,
            window,
            cx,
        );
    }

    /// CDXC:SessionChat 2026-09-23 WHY:
    /// Remote links belong to the session computer: client filesystem checks reject valid remote paths, and client home/drive expansion can point at an unrelated local file.
    pub(crate) fn open_remote_session_chat_file_for_key(
        &mut self,
        key: GpuiRemoteAttachSessionKey,
        path: &str,
        line: Option<u32>,
        column: Option<u32>,
        requested_view: Option<shared_settings::SharedChatFileOpenView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if path.is_empty()
            || path.contains('\0')
            || path.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        {
            return;
        }
        let scoped_project_id =
            gpui_remote_scoped_project_id(&key.remote_machine_id, &key.project_id);
        let Some(snapshot) = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot
                    .active_project_id
                    .as_ref()
                    .is_some_and(|id| id.0 == scoped_project_id)
            })
        else {
            self.report_session_chat_file_open_failure(
                "That remote session's project is no longer active.",
                cx,
            );
            return;
        };
        // CDXC:Navigation 2026-10-17 WHY: a remote file follows Settings > File opening too, but an editor app or the system app on this computer cannot reach a file on the session computer: an editor choice opens it in Files, and video, audio and what only the system app could show (HEIC, JPEG XL) keep opening in Code as they did before.
        let video_or_audio = matches!(
            crate::app::native_docs::state::DocsMediaKind::for_path(path),
            Some(
                crate::app::native_docs::state::DocsMediaKind::Video
                    | crate::app::native_docs::state::DocsMediaKind::Audio
            )
        );
        let preferred = requested_view.or_else(|| {
            if video_or_audio {
                return None;
            }
            match crate::app::native_docs::entry::file_open_target_for_path(Path::new(path)) {
                shared_settings::SharedFileOpenTarget::Code
                | shared_settings::SharedFileOpenTarget::SystemApp => None,
                shared_settings::SharedFileOpenTarget::Files
                | shared_settings::SharedFileOpenTarget::Editor(_) => {
                    Some(shared_settings::SharedChatFileOpenView::Docs)
                }
            }
        });
        if preferred == Some(shared_settings::SharedChatFileOpenView::Docs)
            && !gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
            && self.titlebar_mode_available(TitlebarMode::Manage)
        {
            let project_path = snapshot
                .in_memory_project_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned());
            self.open_remote_chat_docs_file(&key, path, project_path.as_deref(), window, cx);
            return;
        }
        if requested_view == Some(shared_settings::SharedChatFileOpenView::Docs) {
            self.report_session_chat_file_open_failure(
                "Files view is not available for this project.",
                cx,
            );
            return;
        }
        let Some(target) = self.source_code_server_runtime_target(snapshot).filter(|target| {
            matches!(&target.endpoint, SourceCodeServerRuntimeEndpoint::Remote { remote_machine_id, .. } if remote_machine_id == &key.remote_machine_id)
        }) else {
            self.report_session_chat_file_open_failure("VS Code view is not available for this remote project.", cx);
            return;
        };
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            self.copy_path_for_disabled_project_workarea(path, "VS Code", cx);
            return;
        }
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            column,
            file_path: PathBuf::from(path),
            line,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path: target.project_path.clone(),
            remote_target: Some(target),
            remote_working_directory: self
                .remote_attach_sessions
                .get(&key)
                .and_then(|session_id| {
                    self.agents_terminal_runtime_sessions
                        .runtime_session_id_for_shell_session(*session_id)
                })
                .and_then(|id| self.agents_terminal_runtime_osc_states.get(&id))
                .and_then(|state| state.pwd.clone()),
        });
        self.report_session_chat_file_opening("VS Code view", Path::new(path), cx);
        self.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Source, cx);
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
        let pending = self.pending_source_file_open.as_ref();
        let runtime_target = self.source_code_server_runtime.target.as_ref();
        let requested_target = pending.and_then(|pending| pending.remote_target.as_ref());
        support_logs::append_for_scenario(
            support_logs::GpuiSupportLog::SidebarRefresh,
            "native.sidebar.refresh",
            "gpui.sourceFileOpen.pending",
            serde_json::json!({
                "pending": pending.is_some(),
                "runtimeState": format!("{:?}", self.source_code_server_runtime.state),
                "hasRuntimeTarget": runtime_target.is_some(),
                "projectPathMatches": pending.zip(runtime_target).is_some_and(|(pending, runtime)| pending.project_path == runtime.project_path),
                "projectIdentityMatches": requested_target.zip(runtime_target).is_some_and(|(requested, runtime)| requested.active_project_id == runtime.active_project_id),
                "workareaIdentityMatches": requested_target.zip(runtime_target).is_some_and(|(requested, runtime)| requested.source_workarea_id == runtime.source_workarea_id),
                "endpointMatches": requested_target.zip(runtime_target).is_some_and(|(requested, runtime)| requested.endpoint == runtime.endpoint),
                "surfaceCurrent": self.project_workarea_runtime_cef_surface_is_current(ProjectWorkareaCefSurfaceSlotKey::Source),
                "activeMode": format!("{:?}", self.active_mode),
            }),
        );
    }

    fn open_remote_chat_docs_file(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        path: &str,
        project_path: Option<&str>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.gpui_remote_gxserver_request_target(&key.remote_machine_id) else {
            let computer = gpui_remote_machine_name_from_settings(&key.remote_machine_id)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "The session's computer".to_string());
            self.report_session_chat_file_open_failure(
                &format!("{computer} is offline or not connected. Reconnect it, then try again."),
                cx,
            );
            return;
        };
        let path = if path.to_ascii_lowercase().starts_with("file://") {
            let url_path = &path[7..];
            let url_path = if url_path.starts_with('/') {
                Some(url_path)
            } else {
                url_path
                    .find('/')
                    .filter(|index| url_path[..*index].eq_ignore_ascii_case("localhost"))
                    .map(|index| &url_path[index..])
            };
            let Some(url_path) = url_path else {
                self.report_session_chat_file_open_failure(
                    "That file URL is not a path on the session computer.",
                    cx,
                );
                return;
            };
            let Some(decoded) =
                browser_favicon_percent_decode(url_path, GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            else {
                return;
            };
            decoded
        } else {
            path.to_string()
        };
        let (relative, outside) =
            gpui_remote_chat_file_place(&path, project_path.unwrap_or_default());
        // CDXC:Docs 2026-11-11 WHY: asking a Windows computer about `\\host\share\…` signs it in
        // to that host, so gxserver refuses network shares and device paths on their text
        // (server/src/project_docs/chat_file.rs) and the link says so here without asking.
        if relative.starts_with("//") {
            self.report_session_chat_file_open_failure(
                "Files can't open a file on a network share. Open it on the computer that has it.",
                cx,
            );
            return;
        }
        /*
        CDXC:Docs 2026-11-07 WHY: issue #462. The check asked about the Docs folders alone, while
        the Files view it opens covers the whole project, so any project file outside them failed
        too. It now asks with the Files view's project scope, and a path outside the project (an
        agent's log in `/private/tmp/…`) goes with this chat's session: gxserver opens it, read-only,
        only when this chat links to it, and only as plain text or a picture (server/src/project_docs/chat_file.rs).
        */
        let mut params = serde_json::json!({
            "projectId": key.project_id,
            "action": "stat",
            "path": relative,
            "scope": "project",
            "additionalDocsFolders": gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot),
        });
        if outside {
            params["sessionId"] = serde_json::json!(key.session_id);
        }
        let computer = gpui_remote_machine_name_from_settings(&key.remote_machine_id)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "The session's computer".to_string());
        let picture = matches!(
            crate::app::native_docs::state::DocsMediaKind::for_path(&relative),
            Some(crate::app::native_docs::state::DocsMediaKind::Image)
        );
        // CDXC:Docs 2026-11-11 WHY: the remote check reads the chat's whole Files and links list,
        // a few seconds on a long chat, and nothing showed meanwhile, so a click looked like it did
        // nothing. The opening toast shows at once, and the failure replaces it with the reason.
        self.report_session_chat_file_opening("Files view", Path::new(&path), cx);
        let key = key.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/runProjectDocsAction",
                        &params,
                        Duration::from_secs(30),
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let active = gpui_remote_scoped_project_id(&key.remote_machine_id, &key.project_id);
                if this.active_sidebar_project_id().as_deref() != Some(active.as_str()) {
                    this.remove_gpui_app_toast(GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID, cx);
                    return;
                }
                match result {
                    Ok(value) if value.get("error").is_none() && value.get("file").is_some() => {
                        if outside {
                            this.native_docs
                                .remote_chat_files
                                .insert(relative.clone(), (active.clone(), key.session_id.clone()));
                        }
                        this.pending_docs_file_open = Some(relative);
                        this.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
                        this.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
                        this.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
                        if !this.deliver_pending_docs_file_open(cx) {
                            this.schedule_pending_docs_file_open_delivery(cx);
                        }
                    }
                    outcome => {
                        this.remove_gpui_app_toast(GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID, cx);
                        let reason =
                            gpui_remote_chat_file_failure(outcome, outside, picture, &computer);
                        this.report_session_chat_file_open_failure(&reason, cx);
                    }
                }
            });
        })
        .detach();
    }
}

/// Where a chat link's path sits on a remote computer: the path the Files request carries
/// (project-relative inside the project, absolute with forward slashes outside it) and whether it
/// is outside. A Windows computer is known by its project folder or by the link itself (a drive or
/// UNC path), since this computer's own path rules read `C:\…` as a relative name.
pub(crate) fn gpui_remote_chat_file_place(path: &str, project_path: &str) -> (String, bool) {
    let mut path = path.to_string();
    let mut root = project_path.to_string();
    let link_is_windows = {
        let bare = path.strip_prefix('/').unwrap_or(&path);
        gpui_terminal_link_is_windows_drive_path(bare) || path.starts_with("\\\\")
    };
    let windows = gpui_is_windows_remote_path(&root) || link_is_windows;
    if windows {
        path = path.replace('\\', "/");
        root = root.replace('\\', "/");
        if path.starts_with('/') && gpui_terminal_link_is_windows_drive_path(&path[1..]) {
            path.remove(0);
        }
    }
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let relative = if !root.is_empty()
        && (path.starts_with(&prefix)
            || (windows
                && path
                    .get(..prefix.len())
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&prefix))))
    {
        path[prefix.len()..].to_string()
    } else {
        path
    };
    let outside = gpui_remote_chat_file_is_outside(&relative, windows);
    (relative, outside)
}

/// The plain reason a remote chat file did not open: the computer could not be reached, it runs a
/// Ghostex too old for the file, or gxserver's own reason (gone, not linked by this chat, too large).
fn gpui_remote_chat_file_failure(
    outcome: Result<serde_json::Value, String>,
    outside: bool,
    picture: bool,
    computer: &str,
) -> String {
    let older = format!(
        "{computer} runs an older Ghostex that can't show this file in Files. Update Ghostex on {computer}, then try again."
    );
    match outcome {
        Ok(value) => match value.get("error").and_then(serde_json::Value::as_str) {
            // Before chat-linked files (11.3.0), an absolute path was refused as any Files path.
            Some("Paths must be project-relative.") if outside => older,
            // 11.3.0 opened chat-linked text only and called a picture "not plain text".
            Some(error)
                if picture && outside && error.starts_with("This file is not plain text") =>
            {
                older
            }
            Some(error) if !error.trim().is_empty() => error.to_string(),
            _ => "That file is not on the session computer any more.".to_string(),
        },
        // gxserver answered, but refused the request as a whole.
        Err(error)
            if error.starts_with("Remote gxserver request failed")
                || error.starts_with("gxserver returned") =>
        {
            format!("{computer} couldn't open this file in Files.")
        }
        Err(_) => {
            format!("Couldn't reach {computer}. Check that it is on and connected, then try again.")
        }
    }
}

/// A chat link's path that names a file outside the remote project: absolute (a drive path on a
/// Windows remote) or from the home folder, after the project's own prefix was taken off.
fn gpui_remote_chat_file_is_outside(path: &str, windows: bool) -> bool {
    path.starts_with('/')
        || path == "~"
        || path.starts_with("~/")
        || (windows && (gpui_terminal_link_is_windows_drive_path(path) || path.starts_with("//")))
}

/// CDXC:CodeEditor 2026-09-23 WHY:
/// The remote editor's Node runtime interprets its own path syntax and IPC endpoint. Running the local Code CLI would target the client's editor even when both computers use the same project path.
pub(crate) fn open_remote_source_file(
    target: &SourceCodeServerRuntimeTarget,
    file_path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    working_directory: Option<&str>,
) -> Result<(), String> {
    let SourceCodeServerRuntimeEndpoint::Remote {
        machine_config,
        execution_target,
        ..
    } = &target.endpoint
    else {
        return Err("The remote editor target is unavailable.".into());
    };
    let request = STANDARD.encode(serde_json::to_vec(&serde_json::json!({
        "path": file_path.to_string_lossy(), "project": target.project_path.to_string_lossy(), "line": line, "column": column, "cwd": working_directory,
    })).map_err(|error| error.to_string())?);
    let script = format!(
        r#"
const fs = require('fs'), path = require('path'), os = require('os');
const request = JSON.parse(Buffer.from('{request}', 'base64').toString());
const packageRoot = process.argv[1], userData = process.argv[2];
let name = request.path;
if (/^file:\/\//i.test(name)) name = require('url').fileURLToPath(name);
if (name === '~') name = os.homedir();
else if (/^~[/\\]/.test(name)) name = path.join(os.homedir(), name.slice(2));
const root = fs.realpathSync(request.project);
const within = candidate => {{ const relative = path.relative(root, candidate); return relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative); }};
const absolute = path.isAbsolute(name);
const candidates = [absolute ? name : path.resolve(root, name)];
if (!absolute && request.cwd && path.isAbsolute(request.cwd)) {{
  try {{ const cwd = fs.realpathSync(request.cwd); if (within(cwd)) candidates.push(path.resolve(cwd, name)); }} catch {{}}
}}
let file;
for (const candidate of candidates) {{
  try {{ const resolved = fs.realpathSync(candidate); if (absolute || within(resolved)) {{ file = resolved; break; }} }} catch {{}}
}}
if (!file) throw new Error('That file could not be found in the session project.');
const metadata = fs.statSync(file);
if (!metadata.isFile() && !metadata.isDirectory()) throw new Error('That path is not a file or folder.');
const socket = process.platform === 'win32'
  ? '\\\\.\\pipe\\ghostex-code-' + require('crypto').createHash('sha256').update(userData.replace(/\//g, '\\').toLowerCase()).digest('hex')
  : path.join(userData, 'code-server-ipc.sock');
if (metadata.isDirectory()) {{
  const body = JSON.stringify({{filePath: file, workspaceFolder: request.project, requestKey: 'ghostex-source-file-open', pipeArgs: {{type: 'browseFolder', folderURIs: [file]}}}});
  const pending = require('http').request({{socketPath: socket, path: '/queue-open', method: 'POST', headers: {{'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body)}}}}, response => {{
    response.resume();
    response.on('end', () => {{
      if (response.statusCode !== 200) console.error('GHOSTEX_SOURCE_OPEN_HTTP_' + response.statusCode);
      process.exit(response.statusCode === 200 ? 0 : 1);
    }});
  }});
  pending.setTimeout(25000, () => pending.destroy(new Error('The remote VS Code view did not accept the folder.')));
  pending.on('error', error => {{ console.error(error.message); process.exit(1); }});
  pending.end(body);
}} else {{
  const positioned = request.line ? file + ':' + request.line + (request.column ? ':' + request.column : '') : file;
  const result = require('child_process').spawnSync(process.execPath, [path.join(packageRoot, 'out/node/entry.js'), '--user-data-dir', userData, '--session-socket', socket, '--reuse-window', '--queue-open', '--open-request-key', 'ghostex-source-file-open', '--open-workspace-folder', request.project, positioned], {{stdio: 'inherit', windowsHide: true}});
  if (result.error) throw result.error;
  process.exit(result.status === null ? 1 : result.status);
}}
"#
    );
    let command = if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsPowerShell
    ) {
        format!(
            "{}\n& (Join-Path $gxCode 'lib/node.exe') -e {} $gxCode (Join-Path $gxData 'code-server/runtime/user-data')\nexit $LASTEXITCODE",
            gpui_remote_windows_code_setup(),
            gpui_powershell_quote(&script)
        )
    } else {
        format!(
            "set -eu\n{}\n\"$code_root/package/lib/node\" -e {} \"$code_root/package\" \"$code_root/runtime/user-data\"",
            source_code_server_remote_data_root_script(),
            gpui_shell_single_quote(&script)
        )
    };
    let started = std::time::Instant::now();
    support_logs::append_for_scenario(
        support_logs::GpuiSupportLog::SidebarRefresh,
        "native.sidebar.refresh",
        "gpui.sourceFileOpen.operationStarted",
        serde_json::json!({
            "windowsHost": matches!(execution_target, GpuiRemoteExecutionTarget::WindowsPowerShell),
        }),
    );
    let result = gpui_run_remote_ssh_script_in_execution_target(
        machine_config,
        execution_target,
        &command,
        Duration::from_secs(30),
    );
    let error_category = if result.exit_code == 0 {
        "none"
    } else if result.exit_code == 124 {
        "sshCommandTimeout"
    } else if result
        .stderr
        .contains("The remote VS Code view did not accept the folder.")
    {
        "folderQueueTimeout"
    } else if result.stderr.contains("GHOSTEX_SOURCE_OPEN_HTTP_") {
        "folderQueueRejected"
    } else if result.stderr.contains("That file could not be found") {
        "pathNotFound"
    } else if result.stderr.contains("That path is not a file or folder") {
        "unsupportedFileType"
    } else if result.stderr.contains("missing its native VS Code editor") {
        "missingCodePayload"
    } else if result.stderr.contains("ENOENT") {
        "missingFileOrIpc"
    } else if result.stderr.contains("EACCES") || result.stderr.contains("EPERM") {
        "accessDenied"
    } else {
        "remoteCommandFailed"
    };
    support_logs::append_for_scenario(
        support_logs::GpuiSupportLog::SidebarRefresh,
        "native.sidebar.refresh",
        "gpui.sourceFileOpen.operationFinished",
        serde_json::json!({
            "exitCode": result.exit_code,
            "elapsedMs": started.elapsed().as_millis(),
            "errorCategory": error_category,
        }),
    );
    if result.exit_code == 0 {
        Ok(())
    } else {
        Err("The remote Code editor could not open that file. Check that the path exists on the session computer.".into())
    }
}

/// Windows cannot be run here, so the placement of a Windows computer's chat links is pinned by
/// hand: the user's `C:\dev\.desk-runner\…` links from a session in `C:\dev\mastro_ed`.
#[cfg(test)]
mod tests {
    use super::gpui_remote_chat_file_place;

    #[test]
    fn windows_chat_links_are_placed_on_any_computer() {
        let project = r"C:\dev\mastro_ed";
        assert_eq!(
            gpui_remote_chat_file_place(
                r"C:\dev\.desk-runner\d1111\university-reply\reply-email.txt",
                project
            ),
            (
                "C:/dev/.desk-runner/d1111/university-reply/reply-email.txt".to_string(),
                true
            )
        );
        // A `file:///C:/…` link, inside the project, in another case.
        assert_eq!(
            gpui_remote_chat_file_place("/c:/Dev/Mastro_Ed/docs/plan.md", project),
            ("docs/plan.md".to_string(), false)
        );
        // A network share is placed so the link can say Files never opens one.
        assert_eq!(
            gpui_remote_chat_file_place(r"\\nas\share\shot.png", project),
            ("//nas/share/shot.png".to_string(), true)
        );
        // Known as Windows by the link alone, when the project folder is not.
        assert_eq!(
            gpui_remote_chat_file_place(r"D:\scratch\out.log", ""),
            ("D:/scratch/out.log".to_string(), true)
        );
        assert_eq!(
            gpui_remote_chat_file_place("src/main.rs", project),
            ("src/main.rs".to_string(), false)
        );
        // A POSIX computer keeps its paths.
        assert_eq!(
            gpui_remote_chat_file_place("/tmp/claude/dev.log", "/home/me/app"),
            ("/tmp/claude/dev.log".to_string(), true)
        );
        assert_eq!(
            gpui_remote_chat_file_place("/home/me/app/README.md", "/home/me/app"),
            ("README.md".to_string(), false)
        );
    }
}
