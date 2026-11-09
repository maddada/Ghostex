import { describe, expect, it } from 'vitest';

import { windowsSigningDecision } from './windows-signing.mjs';

const CERTUM = ['CERTUM_SIMPLYSIGN_EMAIL', 'CERTUM_SIMPLYSIGN_TOTP'];
const policy = (value) => () => value;
const failPolicy = () => {
  throw new Error('the SignPath policy must not be read');
};

describe('windowsSigningDecision', () => {
  it('signs with Certum in auto and required when both Certum secrets exist, without reading the SignPath policy', () => {
    for (const mode of ['auto', 'required']) {
      const decision = windowsSigningDecision(mode, new Set([...CERTUM, 'SIGNPATH_API_TOKEN']), failPolicy);
      expect(decision).toMatchObject({ signed: true, signer: 'certum' });
    }
  });

  it('never signs when off, even with every secret', () => {
    const decision = windowsSigningDecision('off', new Set([...CERTUM, 'SIGNPATH_API_TOKEN']), failPolicy);
    expect(decision).toMatchObject({ signed: false, signer: 'none' });
  });

  it('ignores a half-configured Certum and keeps the SignPath rules', () => {
    const secrets = new Set(['CERTUM_SIMPLYSIGN_EMAIL', 'SIGNPATH_API_TOKEN']);
    expect(windowsSigningDecision('auto', secrets, policy('test-signing'))).toMatchObject({ signed: false });
    expect(windowsSigningDecision('auto', secrets, policy('release-signing'))).toMatchObject({
      signed: true,
      signer: 'signpath',
    });
    expect(windowsSigningDecision('required', secrets, policy('test-signing'))).toMatchObject({
      signed: true,
      signer: 'signpath',
    });
  });

  it('stays unsigned in auto and fails in required when no signer is configured', () => {
    expect(windowsSigningDecision('auto', new Set(), failPolicy)).toMatchObject({ signed: false, signer: 'none' });
    expect(() => windowsSigningDecision('required', new Set(['CERTUM_SIMPLYSIGN_TOTP']), failPolicy)).toThrow(
      /CERTUM_SIMPLYSIGN_EMAIL and CERTUM_SIMPLYSIGN_TOTP/
    );
  });
});
