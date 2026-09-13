#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewerDetachPreparation {
    /// This attached client cannot confirm receipt of previously accepted input.
    Unsupported,
    /// The retiring owner must stay alive until its ordered detach completes.
    Pending,
    Ready,
}

/// CDXC:Zmx 2026-09-13 WHY:
/// A PTY write only proves kernel acceptance; retire a client with accepted input only after its random nonce acknowledges ordered daemon receipt and graceful detach.
/// Capability belongs to one attach generation, and retirement freezes subsequent input and VT replies so no bytes reach a reconnect wrapper after Detach.
/// SEE-ALSO: .dependencies/zmx/src/client_detach.zig and apps/desktop/src/app/helpers/remote/ssh_process.rs TEMP_REMOTE_SSH_READY_TITLE.
#[derive(Default)]
pub(super) struct ViewerDetachState {
    pub(super) capable: bool,
    pub(super) accepted_input: bool,
    pub(super) retiring: bool,
    pub(super) nonce: Option<String>,
    pub(super) acknowledged: bool,
}

impl ViewerDetachState {
    pub(super) fn request(&mut self) -> std::io::Result<Vec<u8>> {
        let mut random = [0u8; 16];
        #[cfg(unix)]
        if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        #[cfg(not(unix))]
        return Err(std::io::ErrorKind::Unsupported.into());
        let nonce = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut request = vec![0x9d];
        request.extend_from_slice(format!("1337;ZMX_DETACH={nonce}\x07").as_bytes());
        self.nonce = Some(nonce);
        Ok(request)
    }

    pub(super) fn observe(&mut self, osc: &[u8]) -> bool {
        if osc == b"1337;ZMX_DETACH_CAP=1" {
            self.capable = true;
            self.nonce = None;
            self.acknowledged = false;
        } else if osc == b"1337;ZMX_DETACH_CAP=0"
            || osc == b"2;TEMP_REMOTE_SSH_READY_20260814"
            || osc == b"0;TEMP_REMOTE_SSH_READY_20260814"
        {
            self.capable = false;
            self.nonce = None;
            self.acknowledged = false;
        } else if let Some(nonce) = osc.strip_prefix(b"1337;ZMX_DETACH_ACK=") {
            if !self
                .nonce
                .as_ref()
                .is_some_and(|expected| expected.as_bytes() == nonce)
            {
                return false;
            }
            self.acknowledged = true;
            self.accepted_input = false;
            self.capable = false;
        } else {
            return false;
        }
        true
    }
}

/// Observes bounded OSC controls across arbitrary PTY read boundaries without
/// changing the byte stream consumed by libghostty-vt.
#[derive(Default)]
pub(super) struct ViewerDetachParser {
    osc: Vec<u8>,
    escaped: bool,
    in_osc: bool,
}

impl ViewerDetachParser {
    pub(super) fn feed(&mut self, bytes: &[u8], state: &mut ViewerDetachState) -> bool {
        let mut changed = false;
        for &byte in bytes {
            if self.in_osc {
                if byte == 7 || (self.escaped && byte == b'\\') {
                    changed |= state.observe(&self.osc);
                    self.osc.clear();
                    self.in_osc = false;
                    self.escaped = false;
                } else if byte == 27 {
                    self.escaped = true;
                } else if self.escaped || self.osc.len() >= 128 {
                    self.osc.clear();
                    self.in_osc = false;
                    self.escaped = false;
                } else {
                    self.osc.push(byte);
                }
            } else if self.escaped {
                self.in_osc = byte == b']';
                self.escaped = byte == 27;
            } else {
                self.escaped = byte == 27;
            }
        }
        changed
    }
}
