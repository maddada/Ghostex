//! The approval notice: a request an agent (or a phone, or the CLI) made on this computer or a
//! connected one shows in the centred notice modal with Deny and Approve (app/notice_modal_host.rs);
//! Approve runs the system prompt and signs it for that computer's gxserver. The notice goes when
//! the request is decided anywhere (here, on the phone) or expires.

use ghostex_gx_protocol::approval::ApprovalRequest;

use super::Transport;
use crate::GhostexGpuiApp;
use crate::app::window::toast::{GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel};

/// Every request this app was told of or listed as waiting (any computer): the renewal shows only
/// the others (`owner_security_show_missed_requests`).
static SEEN_REQUESTS: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
    std::sync::Mutex::new(None);

fn card_id(machine_id: Option<&str>, request_id: &str) -> String {
    format!(
        "security-approval-{}-{request_id}",
        machine_id.unwrap_or("local")
    )
}

/*
CDXC:RemotePairing 2026-11-09 WHY:
User, about the phone's pairing card: "i don't understand as a user what the heck I need to do
here". The phone now says that Ghostex on the computer shows "Approve this phone?" with the same
three words, so a phone's pairing request is titled that here instead of the generic
"Approval needed" (a computer asking to connect through Easy Connect reads "Approve this
computer?"). gxserver writes the summary: a phone's starts with "Pair", a computer's with "Let".
*/
/// Why a card's Approve or Deny didn't go through.
enum Decided {
    /// A newer request replaced the card's ([`super::CardRequest::Moved`]).
    Moved(ApprovalRequest),
    /// The request no longer waits; the plain words why.
    Gone(String),
    /// gxserver or the prompt refused.
    Failed(String),
}

/// What a card's decision did: the unlock card let this app in with its old owner key.
const UNLOCKED: bool = true;

/// The notice's title: what the person is asked to approve.
fn approval_title(request: &ApprovalRequest) -> &'static str {
    use ghostex_gx_protocol::approval::SensitiveAction;
    if request.action != SensitiveAction::PairingApproveDevice {
        return "Approval needed";
    }
    // The same question the Connect a phone window asks (CDXC:RemotePairing 2026-11-10 in
    // window/connect_device_modal.rs).
    if request.summary.starts_with("Pair ") {
        "Is this your phone?"
    } else {
        "Is this your computer?"
    }
}

impl GhostexGpuiApp {
    /// `approvalRequestsChanged` from this computer's gxserver (`machine_id: None`) or a
    /// connected computer's.
    pub(crate) fn owner_security_on_approval_change(
        &mut self,
        machine_id: Option<String>,
        change: &str,
        request: Option<ApprovalRequest>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = request else {
            return;
        };
        let id = card_id(machine_id.as_deref(), &request.id);
        if change == "requested" {
            SEEN_REQUESTS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_or_insert_with(Default::default)
                .insert(request.id.clone());
        }
        if change != "requested" {
            // That computer let this app act as the owner there: open the owner session now.
            if change == "approved" && super::asks_for_this_app(&request) {
                // This computer's own owner let this app in here (m7-arch's app on m7-arch).
                if machine_id.is_none() {
                    super::note_owner_key_approved(&Transport::Local);
                }
                if let Some(target) = machine_id
                    .as_deref()
                    .and_then(|machine| self.gpui_remote_gxserver_request_target(machine))
                {
                    super::note_owner_key_approved(&Transport::Remote(target));
                    // Its requests that were notices ("Approve on …") can be approved here now.
                    self.owner_security_show_pending_requests(machine_id.clone(), cx);
                }
            }
            self.remove_gpui_app_toast(&id, cx);
            // Shown through another entry of the same computer (`owner_security_show_card`).
            if let Some(machine) = machine_id.as_deref() {
                for sibling in crate::app::remote_conn::connected_computers::gpui_remote_machines_sharing_server_id(machine) {
                    self.remove_gpui_app_toast(&card_id(Some(&sibling), &request.id), cx);
                }
            }
            return;
        }
        // The app's own clicks are approved right where they happen (`approve_own_call`); its
        // background work shows its own card (`owner_security_show_waiting_request`). Another
        // computer's app acting as the owner here is someone else asking, and shows.
        if super::asked_by_this_app(&request) || super::asks_for_this_app(&request) {
            return;
        }
        // The Connect a phone window shows a pairing request itself, words and Approve included:
        // one place for one request (CDXC:RemotePairing 2026-11-10 in
        // window/connect_device_modal.rs). Its notice comes back if the window closes first.
        if machine_id.is_none()
            && request.action
                == ghostex_gx_protocol::approval::SensitiveAction::PairingApproveDevice
            && self.native_app_modal_kind()
                == Some(crate::app::model::GpuiAppModalKind::RemoteSetup)
        {
            return;
        }
        self.owner_security_show_card(machine_id, request, cx);
    }

