import { GHOSTEX_ANDROID_APK_URL, GHOSTEX_DISCORD_URL } from '@/packages/shared/sidebar-commands';

/*
 * CDXC:RemoteSetup 2026-09-03:
 * The install popover encodes the GitHub latest-release APK URL directly. The
 * mockup shows a `ghostex.dev/android` short link, but nothing in this repo or
 * the website config defines that redirect, so the QR and the visible link both
 * carry the URL that actually resolves today. Switch both to the short link
 * once the redirect exists on the website.
 */
export const REMOTE_SETUP_ANDROID_INSTALL_URL = GHOSTEX_ANDROID_APK_URL;
export const REMOTE_SETUP_ANDROID_INSTALL_URL_LABEL = GHOSTEX_ANDROID_APK_URL.replace(/^https?:\/\//u, '');
export const REMOTE_SETUP_DISCORD_URL = GHOSTEX_DISCORD_URL;
