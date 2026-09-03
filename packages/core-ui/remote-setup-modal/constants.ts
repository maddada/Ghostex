import { GHOSTEX_ANDROID_APK_URL, GHOSTEX_DISCORD_URL } from '@/packages/shared/sidebar-commands';

/*
 * CDXC:RemoteSetup 2026-09-03:
 * The install popover shows and QR-encodes the short website link, which the
 * website redirects to the latest-release APK on GitHub
 * (`GHOSTEX_ANDROID_APK_URL`, kept here as the redirect's target for reference).
 */
export const GHOSTEX_ANDROID_INSTALL_URL = 'https://ghostex.dev/android';
export const GHOSTEX_ANDROID_INSTALL_URL_LABEL = GHOSTEX_ANDROID_INSTALL_URL.replace(/^https?:\/\//u, '');
export const GHOSTEX_ANDROID_INSTALL_REDIRECT_TARGET = GHOSTEX_ANDROID_APK_URL;
export const REMOTE_SETUP_DISCORD_URL = GHOSTEX_DISCORD_URL;
