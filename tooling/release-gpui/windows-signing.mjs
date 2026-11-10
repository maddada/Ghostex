/*
 * CDXC:Release 2026-11-09 DECISION (see release-gpui-windows.yml and docs/code-signing.md):
 * which signer the dispatcher's --windows-signing <auto|required|off> turns on.
 *   - Certum (the user's own trusted "Open Source Code Signing in the Cloud" certificate)
 *     signs whenever both of its secrets exist, in auto and required alike; the Windows
 *     jobs prefer it over SignPath.
 *   - Without Certum, auto signs with SignPath only when its policy is release-signing
 *     (the 2026-10-19 "Turn signing off for now" decision), and required signs with
 *     whatever SignPath policy is configured, test-signing included.
 *   - required fails when neither signer is configured; off never signs.
 * `secrets` is the set of repository secret names; `signPathPolicy` is called only when
 * the SignPath policy decides the outcome (it costs a GitHub API call).
 */
export const CERTUM_SIGNING_SECRETS = ['CERTUM_SIMPLYSIGN_EMAIL', 'CERTUM_SIMPLYSIGN_TOTP'];

export function windowsSigningDecision(mode, secrets, signPathPolicy) {
  const certum = CERTUM_SIGNING_SECRETS.every((name) => secrets.has(name));
  const signPath = secrets.has('SIGNPATH_API_TOKEN');
  if (mode === 'off') return { signed: false, signer: 'none', reason: '--windows-signing off' };
  if (certum) return { signed: true, signer: 'certum', reason: 'the Certum SimplySign secrets are configured' };
  if (mode === 'required') {
    if (!signPath) {
      throw new Error(
        `Windows signing requires repository secrets: ${CERTUM_SIGNING_SECRETS.join(' and ')} (Certum), or SIGNPATH_API_TOKEN (SignPath)`
      );
    }
    return {
      signed: true,
      signer: 'signpath',
      reason: `--windows-signing required (SignPath policy ${signPathPolicy()})`,
    };
  }
  if (!signPath) {
    return { signed: false, signer: 'none', reason: 'no Certum or SignPath secrets are configured' };
  }
  const policy = signPathPolicy();
  if (policy !== 'release-signing') {
    return {
      signed: false,
      signer: 'none',
      reason:
        `SignPath policy is ${policy}, and auto signs with SignPath only with release-signing ` +
        '(set the CERTUM_SIMPLYSIGN_* secrets, or gh variable set SIGNPATH_SIGNING_POLICY_SLUG --body release-signing; --windows-signing required test-signs one dispatch)',
    };
  }
  return { signed: true, signer: 'signpath', reason: 'SignPath policy is release-signing' };
}
