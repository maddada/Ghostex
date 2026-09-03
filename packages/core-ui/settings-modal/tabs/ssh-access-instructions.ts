import type { GxserverRemoteAccessPlatform } from '@/packages/shared/gxserver-protocol';

/*
 * CDXC:RemotePairing 2026-09-03:
 * Per-OS "turn on SSH access by hand" steps, mirrored from the mockup's
 * `SSH_INSTRUCTIONS` (docs/2026-09-03/mobile-setup/shared.js) so the Settings
 * cards and the Remote Setup modal read the same text. Product copy says
 * "SSH access"; the OS's own feature name appears only inside the path the
 * user has to find on that OS.
 */
export type SshAccessInstructions = {
  title: string;
  steps: readonly string[];
  note: string;
};

export const SSH_ACCESS_PLATFORMS: readonly GxserverRemoteAccessPlatform[] = ['macos', 'windows', 'linux'];

export const SSH_ACCESS_PLATFORM_LABELS: Record<GxserverRemoteAccessPlatform, string> = {
  linux: 'Linux',
  macos: 'macOS',
  windows: 'Windows',
};

export const SSH_ACCESS_INSTRUCTIONS: Record<GxserverRemoteAccessPlatform, SshAccessInstructions> = {
  macos: {
    title: 'Turn on SSH access on macOS',
    steps: [
      'Open System Settings → General → Sharing.',
      'Turn on Remote Login.',
      'Under Remote Login, make sure your user is allowed access.',
    ],
    note: 'Or let Ghostex do it: Turn on SSH access above. macOS asks for an admin password once.',
  },
  windows: {
    title: 'Turn on SSH access on Windows',
    steps: [
      'Open Settings → System → Optional features → Add a feature.',
      'Install OpenSSH Server.',
      'Open Services, start OpenSSH SSH Server, and set its startup type to Automatic.',
    ],
    note: 'Or let Ghostex do it: Turn on SSH access above. Windows asks for admin approval once.',
  },
  linux: {
    title: 'Turn on SSH access on Linux',
    steps: [
      'Install the OpenSSH server: sudo apt install openssh-server (Debian, Ubuntu) or sudo dnf install openssh-server (Fedora).',
      'Start it and keep it on: sudo systemctl enable --now ssh (or sshd on Fedora).',
    ],
    note: 'Or let Ghostex do it: Turn on SSH access above. It asks for your password once.',
  },
};

export function readSshAccessPlatform(value: unknown): GxserverRemoteAccessPlatform | undefined {
  return value === 'macos' || value === 'windows' || value === 'linux' ? value : undefined;
}
