//! macOS: the login Keychain and LocalAuthentication (Touch ID, or the Mac password).

use std::{sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::{
    class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool},
};
use objc2_foundation::NSString;
use security_framework::passwords::{get_generic_password, set_generic_password};

use super::SERVICE;
use crate::{KeyError, Presence};

#[link(name = "LocalAuthentication", kind = "framework")]
extern "C" {}

/// `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = -25300;
/// `LAPolicyDeviceOwnerAuthentication`: Touch ID, an Apple Watch, or the Mac password.
const POLICY_DEVICE_OWNER_AUTHENTICATION: isize = 2;
/// `LABiometryTypeNone`.
const BIOMETRY_NONE: isize = 0;

/*
CDXC:Security 2026-11-08 WHY:
The approver key is a P-256 key in the login Keychain unlocked by a LocalAuthentication prompt, not a Secure Enclave key: a persistent Secure Enclave key needs the data protection keychain and a keychain-access-groups entitlement (with a provisioning profile) that the Developer ID app doesn't ship. A Keychain item the Ghostex app created is read by other programs only after the user types their keychain password, and an agent can't pass Touch ID. Moving the key into the Secure Enclave (CryptoKit through a Swift shim, or the entitlement) re-enrolls it once and changes nothing else.
*/
pub(crate) fn read_secret(account: &str) -> Result<Option<Vec<u8>>, KeyError> {
    match get_generic_password(SERVICE, account) {
        Ok(secret) => Ok(Some(secret)),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(KeyError::KeyStore(error.to_string())),
    }
}

pub(crate) fn write_secret(account: &str, secret: &[u8]) -> Result<(), KeyError> {
    set_generic_password(SERVICE, account, secret)
        .map_err(|error| KeyError::KeyStore(error.to_string()))
}

/// Shows the system's Touch ID / password sheet with `reason` and waits for the answer.
pub(crate) fn confirm_presence(reason: &str) -> Result<Presence, KeyError> {
    unsafe {
        let context: Option<Retained<AnyObject>> = msg_send![class!(LAContext), new];
        let context =
            context.ok_or_else(|| KeyError::PresenceUnavailable("LocalAuthentication".into()))?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let can: Bool = msg_send![
            &*context,
            canEvaluatePolicy: POLICY_DEVICE_OWNER_AUTHENTICATION,
            error: &mut error
        ];
        if !can.as_bool() {
            return Err(KeyError::PresenceUnavailable(
                "no Touch ID or password is set up".into(),
            ));
        }
        let biometry: isize = msg_send![&*context, biometryType];
        let reason = NSString::from_str(reason);
        let (sender, receiver) = mpsc::channel::<bool>();
        let reply = RcBlock::new(move |success: Bool, _error: *mut AnyObject| {
            let _ = sender.send(success.as_bool());
        });
        let _: () = msg_send![
            &*context,
            evaluatePolicy: POLICY_DEVICE_OWNER_AUTHENTICATION,
            localizedReason: &*reason,
            reply: &*reply
        ];
        // The sheet times out by itself; this only bounds a reply that never comes.
        match receiver.recv_timeout(Duration::from_secs(300)) {
            Ok(true) if biometry != BIOMETRY_NONE => Ok(Presence::TouchId),
            Ok(true) => Ok(Presence::MacPassword),
            _ => Err(KeyError::Cancelled),
        }
    }
}