    /// Takes down this computer's notice for request `request_id` (the Connect a phone window shows
    /// it now); it is still waiting, so it comes back when that window closes.
    pub(crate) fn owner_security_hide_local_card(
        &mut self,
        request_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.remove_gpui_app_toast(&card_id(None, request_id), cx);
    }

    /*
    CDXC:Security 2026-11-17 FIX:
    The user ran `ghostex computer-code` on their headless Windows PC and no card came on the Mac for about ten minutes, until the Mac app restarted and listed what waited there. A connected computer's request shows from its `approvalRequestsChanged` event, and its waiting requests are listed only when its stream goes live, so a missed event (or a card queued behind another) left nothing until the next restart. The renewal (once a minute) now also lists each connected computer's waiting requests and shows any this app never showed; one already shown isn't raised again, so a card the user closed stays closed.
    */
    /// Requests still waiting on `machine_id` that this app never showed (their event was missed):
    /// shown now, once each.
    pub(crate) fn owner_security_show_missed_requests(
        &mut self,
        machine_id: String,
        requests: Vec<ApprovalRequest>,
        cx: &mut gpui::Context<Self>,
    ) {
        let missed: Vec<ApprovalRequest> = {
            let seen = SEEN_REQUESTS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            requests
                .into_iter()
                .filter(|request| seen.as_ref().is_none_or(|seen| !seen.contains(&request.id)))
                .collect()
        };
        for request in missed {
            self.owner_security_on_approval_change(
                Some(machine_id.clone()),
                "requested",
                Some(request),
                cx,
            );
        }
    }

