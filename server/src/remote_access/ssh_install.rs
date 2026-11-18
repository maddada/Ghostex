/*
CDXC:RemotePairing 2026-11-18 DECISION:
Users on Linux opened Connect a computer and hit a dead end: "The OpenSSH server is not installed.
Install it with your package manager (for example …)". The user picked both ways out (Q3975, "give
ppl both options (copy and install)"): the Turn on SSH step shows the one install command for this
distribution with a Copy button, and an Install button that installs the server behind the desktop's
admin password prompt (pkexec), then starts it. Nothing installs without that click. The package
manager is found by its program on this computer (apt-get, dnf, pacman, zypper, in that order); with
none of them the window falls back to a short list of commands and offers no Install.
*/
/// A Linux package manager Ghostex knows how to install the OpenSSH server with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxPackageManager {
    Apt,
    Dnf,
    Pacman,
    Zypper,
}

/// The install command for a computer whose package manager Ghostex doesn't know.
pub const FALLBACK_INSTALL_HINT: &str = "`sudo apt install openssh-server`, `sudo dnf install openssh-server`, or `sudo pacman -S openssh`";

/// Where package managers live; gxserver may run with a short PATH (a systemd user unit).
const PROGRAM_DIRS: &[&str] = &["/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/local/bin"];

impl LinuxPackageManager {
    /// The first known package manager whose program `exists` says is here.
    pub fn detect_with(exists: impl Fn(&str) -> bool) -> Option<Self> {
        [
            ("apt-get", Self::Apt),
            ("dnf", Self::Dnf),
            ("pacman", Self::Pacman),
            ("zypper", Self::Zypper),
        ]
        .into_iter()
        .find(|(program, _)| {
            PROGRAM_DIRS
                .iter()
                .any(|dir| exists(&format!("{dir}/{program}")))
        })
        .map(|(_, manager)| manager)
    }

    /// The package manager on this computer, if Ghostex knows it.
    pub fn detect() -> Option<Self> {
        Self::detect_with(|path| std::path::Path::new(path).is_file())
    }

    /// The command a person types to install the OpenSSH server.
    pub fn user_command(self) -> &'static str {
        match self {
            Self::Apt => "sudo apt install openssh-server",
            Self::Dnf => "sudo dnf install openssh-server",
            Self::Pacman => "sudo pacman -S openssh",
            Self::Zypper => "sudo zypper install openssh",
        }
    }

    /// The same install, run as root without questions (inside the admin prompt). apt retries
    /// once after refreshing its package lists, which a fresh install may not have yet.
    pub fn root_install_script(self) -> &'static str {
        match self {
            Self::Apt => {
                "export DEBIAN_FRONTEND=noninteractive; apt-get install -y openssh-server || { apt-get update && apt-get install -y openssh-server; }"
            }
            Self::Dnf => "dnf install -y openssh-server",
            Self::Pacman => "pacman -S --noconfirm --needed openssh",
            Self::Zypper => "zypper --non-interactive install openssh",
        }
    }

    /// The systemd unit the installed server runs as: `ssh` on Debian and Ubuntu, `sshd`
    /// elsewhere.
    pub fn ssh_unit(self) -> &'static str {
        match self {
            Self::Apt => "ssh",
            _ => "sshd",
        }
    }

    /// Install, then start the server the way `ssh_enable.rs` starts an installed one (the socket
    /// when the distribution enabled it, else the service, kept across reboots): one admin prompt.
    pub fn root_install_and_start_script(self) -> String {
        let unit = self.ssh_unit();
        format!(
            "{{ {install}; }} && if systemctl is-enabled --quiet {unit}.socket 2>/dev/null; then systemctl start {unit}.socket; else systemctl enable --now {unit}.service; fi",
            install = self.root_install_script(),
        )
    }
}

/// The install command to show for this computer: its package manager's, or the short list.
pub fn install_command_hint(manager: Option<LinuxPackageManager>) -> String {
    match manager {
        Some(manager) => format!("`{}`", manager.user_command()),
        None => FALLBACK_INSTALL_HINT.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(present: &[&str]) -> Option<LinuxPackageManager> {
        LinuxPackageManager::detect_with(|path| present.contains(&path))
    }

    #[test]
    fn detects_each_package_manager_by_its_program() {
        assert_eq!(
            detect(&["/usr/bin/apt-get"]),
            Some(LinuxPackageManager::Apt)
        );
        assert_eq!(detect(&["/usr/bin/dnf"]), Some(LinuxPackageManager::Dnf));
        assert_eq!(
            detect(&["/usr/bin/pacman"]),
            Some(LinuxPackageManager::Pacman)
        );
        assert_eq!(
            detect(&["/usr/bin/zypper"]),
            Some(LinuxPackageManager::Zypper)
        );
        assert_eq!(detect(&["/bin/apt-get"]), Some(LinuxPackageManager::Apt));
        assert_eq!(detect(&[]), None);
        assert_eq!(detect(&["/usr/bin/apk"]), None);
    }

    #[test]
    fn apt_wins_when_several_are_present() {
        assert_eq!(
            detect(&["/usr/bin/dnf", "/usr/bin/apt-get"]),
            Some(LinuxPackageManager::Apt)
        );
    }

    #[test]
    fn hint_names_one_command_or_the_short_list() {
        assert_eq!(
            install_command_hint(Some(LinuxPackageManager::Pacman)),
            "`sudo pacman -S openssh`"
        );
        assert_eq!(install_command_hint(None), FALLBACK_INSTALL_HINT);
    }

    #[test]
    fn install_script_starts_the_distribution_unit() {
        let script = LinuxPackageManager::Apt.root_install_and_start_script();
        assert!(script.contains("apt-get install -y openssh-server"));
        assert!(script.contains("systemctl enable --now ssh.service"));
        let script = LinuxPackageManager::Dnf.root_install_and_start_script();
        assert!(script.contains("systemctl start sshd.socket"));
    }
}
