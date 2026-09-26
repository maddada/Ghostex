use std::{
    io, mem,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{GENERIC_ALL, HANDLE, LUID},
    Security::{
        AddAccessAllowedAce, CreateRestrictedToken, CreateWellKnownSid, EqualSid, GetLengthSid,
        GetSidIdentifierAuthority, GetSidSubAuthority, GetSidSubAuthorityCount,
        GetTokenInformation, InitializeAcl, LookupPrivilegeValueW, SetTokenInformation,
        TokenDefaultDacl, TokenElevation, TokenGroups, TokenIntegrityLevel, TokenOwner,
        TokenPrivileges, TokenUser, WinBuiltinAccountOperatorsSid, WinBuiltinAdministratorsSid,
        WinBuiltinBackupOperatorsSid, WinBuiltinCryptoOperatorsSid,
        WinBuiltinNetworkConfigurationOperatorsSid, WinBuiltinPowerUsersSid,
        WinBuiltinPreWindows2000CompatibleAccessSid, WinBuiltinPrintOperatorsSid,
        WinBuiltinSystemOperatorsSid, WinLocalAccountAndAdministratorSid, WinLocalSystemSid,
        WinMediumLabelSid, ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, LUID_AND_ATTRIBUTES, PSID,
        SID_AND_ATTRIBUTES, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL,
        TOKEN_DUPLICATE, TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_INFORMATION_CLASS,
        TOKEN_MANDATORY_LABEL, TOKEN_OWNER, TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER,
        WELL_KNOWN_SID_TYPE,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// Groups UAC turns into deny-only entries when it filters an administrator's token.
const ADMINISTRATIVE_GROUPS: [WELL_KNOWN_SID_TYPE; 10] = [
    WinBuiltinAdministratorsSid,
    WinLocalAccountAndAdministratorSid,
    WinBuiltinPowerUsersSid,
    WinBuiltinAccountOperatorsSid,
    WinBuiltinSystemOperatorsSid,
    WinBuiltinPrintOperatorsSid,
    WinBuiltinBackupOperatorsSid,
    WinBuiltinNetworkConfigurationOperatorsSid,
    WinBuiltinCryptoOperatorsSid,
    WinBuiltinPreWindows2000CompatibleAccessSid,
];

/// Domain groups UAC filters: Enterprise Read-only DCs, Domain Admins, Domain Controllers,
/// Cert Publishers, Schema Admins, Enterprise Admins, Group Policy Creator Owners,
/// Read-only DCs and RAS and IAS Servers.
const DOMAIN_ADMINISTRATIVE_RIDS: [u32; 9] = [498, 512, 516, 517, 518, 519, 520, 521, 553];

/// The privileges a standard user's token keeps; every other one is removed.
const STANDARD_USER_PRIVILEGES: [&str; 5] = [
    "SeChangeNotifyPrivilege",
    "SeShutdownPrivilege",
    "SeUndockPrivilege",
    "SeIncreaseWorkingSetPrivilege",
    "SeTimeZonePrivilege",
];

/// `SE_GROUP_INTEGRITY`, which windows-sys only exports behind a feature the server does not need.
const SE_GROUP_INTEGRITY: u32 = 0x20;

/// Large enough for any SID (SECURITY_MAX_SID_SIZE is 68 bytes), kept u32-aligned.
type SidBuffer = [u32; 17];

pub(crate) fn current_process_is_elevated() -> io::Result<bool> {
    token_is_elevated(&current_process_token()?)
}

/// CDXC:PlatformSupport 2026-09-26 DECISION:
/// The user decided agents on Windows run without administrator rights, so Codex works "and not fight their updates", and without `--no-daemon`. Codex 0.157 refuses to start its shared background server from an elevated process, and Windows OpenSSH hands Administrators-group accounts a full admin token, so a gxserver started by a remote client (or from an admin terminal) passed admin rights to every wmx session and agent. When the caller is elevated, gxserver is started with the token UAC would give the same user: administrative groups deny-only, only standard-user privileges, Medium integrity, and the user as owner of what it creates.
///
/// CDXC:PlatformSupport 2026-09-26 WHY:
/// The token is derived from the caller's own so CreateProcessAsUserW needs no SeAssignPrimaryTokenPrivilege and the server stays outside any job. Relaunching through Task Scheduler or with the desktop shell's token was tried: both put the server in a job that forbids breakaway, which makes Codex refuse again ("host Job Object prevents daemon detachment").
pub(crate) fn standard_user_token_if_elevated() -> io::Result<Option<OwnedHandle>> {
    let process = current_process_token()?;
    if !token_is_elevated(&process)? {
        return Ok(None);
    }
    let token = standard_user_token(&process)?;
    if token_is_elevated(&token)? {
        return Err(io::Error::other(
            "Windows still reports administrator rights after removing them from gxserver's token",
        ));
    }
    Ok(Some(token))
}

fn current_process_token() -> io::Result<OwnedHandle> {
    let mut token: HANDLE = ptr::null_mut();
    let access = TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT;
    if unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

fn token_is_elevated(token: &OwnedHandle) -> io::Result<bool> {
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut length = 0;
    let queried = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenElevation,
            ptr::from_mut(&mut elevation).cast(),
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    if queried == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(elevation.TokenIsElevated != 0)
}

fn standard_user_token(process: &OwnedHandle) -> io::Result<OwnedHandle> {
    let groups = token_information(process, TokenGroups)?;
    let groups = unsafe {
        let header = groups.as_ptr().cast::<TOKEN_GROUPS>();
        std::slice::from_raw_parts(
            ptr::addr_of!((*header).Groups).cast::<SID_AND_ATTRIBUTES>(),
            (*header).GroupCount as usize,
        )
    };
    let administrative = ADMINISTRATIVE_GROUPS
        .iter()
        .map(|kind| well_known_sid(*kind))
        .collect::<io::Result<Vec<_>>>()?;
    let disable = groups
        .iter()
        .filter(|group| {
            administrative
                .iter()
                .any(|sid| unsafe { EqualSid(group.Sid, sid.as_ptr().cast_mut().cast()) } != 0)
                || is_domain_administrative_group(group.Sid)
        })
        .map(|group| SID_AND_ATTRIBUTES {
            Sid: group.Sid,
            Attributes: 0,
        })
        .collect::<Vec<_>>();

    let privileges = token_information(process, TokenPrivileges)?;
    let privileges = unsafe {
        let header = privileges.as_ptr().cast::<TOKEN_PRIVILEGES>();
        std::slice::from_raw_parts(
            ptr::addr_of!((*header).Privileges).cast::<LUID_AND_ATTRIBUTES>(),
            (*header).PrivilegeCount as usize,
        )
    };
    let kept = STANDARD_USER_PRIVILEGES
        .iter()
        .map(|name| privilege_luid(name))
        .collect::<io::Result<Vec<_>>>()?;
    let delete = privileges
        .iter()
        .filter(|privilege| {
            !kept.iter().any(|luid| {
                luid.LowPart == privilege.Luid.LowPart && luid.HighPart == privilege.Luid.HighPart
            })
        })
        .copied()
        .collect::<Vec<_>>();

    let mut restricted: HANDLE = ptr::null_mut();
    let created = unsafe {
        CreateRestrictedToken(
            process.as_raw_handle(),
            0,
            disable.len() as u32,
            disable.as_ptr(),
            delete.len() as u32,
            delete.as_ptr(),
            0,
            ptr::null(),
            &mut restricted,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    let restricted = unsafe { OwnedHandle::from_raw_handle(restricted) };

    let medium = well_known_sid(WinMediumLabelSid)?;
    let medium_sid: PSID = medium.as_ptr().cast_mut().cast();
    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: medium_sid,
            Attributes: SE_GROUP_INTEGRITY,
        },
    };
    set_token_information(
        &restricted,
        TokenIntegrityLevel,
        ptr::from_ref(&label).cast(),
        mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + unsafe { GetLengthSid(medium_sid) },
    )?;

    // An elevated token makes Administrators the owner and sole grantee of new objects, which
    // the deny-only group could no longer open; give them to the user as UAC's token does.
    let user = token_information(process, TokenUser)?;
    let user_sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let owner = TOKEN_OWNER { Owner: user_sid };
    set_token_information(
        &restricted,
        TokenOwner,
        ptr::from_ref(&owner).cast(),
        mem::size_of::<TOKEN_OWNER>() as u32,
    )?;
    let system = well_known_sid(WinLocalSystemSid)?;
    let mut dacl = default_dacl(&[user_sid, system.as_ptr().cast_mut().cast()])?;
    let default_dacl = TOKEN_DEFAULT_DACL {
        DefaultDacl: dacl.as_mut_ptr().cast::<ACL>(),
    };
    set_token_information(
        &restricted,
        TokenDefaultDacl,
        ptr::from_ref(&default_dacl).cast(),
        mem::size_of::<TOKEN_DEFAULT_DACL>() as u32,
    )?;
    Ok(restricted)
}

fn token_information(token: &OwnedHandle, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<u64>> {
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            ptr::null_mut(),
            0,
            &mut length,
        );
    }
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u64; (length as usize).div_ceil(mem::size_of::<u64>())];
    let queried = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    };
    if queried == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn set_token_information(
    token: &OwnedHandle,
    class: TOKEN_INFORMATION_CLASS,
    information: *const std::ffi::c_void,
    length: u32,
) -> io::Result<()> {
    if unsafe { SetTokenInformation(token.as_raw_handle(), class, information, length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn well_known_sid(kind: WELL_KNOWN_SID_TYPE) -> io::Result<SidBuffer> {
    let mut sid: SidBuffer = [0; 17];
    let mut length = mem::size_of::<SidBuffer>() as u32;
    let created =
        unsafe { CreateWellKnownSid(kind, ptr::null_mut(), sid.as_mut_ptr().cast(), &mut length) };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}

fn is_domain_administrative_group(sid: PSID) -> bool {
    unsafe {
        let authority = (*GetSidIdentifierAuthority(sid)).Value;
        let count = *GetSidSubAuthorityCount(sid);
        authority == [0, 0, 0, 0, 0, 5]
            && count == 5
            && *GetSidSubAuthority(sid, 0) == 21
            && DOMAIN_ADMINISTRATIVE_RIDS.contains(&*GetSidSubAuthority(sid, 4))
    }
}

fn privilege_luid(name: &str) -> io::Result<LUID> {
    let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    if unsafe { LookupPrivilegeValueW(ptr::null(), wide.as_ptr(), &mut luid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(luid)
}

fn default_dacl(grantees: &[PSID]) -> io::Result<Vec<u32>> {
    let ace_header = mem::size_of::<ACCESS_ALLOWED_ACE>() - mem::size_of::<u32>();
    let length = mem::size_of::<ACL>()
        + grantees
            .iter()
            .map(|sid| ace_header + unsafe { GetLengthSid(*sid) } as usize)
            .sum::<usize>();
    let mut acl = vec![0_u32; length.div_ceil(mem::size_of::<u32>())];
    let acl_ptr = acl.as_mut_ptr().cast::<ACL>();
    let acl_length = (acl.len() * mem::size_of::<u32>()) as u32;
    if unsafe { InitializeAcl(acl_ptr, acl_length, ACL_REVISION) } == 0 {
        return Err(io::Error::last_os_error());
    }
    for sid in grantees {
        if unsafe { AddAccessAllowedAce(acl_ptr, ACL_REVISION, GENERIC_ALL, *sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(acl)
}