    /// This computer's pairing requests still waiting show as notices: the Connect a phone window,
    /// which showed them, went away.
    pub(crate) fn owner_security_show_pending_pairing_requests(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let listed = cx
            .background_executor()
            .spawn(async move { super::pending_requests(&Transport::Local) });
        cx.spawn(async move |this, cx| {
            let requests = listed.await.unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                for request in requests.into_iter().filter(|request| {
                    request.action
                        == ghostex_gx_protocol::approval::SensitiveAction::PairingApproveDevice
                }) {
                    this.owner_security_on_approval_change(None, "requested", Some(request), cx);
                }
            });
        })
        .detach();
    }

    /// A computer's stream went live (`machine_id: None` for this one): its requests still
    /// waiting show as cards, including any whose event this app missed (CDXC:Security
    /// 2026-11-09 at `owner_security::pending_requests`), and cards for requests no longer
    /// waiting there go (CDXC:Security 2026-11-10 at `owner_security::card_request`).
    pub(crate) fn owner_security_show_pending_requests(
        &mut self,
        machine_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let transport = match machine_id.as_deref() {
            None => Transport::Local,
            Some(machine) => match self.gpui_remote_gxserver_request_target(machine) {
                Some(target) => Transport::Remote(target),
                None => return,
            },
        };
        let listed = cx
            .background_executor()
            .spawn(async move { super::pending_requests(&transport) });
        cx.spawn(async move |this, cx| {
            let Some(requests) = listed.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                // A card whose request no longer waits there (it lapsed or was answered while this
                // app was away, or gxserver restarted without it) goes; the rest show again.
                let prefix = card_id(machine_id.as_deref(), "");
                let live: std::collections::HashSet<String> = requests
                    .iter()
                    .map(|request| card_id(machine_id.as_deref(), &request.id))
                    .collect();
                let stale: Vec<String> = this
                    .app_notices
                    .queue
                    .iter()
                    .chain(this.app_toasts.iter())
                    .map(|toast| toast.id.clone())
                    .filter(|id| {
                        // `apr_` and 16 characters: never another computer's whose id starts alike.
                        id.strip_prefix(&prefix)
                            .is_some_and(|rest| rest.starts_with("apr_") && rest.len() == 20)
                            && !live.contains(id)
                    })
                    .collect();
                for id in stale {
                    this.remove_gpui_app_toast(&id, cx);
                }
                for request in requests {
                    this.owner_security_on_approval_change(
                        machine_id.clone(),
                        "requested",
                        Some(request),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// The card for a request the app's own background work waits on (a connected computer's
    /// renewal, `remote_conn/connected_computers.rs`): shown once, whoever made it, since that
    /// work never shows the system prompt.
    pub(crate) fn owner_security_show_waiting_request(
        &mut self,
        machine_id: Option<String>,
        request: &ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        // Once per request: a card the user closed doesn't come back on the next renewal.
        static SHOWN: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
            std::sync::Mutex::new(None);
        let first = SHOWN
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(Default::default)
            .insert(card_id(machine_id.as_deref(), &request.id));
        if first {
            self.owner_security_show_card(machine_id, request.clone(), cx);
        }
    }

    /// The card for a request: with Approve where this app can answer it (this computer, or a
    /// connected one where it acts as the owner with its approver key), else a notice naming where
    /// to approve it (CDXC:Security 2026-11-10 at `owner_security::answerer`).
    fn owner_security_show_card(
        &mut self,
        machine_id: Option<String>,
        request: ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(machine) = machine_id.clone() else {
            self.owner_security_show_approvable_card(None, request, cx);
            return;
        };
        /*
        CDXC:Security 2026-11-17 FIX:
        The user's Windows PC was in Settings > Remote twice (over SSH, where this app is one of its owners, and through Easy Connect, added later with a key it never approved). Each entry showed that computer's requests: one as a card with Approve, one as "Approve on M7-Desktop", which sent the user to the Windows screen they never use. A request now shows once, through the first entry (by id) of that computer that can approve it, or as one notice when none can; each entry works out the same one, and showing it takes down the others' card for that request.
        */
        let siblings =
            crate::app::remote_conn::connected_computers::gpui_remote_machines_sharing_server_id(
                &machine,
            );
        let targets: Vec<_> = siblings
            .iter()
            .filter_map(|sibling| {
                self.gpui_remote_gxserver_request_target(sibling)
                    .map(|target| (sibling.clone(), target))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let checked = cx.background_executor().spawn({
            let request = request.clone();
            async move {
                let mut first_elsewhere = None;
                for (sibling, target) in targets {
                    match super::answerer_for(&Transport::Remote(target), &request) {
                        super::Answerer::ThisApp => return (sibling, super::Answerer::ThisApp),
                        elsewhere => {
                            first_elsewhere.get_or_insert((sibling, elsewhere));
                        }
                    }
                }
                first_elsewhere.expect("at least one entry was checked")
            }
        });
        cx.spawn(async move |this, cx| {
            let (chosen, answerer) = checked.await;
            let _ = this.update(cx, |this, cx| {
                for sibling in &siblings {
                    if *sibling != chosen {
                        this.remove_gpui_app_toast(&card_id(Some(sibling), &request.id), cx);
                    }
                }
                match answerer {
                    super::Answerer::ThisApp => {
                        this.owner_security_show_approvable_card(Some(chosen), request, cx)
                    }
                    super::Answerer::Elsewhere {
                        devices,
                        asked,
                        note,
                    } => this.owner_security_show_approve_elsewhere(
                        &chosen,
                        &request,
                        &devices,
                        asked,
                        note.as_deref(),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    /// A connected computer's request this app can't answer: a notice (no Approve) that says what
    /// it asks and where to approve it; it goes when the request is answered there.
    fn owner_security_show_approve_elsewhere(
        &mut self,
        machine_id: &str,
        request: &ApprovalRequest,
        devices: &[String],
        asked: bool,
        note: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let computer = crate::app::helpers::gpui_remote_machine_name_from_settings(machine_id)
            .unwrap_or_else(|| machine_id.to_string());
        let summary = request.summary.trim().trim_end_matches('.');
        let where_to = match devices {
            [] => format!("in Ghostex on the computer or phone that owns {computer}"),
            several => format!("in Ghostex on {}", several.join(" or ")),
        };
        let then = if asked {
            format!(
                " It also asks there, once, to let this computer answer {computer}'s requests from here."
            )
        } else {
            String::new()
        };
        let then = match note.filter(|note| !note.is_empty()) {
            Some(note) => format!("{then} {note}"),
            None => then,
        };
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: card_id(Some(machine_id), &request.id),
                level: GpuiAppToastLevel::Info,
                // Not "Approve on {computer}": its owners may all be elsewhere (a headless
                // computer, CDXC:Security 2026-11-17 at `approvers_here` in gxserver).
                title: format!("{computer} asks for your approval"),
                description: Some(format!(
                    "{computer} asks: {summary}. Approve it {where_to}.{then}"
                )),
                copy_text: None,
                action: None,
                loading: false,
                persistent: true,
                duration_ms: 0,
                epoch: 0,
            },
            cx,
        );
    }

    /// The card that lets this app in on a locked connected computer with its old shared owner
    /// key, once per request; only its Approve signs with that key (CDXC:Security 2026-11-17 at
    /// `owner_security::approve_with_shared_owner_key`).
    pub(crate) fn owner_security_show_unlock_card(
        &mut self,
        machine_id: String,
        request: ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        static SHOWN: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
            std::sync::Mutex::new(None);
        let id = card_id(Some(&machine_id), &request.id);
        let first = SHOWN
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(Default::default)
            .insert(id.clone());
        let Some(target) = self.gpui_remote_gxserver_request_target(&machine_id) else {
            return;
        };
        if !first {
            return;
        }
        let (title, description) = super::old_key_unlock_card(&target, &request);
        let computer = crate::app::helpers::gpui_remote_machine_name_from_settings(&machine_id)
            .unwrap_or_else(|| machine_id.clone());
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                level: GpuiAppToastLevel::Warning,
                title,
                description: Some(description),
                copy_text: None,
                action: Some(GpuiAppToastAction::SecurityApproval {
                    request: Box::new(request),
                    deny: false,
                    machine_id: Some(machine_id),
                    computer: Some(computer),
                    // This Mac's own key: this Mac's own switch (second review, finding 6).
                    require_presence: super::plain::this_computer_requires_presence(),
                }),
                loading: false,
                persistent: true,
                duration_ms: 0,
                epoch: 0,
            },
            cx,
        );
    }

    fn owner_security_show_approvable_card(
        &mut self,
        machine_id: Option<String>,
        request: ApprovalRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let id = card_id(machine_id.as_deref(), &request.id);
        // The notice modal draws the request's own words with who asked and on which computer
        // (app/notice_modal_host.rs `notice_item`); the description is that line for any other
        // reader of the toast.
        let mut description = request.summary.clone();
        if let Some(detail) = request
            .detail
            .as_deref()
            .filter(|detail| !detail.is_empty())
        {
            description.push_str(". ");
            description.push_str(detail);
        }
        let computer = machine_id.as_deref().map(|machine| {
            crate::app::helpers::gpui_remote_machine_name_from_settings(machine)
                .unwrap_or_else(|| machine.to_string())
        });
        let require_presence = match machine_id.as_deref() {
            None => super::plain::asks_presence_first(&Transport::Local),
            Some(machine) => self
                .gpui_remote_gxserver_request_target(machine)
                .is_none_or(|target| super::plain::asks_presence_first(&Transport::Remote(target))),
        };
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id,
                level: GpuiAppToastLevel::Warning,
                title: approval_title(&request).to_string(),
                description: Some(description),
                copy_text: None,
                action: Some(GpuiAppToastAction::SecurityApproval {
                    request: Box::new(request),
                    deny: false,
                    machine_id,
                    computer,
                    require_presence,
                }),
                loading: false,
                persistent: true,
                duration_ms: 0,
                epoch: 0,
            },
            cx,
        );
    }

    /// Approve (the system prompt, then the signature) or Deny on an approval card.
    pub(crate) fn owner_security_decide(
        &mut self,
        request: &ApprovalRequest,
        deny: bool,
        machine_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let transport = match machine_id {
            None => Transport::Local,
            Some(machine) => match self.gpui_remote_gxserver_request_target(machine) {
                Some(target) => Transport::Remote(target),
                None => {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Computer not connected",
                        "Connect to that computer again, then approve.",
                        cx,
                    );
                    return;
                }
            },
        };
        let request = request.clone();
        let card = card_id(machine_id, &request.id);
        let computer = machine_id.map(|machine| {
            crate::app::helpers::gpui_remote_machine_name_from_settings(machine)
                .unwrap_or_else(|| machine.to_string())
        });
        let lets_computer_in = matches!(
            request.action,
            ghostex_gx_protocol::approval::SensitiveAction::ComputerGrant
                | ghostex_gx_protocol::approval::SensitiveAction::ComputerRegister
        );
        let decided = cx.background_executor().spawn(async move {
            // The unlock card ("Let this Mac in on …?"): a click after a lost answer finds its
            // request approved, and the key already in there; it still finishes (third review,
            // finding 1), so it skips the "no longer waiting" check.
            if !deny && super::asks_for_this_app(&request) {
                return super::unlock_with_old_key(&transport, &request)
                    .map(|()| UNLOCKED)
                    .map_err(Decided::Failed);
            }
            match super::card_request(&transport, &request.id) {
                super::CardRequest::Waiting => {}
                super::CardRequest::Moved(newer) => return Err(Decided::Moved(newer)),
                super::CardRequest::Gone(why) => return Err(Decided::Gone(why)),
            }
            if deny {
                super::deny_request(&transport, &request.id)
            } else {
                super::approve_request(&transport, &request)
            }
            .map(|()| !UNLOCKED)
            .map_err(Decided::Failed)
        });
        let machine_id = machine_id.map(str::to_string);
        cx.spawn(async move |this, cx| {
            let outcome = decided.await;
            let _ = this.update(cx, |this, cx| {
                // The unlock card's request may have been asked again: this card goes now.
                if matches!(outcome, Ok(UNLOCKED)) {
                    this.remove_gpui_app_toast(&card, cx);
                }
                // A connected computer waits on this request: send its call now instead of on
                // its next renewal, well inside the approval's two minutes.
                if outcome.is_ok() && lets_computer_in && !deny {
                    this.renew_connected_computers(true, cx);
                }
                if let Err(Decided::Moved(newer)) = outcome {
                    // The newer request's words may differ (a phone's new three words): its card
                    // shows to be read and approved, never approved for the user.
                    this.remove_gpui_app_toast(&card, cx);
                    this.owner_security_show_card(machine_id.clone(), newer, cx);
                    this.dispatch_gpui_workspace_action_toast(
                        "info",
                        "A newer request",
                        "This request was replaced by a newer one for the same thing. Its card shows now: check it, then approve.",
                        cx,
                    );
                } else if let Err(Decided::Gone(why)) = outcome {
                    this.remove_gpui_app_toast(&card, cx);
                    this.dispatch_gpui_workspace_action_toast("info", "No longer waiting", &why, cx);
                } else if let Err(Decided::Failed(error)) = outcome {
                    // Another owner there: name both computers and the card that lets this app in.
                    let error = match computer.as_deref() {
                        Some(computer) if error == super::OWNER_WAITING_ERROR => {
                            super::owner_waiting_message(computer)
                        }
                        _ => error,
                    };
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        if deny {
                            "Couldn't deny it"
                        } else {
                            "Not approved"
                        },
                        &error,
                        cx,
                    );
                }
            });
        })
        .detach();
    }
}
