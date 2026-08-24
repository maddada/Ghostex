/*
 * Self-signed localhost certificate for the sandbox dev server.
 *
 * Why TLS at all: the tutorial-video window shows the real YouTube watch page
 * through the `/yt` + `/gv` reverse proxy (yt-proxy.ts). YouTube's media
 * protocol POSTs segment requests with a STREAMING request body, and Chrome
 * only sends those over HTTP/2+ — against a plain HTTP/1.1 dev server every
 * segment dies with `net::ERR_ALPN_NEGOTIATION_FAILED`. Vite serves HTTP/2
 * (`http2.createSecureServer`, HTTP/1.1 still allowed) as soon as
 * `server.https` is set and no `server.proxy` is configured, which is the case
 * here — the proxy is plain middleware.
 *
 * The key/cert are generated on demand into `dev-cert/` (gitignored, never
 * shipped, dev-server only). Browsers show the usual self-signed warning once.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const CERT_DIR_NAME = 'dev-cert';
const CERT_FILE = 'localhost-cert.pem';
const KEY_FILE = 'localhost-key.pem';

export interface SandboxDevCertificate {
  cert: Buffer;
  key: Buffer;
}

/**
 * Returns the sandbox dev certificate, generating it with `openssl` when it is
 * missing. Returns null when openssl is unavailable, so the caller can fall
 * back to plain HTTP instead of failing to boot.
 */
export function sandboxDevCertificate(sandboxRoot: string): SandboxDevCertificate | null {
  const certDir = path.join(sandboxRoot, CERT_DIR_NAME);
  const certPath = path.join(certDir, CERT_FILE);
  const keyPath = path.join(certDir, KEY_FILE);
  if (!fs.existsSync(certPath) || !fs.existsSync(keyPath)) {
    fs.mkdirSync(certDir, { recursive: true });
    const result = spawnSync(
      'openssl',
      [
        'req',
        '-x509',
        '-newkey',
        'rsa:2048',
        '-nodes',
        '-days',
        '825',
        '-subj',
        '/CN=localhost',
        '-addext',
        'subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1',
        '-keyout',
        keyPath,
        '-out',
        certPath,
      ],
      { encoding: 'utf8' }
    );
    if (result.status !== 0) {
      return null;
    }
  }
  try {
    return { cert: fs.readFileSync(certPath), key: fs.readFileSync(keyPath) };
  } catch {
    return null;
  }
}
