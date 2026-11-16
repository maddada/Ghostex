//! Connect a computer's SSH details tab: the Add a computer form (name, SSH host, username,
//! password, and Advanced), the same one Settings > Remote's Add a computer dialog shows. Both use
//! `settings_modal/tabs/remote/ssh_form.rs` for the fields' limits, wording and checks.
//!
//! CDXC:RemotePairing 2026-11-16 DECISION:
//! User, of the SSH details tab: "Form in the tab". The tab holds the form itself; its Add computer
//! saves the computer (and its password, to the keychain) as Settings' dialog does and closes the
//! window, and the app connects it, asking before it installs gxserver on a computer that has none.
use super::super::settings_modal::tabs::remote::{
    SshComputerForm, SshFormField, ssh_form_words as words,
};
use super::*;
use gpui_component::input::InputState;

impl GpuiConnectDeviceWindow {
    /// Picks a tab; the form is built the first time SSH details shows.
    pub(super) fn set_computer_tab(
        &mut self,
        tab: ComputerTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.computer_tab = tab;
        if tab == ComputerTab::Ssh && self.ssh_form.is_none() {
            self.ssh_form = Some(SshComputerForm::new(
                window,
                cx,
                |this: &mut Self,
                 field: SshFormField,
                 text: String,
                 input: gpui::Entity<InputState>,
                 window,
                 cx| {
                    if let Some(form) = this.ssh_form.as_mut() {
                        form.field_changed(field, text, &input, window, cx);
                    }
                    cx.notify();
                },
            ));
        }
        cx.notify();
    }

    /// Add computer: saves the computer and closes the window; the app connects it.
    fn add_ssh_computer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((machine, password)) = self.ssh_form.as_ref().and_then(SshComputerForm::computer)
        else {
            return;
        };
        self.close_and_send(
            ConnectDeviceCommand::AddSshComputer { machine, password },
            window,
            cx,
        );
    }

    /// The bits of the form that change the window's height, so it is measured again.
    pub(super) fn ssh_fit_key(&self) -> u8 {
        match (&self.ssh_form, self.computer_tab) {
            (Some(form), ComputerTab::Ssh) => {
                1 + u8::from(form.advanced_open) + 2 * u8::from(form.can_save().0)
            }
            _ => 0,
        }
    }

    /// A labelled field: its label, the input, and a quiet line of what it takes.
    fn ssh_field(
        &self,
        label: &'static str,
        input: &gpui::Entity<InputState>,
        hint: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .min_w_0()
            .gap(px(6.0))
            .child(self.label(label))
            .child(modal_text_input(&p, input, false, window, cx))
            .children(hint.map(|hint| self.muted(hint).text_size(px(12.0)).line_height(px(17.0))))
            .into_any_element()
    }

    /// A title and its explanation with the app's switch at the right; the whole row toggles it.
    fn ssh_switch_row(
        &self,
        id: &'static str,
        title: &'static str,
        info: &'static str,
        on: bool,
        toggle: impl Fn(&mut SshComputerForm) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        h_flex()
            .id(id)
            .role(gpui::Role::Switch)
            .aria_toggled(a11y_toggled(on))
            .aria_label(title)
            .w_full()
            .items_center()
            .gap(px(12.0))
            .switch_row_press(
                switch_row_wash(p.foreground),
                false,
                bind_press(cx, move |this: &mut Self, _window, cx| {
                    if let Some(form) = this.ssh_form.as_mut() {
                        toggle(form);
                    }
                    cx.notify();
                }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(self.label(title))
                    .child(self.muted(info).text_size(px(12.0)).line_height(px(17.0))),
            )
            .child(div().flex_shrink_0().child(modal_switch(&p, on, false)))
            .into_any_element()
    }

    /// SSH details: the form, Advanced, and Add computer.
    pub(super) fn ssh_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(form) = self.ssh_form.as_ref() else {
            return div().into_any_element();
        };
        let name = form.inputs.name.clone();
        let host = form.inputs.host.clone();
        let user = form.inputs.user.clone();
        let password = form.inputs.password.clone();
        let port = form.inputs.port.clone();
        let identity = form.inputs.identity.clone();
        let wsl = form.inputs.wsl.clone();
        let advanced_open = form.advanced_open;
        let agent_access = form.agent_access();
        let reuse = form.reuse_ssh_connection();
        let offers_reuse = form.offers_reuse_ssh_connection();
        let (can_save, reason) = form.can_save();

        let mut advanced = v_flex()
            .w_full()
            .gap(px(12.0))
            .pt(px(4.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .child(div().pt(px(8.0)).flex().child(self.link(
                "connect-computer-ssh-advanced",
                if advanced_open {
                    "Hide advanced"
                } else {
                    "Advanced"
                },
                |this, _window, cx| {
                    if let Some(form) = this.ssh_form.as_mut() {
                        form.advanced_open = !form.advanced_open;
                    }
                    cx.notify();
                },
                cx,
            )));
        if advanced_open {
            advanced = advanced
                .child(self.ssh_switch_row(
                    "connect-computer-ssh-agent-access",
                    words::AGENT_ACCESS_TITLE,
                    words::AGENT_ACCESS_INFO,
                    agent_access,
                    SshComputerForm::toggle_agent_access,
                    cx,
                ))
                .child(self.ssh_field("SSH port", &port, None, window, cx))
                .child(self.ssh_field(
                    "Identity file",
                    &identity,
                    Some(words::IDENTITY_INFO),
                    window,
                    cx,
                ))
                .child(self.ssh_field(
                    "Windows WSL distribution",
                    &wsl,
                    Some(words::WSL_INFO),
                    window,
                    cx,
                ));
            if offers_reuse {
                advanced = advanced.child(self.ssh_switch_row(
                    "connect-computer-ssh-reuse",
                    words::REUSE_SSH_TITLE,
                    words::REUSE_SSH_INFO,
                    reuse,
                    SshComputerForm::toggle_reuse_ssh_connection,
                    cx,
                ));
            }
        }

        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(self.muted(format!(
                "{} If Ghostex's server, gxserver, isn't installed there, Ghostex asks to install it when it connects.",
                words::NEW_INTRO
            )))
            .child(self.ssh_field("Name", &name, None, window, cx))
            .child(self.ssh_field("SSH host", &host, None, window, cx))
            .child(self.ssh_field(
                "SSH username",
                &user,
                Some(words::USER_INFO),
                window,
                cx,
            ))
            .child(self.ssh_field(
                "SSH password",
                &password,
                Some(words::password_info(true)),
                window,
                cx,
            ))
            .child(advanced)
            .children((!can_save).then(|| self.muted(reason).text_size(px(12.0))))
            .child(modal_footer(vec![modal_action_button(
                &p,
                "connect-computer-ssh-add",
                "Add computer",
                None,
                ModalButtonTone::Primary,
                !can_save,
                |this: &mut Self, window, cx| this.add_ssh_computer(window, cx),
                cx,
            )]))
            .into_any_element()
    }
}
