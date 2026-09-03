import { useEffect, useState } from 'react';
import { toDataURL } from 'qrcode';
import { cn } from '@/packages/components/utils';

export type QrCodeProps = {
  alt: string;
  className?: string;
  size?: number;
  value: string;
};

/**
 * CDXC:RemoteSetup 2026-09-03:
 * One QR renderer for every surface that shows a scannable code (the Android
 * install popover, the Easy Connect pairing card, the Tailscale card). The
 * image is rendered off-thread by `qrcode` into a data URL; while it renders,
 * a same-sized placeholder keeps the layout from jumping.
 */
export function QrCode({ alt, className, size = 168, value }: QrCodeProps) {
  const [dataUrl, setDataUrl] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    setDataUrl(undefined);
    toDataURL(value, {
      color: { dark: '#111113', light: '#f4f4f5' },
      errorCorrectionLevel: 'M',
      margin: 2,
      width: size,
    })
      .then((url) => {
        if (!cancelled) {
          setDataUrl(url);
        }
      })
      .catch((error: unknown) => {
        console.error('[qr-code] Failed to render QR code.', error);
      });
    return () => {
      cancelled = true;
    };
  }, [size, value]);

  return (
    <span
      className={cn('gx-qr-code', className)}
      data-slot='qr-code'
      style={{ display: 'inline-block', height: size, width: size }}
    >
      {dataUrl ? (
        <img alt={alt} draggable={false} height={size} src={dataUrl} style={{ display: 'block' }} width={size} />
      ) : null}
    </span>
  );
}
