#!/usr/bin/env node
/*
 * Prepare and submit an App Store release of the iOS app through the App Store Connect API, from the
 * listing in tooling/release-mobile/app-store/ (app-store.json, description.<locale>.txt,
 * review-notes.txt, screenshots/<set>/NN-*.png).
 *
 *   asc-app-store.mjs validate                                          offline: the listing files and screenshots
 *   asc-app-store.mjs check   [--version 10.27.0]                       read-only: what is set, what is missing
 *   asc-app-store.mjs prepare --version 10.27.0 [--timeout-minutes 60] [--dry-run]
 *   asc-app-store.mjs submit  --version 10.27.0 [--dry-run]
 *
 * `prepare` creates (or renames) the editable App Store version, writes the app information (name,
 * subtitle, privacy policy URL, categories, age rating, content rights), the version's text and
 * screenshots, attaches the build once Apple has processed it, and fills App Review Information.
 * `submit` checks the version is complete and sends it to App Review. Nothing is released: the
 * version's release type comes from app-store.json (MANUAL means you press Release).
 *
 * Env: the key env in asc-api.mjs; GHOSTEX_WHATS_NEW (required from the second released version on);
 * GHOSTEX_REVIEW_CONTACT_FIRST_NAME, _LAST_NAME, _PHONE, _EMAIL (optional; App Review Information is
 * written only when all four are set). GHOSTEX_REVIEW_DEMO_USERNAME and _PASSWORD are optional; when
 * either is unset, demoAccountRequired is false and no credentials are sent. GHOSTEX_REVIEW_DEMO_ADDRESS
 * and _SSH_PORT are accepted and ignored. The notes file has no placeholders.
 *
 * CDXC:Release 2026-10-19 WHY:
 * maddada/Ghostex is public, so its workflow logs are too: this script logs states, counts and
 * whether a field is set, never the review contact, demo credentials or the review notes.
 * App Privacy, Pricing and Availability and the EU trader (DSA) status have no App Store Connect API,
 * so they stay one-time steps in App Store Connect (tooling/release-mobile/app-store/README.md).
 * Apple refuses "What's New" on an app's first version, so it is only sent once a version is live.
 */

import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { ascClientFromEnv } from './asc-api.mjs';
import { buildNumberFor } from './asc-testflight-distribute.mjs';

const BUNDLE_ID = 'com.maddada.ghostex.ios';
const PLATFORM = 'IOS';
const STORE_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), 'app-store');
const POLL_SECONDS = 30;
/** Version states App Store Connect lets us edit. */
const EDITABLE_VERSION_STATES = new Set([
  'PREPARE_FOR_SUBMISSION',
  'DEVELOPER_REJECTED',
  'REJECTED',
  'METADATA_REJECTED',
  'INVALID_BINARY',
]);
/** A version in one of these states has been on the App Store. */
const LIVE_VERSION_STATES = new Set(['READY_FOR_DISTRIBUTION', 'REPLACED_WITH_NEW_VERSION']);
const EDITABLE_APP_INFO_STATES = new Set(['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED']);
const SUBMITTED_STATES = new Set(['WAITING_FOR_REVIEW', 'IN_REVIEW']);
/** Accepted portrait sizes per screenshot set (landscape is the same sizes turned). */
const SCREENSHOT_SIZES = {
  APP_IPHONE_67: [
    [1320, 2868],
    [1290, 2796],
    [1260, 2736],
  ],
  APP_IPAD_PRO_3GEN_129: [
    [2064, 2752],
    [2048, 2732],
  ],
};
const LIMITS = {
  name: 30,
  subtitle: 30,
  promotionalText: 170,
  description: 4000,
  whatsNew: 4000,
  notes: 4000,
};

const log = (message) => console.log(message);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const isSet = (value) => (value ? 'set' : 'missing');

// ---------- the listing on disk ----------

function readPngInfo(file) {
  const bytes = readFileSync(file);
  if (bytes.toString('latin1', 1, 4) !== 'PNG') throw new Error(`${file} is not a PNG`);
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  const colorType = bytes[25];
  // Colour types 4 and 6 carry an alpha channel; a tRNS chunk adds transparency to the others.
  let alpha = colorType === 4 || colorType === 6;
  for (let offset = 8; offset + 8 <= bytes.length && !alpha;) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('latin1', offset + 4, offset + 8);
    if (type === 'tRNS') alpha = true;
    if (type === 'IEND') break;
    offset += 12 + length;
  }
  return {
    bytes,
    width,
    height,
    alpha,
    md5: createHash('md5').update(bytes).digest('hex'),
  };
}

function loadScreenshots(displayType, relativeDir) {
  const dir = path.join(STORE_DIR, relativeDir);
  let names;
  try {
    names = readdirSync(dir)
      .filter((name) => /^\d{2}-.+\.png$/.test(name))
      .sort();
  } catch {
    names = [];
  }
  if (names.length > 10)
    throw new Error(`${relativeDir} has ${names.length} screenshots; App Store Connect takes at most 10`);
  const sizes = SCREENSHOT_SIZES[displayType];
  if (!sizes) throw new Error(`unknown screenshot set ${displayType}`);
  return names.map((fileName) => {
    const info = readPngInfo(path.join(dir, fileName));
    const fits = sizes.some(
      ([w, h]) => (info.width === w && info.height === h) || (info.width === h && info.height === w)
    );
    if (!fits)
      throw new Error(
        `${relativeDir}/${fileName} is ${info.width}x${info.height}; ${displayType} takes ${sizes.map(([w, h]) => `${w}x${h}`).join(', ')}`
      );
    if (info.alpha)
      throw new Error(`${relativeDir}/${fileName} has an alpha channel or transparency; App Store Connect refuses it`);
    return { fileName, ...info };
  });
}

function loadListing() {
  const config = JSON.parse(readFileSync(path.join(STORE_DIR, 'app-store.json'), 'utf8'));
  const description = readFileSync(path.join(STORE_DIR, `description.${config.locale}.txt`), 'utf8').trim();
  const notesTemplate = readFileSync(path.join(STORE_DIR, 'review-notes.txt'), 'utf8').trim();
  const listing = { ...config, description, notesTemplate };
  for (const [field, max] of Object.entries(LIMITS)) {
    if (typeof listing[field] === 'string' && listing[field].length > max)
      throw new Error(`${field} is ${listing[field].length} characters; App Store Connect allows ${max}`);
  }
  if (Buffer.byteLength(listing.keywords) > 100)
    throw new Error(`keywords are ${Buffer.byteLength(listing.keywords)} bytes; App Store Connect allows 100`);
  listing.screenshotSets = Object.entries(config.screenshots ?? {}).map(([displayType, dir]) => ({
    displayType,
    dir,
    files: loadScreenshots(displayType, dir),
  }));
  return listing;
}

/*
 * CDXC:Release 2026-11-08 DECISION:
 * User: App Review uses the in-app "Try a demo computer" demo, not an SSH demo server.
 * GHOSTEX_REVIEW_DEMO_* (APPLE_REVIEW_DEMO_* in the workflow) are optional.
 * When the username or password is unset, the review detail sets demoAccountRequired to false and sends no demo credentials, and the notes have no SSH placeholders.
 * Review contact stays optional: write App Review Information only when the contact env is set.
 * Supersedes the demo-computer sign-in of 2026-10-19.
 */
function reviewDetailFromEnv(notesTemplate) {
  const env = process.env;
  const contact = {
    contactFirstName: env.GHOSTEX_REVIEW_CONTACT_FIRST_NAME?.trim(),
    contactLastName: env.GHOSTEX_REVIEW_CONTACT_LAST_NAME?.trim(),
    contactPhone: env.GHOSTEX_REVIEW_CONTACT_PHONE?.trim(),
    contactEmail: env.GHOSTEX_REVIEW_CONTACT_EMAIL?.trim(),
  };
  const notes = notesTemplate.trim();
  if (notes.length > LIMITS.notes)
    throw new Error(`review notes are ${notes.length} characters; App Store Connect allows ${LIMITS.notes}`);
  if (/\{\{[A-Z_]+\}\}/.test(notes)) throw new Error('review-notes.txt has a placeholder this script does not fill');
  const username = env.GHOSTEX_REVIEW_DEMO_USERNAME?.trim() ?? '';
  const password = env.GHOSTEX_REVIEW_DEMO_PASSWORD ?? '';
  const demo =
    username && password.trim()
      ? {
          demoAccountRequired: true,
          demoAccountName: username,
          demoAccountPassword: password,
          notes,
        }
      : { demoAccountRequired: false, notes };
  return {
    contact: Object.values(contact).every(Boolean) ? contact : null,
    demo,
  };
}

// ---------- App Store Connect reads ----------

async function requestOrNull(client, method, url) {
  try {
    return await client.request(method, url);
  } catch (error) {
    if (/ answered 404:/.test(error.message)) return null;
    throw error;
  }
}

async function findApp(client) {
  const { data } = await client.request(
    'GET',
    `/v1/apps?filter[bundleId]=${encodeURIComponent(BUNDLE_ID)}&fields[apps]=name,bundleId,contentRightsDeclaration&limit=10`
  );
  const app = data.find((candidate) => candidate.attributes?.bundleId === BUNDLE_ID);
  if (!app) throw new Error(`no App Store Connect app with bundle ID ${BUNDLE_ID}`);
  log(`App: ${app.attributes.name} (${app.id}, ${BUNDLE_ID})`);
  return app;
}

async function listVersions(client, appId) {
  const { data } = await client.list(
    `/v1/apps/${appId}/appStoreVersions?filter[platform]=${PLATFORM}&fields[appStoreVersions]=versionString,appVersionState,releaseType,copyright,createdDate&limit=200`
  );
  return data;
}

async function editableAppInfo(client, appId) {
  const { data } = await client.list(`/v1/apps/${appId}/appInfos?include=primaryCategory,secondaryCategory&limit=10`);
  const editable = data.find((info) => EDITABLE_APP_INFO_STATES.has(info.attributes?.state));
  return { editable: editable ?? null, all: data };
}

async function findLocalization(client, url, locale) {
  const { data } = await client.list(url);
  return data.find((item) => item.attributes?.locale === locale) ?? null;
}

async function findBuild(client, appId, version, buildNumber) {
  const { data, included } = await client.list(
    `/v1/builds?filter[app]=${appId}&filter[version]=${encodeURIComponent(buildNumber)}&include=preReleaseVersion&limit=50`
  );
  const matches = data.filter((build) => {
    const id = build.relationships?.preReleaseVersion?.data?.id;
    return (
      included.find((resource) => resource.type === 'preReleaseVersions' && resource.id === id)?.attributes?.version ===
      version
    );
  });
  if (matches.length > 1) throw new Error(`${matches.length} builds match ${version} (${buildNumber})`);
  return matches[0] ?? null;
}

async function waitForValidBuild(client, appId, version, buildNumber, timeoutMinutes) {
  const deadline = Date.now() + timeoutMinutes * 60_000;
  let lastState = null;
  for (;;) {
    const build = await findBuild(client, appId, version, buildNumber);
    const state = build ? build.attributes?.processingState : 'NOT_LISTED';
    if (state !== lastState) {
      log(`Build ${version} (${buildNumber}) processing state: ${state}`);
      lastState = state;
    }
    if (state === 'VALID') return build;
    if (state === 'FAILED' || state === 'INVALID')
      throw new Error(`build ${version} (${buildNumber}) processing ended ${state}`);
    if (Date.now() >= deadline)
      throw new Error(
        `build ${version} (${buildNumber}) was not VALID after ${timeoutMinutes} minutes (last state ${state}); upload it with the TestFlight workflow first`
      );
    await sleep(POLL_SECONDS * 1000);
  }
}

function describeEncryption(value) {
  if (value === false) return 'answered: no non-exempt encryption';
  if (value === true) return 'answered: uses non-exempt encryption';
  return 'not answered';
}

// ---------- prepare ----------

/** The App Store version to prepare: this version string, or the editable version renamed to it. */
async function ensureVersion(client, app, versionString, listing, dryRun) {
  const versions = await listVersions(client, app.id);
  let version = versions.find((candidate) => candidate.attributes?.versionString === versionString);
  if (version) {
    const state = version.attributes.appVersionState;
    if (!EDITABLE_VERSION_STATES.has(state))
      throw new Error(
        `App Store version ${versionString} is ${state}; only a version that is not submitted or released can be prepared`
      );
    log(`Version ${versionString}: exists (${state})`);
  } else {
    const editable = versions.find((candidate) => EDITABLE_VERSION_STATES.has(candidate.attributes?.appVersionState));
    if (editable) {
      log(
        `Version ${versionString}: ${dryRun ? 'would rename' : 'renaming'} the editable version ${editable.attributes.versionString} (${editable.attributes.appVersionState})`
      );
      if (!dryRun) {
        const { data } = await client.request('PATCH', `/v1/appStoreVersions/${editable.id}`, {
          data: {
            type: 'appStoreVersions',
            id: editable.id,
            attributes: { versionString },
          },
        });
        version = data;
      } else version = editable;
    } else if (dryRun) {
      log(`Version ${versionString}: would be created`);
      return { version: null, versions };
    } else {
      const { data } = await client.request('POST', '/v1/appStoreVersions', {
        data: {
          type: 'appStoreVersions',
          attributes: {
            platform: PLATFORM,
            versionString,
            releaseType: listing.releaseType,
            copyright: listing.copyright,
          },
          relationships: { app: { data: { type: 'apps', id: app.id } } },
        },
      });
      version = data;
      log(`Version ${versionString}: created`);
    }
  }
  const changes = {};
  if (version.attributes?.releaseType !== listing.releaseType) changes.releaseType = listing.releaseType;
  if (version.attributes?.copyright !== listing.copyright) changes.copyright = listing.copyright;
  if (Object.keys(changes).length) {
    log(`Version ${versionString}: ${dryRun ? 'would set' : 'setting'} ${Object.keys(changes).join(', ')}`);
    if (!dryRun)
      await client.request('PATCH', `/v1/appStoreVersions/${version.id}`, {
        data: { type: 'appStoreVersions', id: version.id, attributes: changes },
      });
  }
  return { version, versions };
}

function changedFields(current, desired) {
  return Object.fromEntries(
    Object.entries(desired).filter(([key, value]) => (current?.[key] ?? null) !== (value ?? null))
  );
}

async function prepareAppInfo(client, app, listing, dryRun) {
  const verb = dryRun ? 'would update' : 'updated';
  const { editable } = await editableAppInfo(client, app.id);
  if (!editable) {
    log(
      'App information: not editable right now (it opens again with the next new version); name, subtitle, categories and age rating left as they are'
    );
  } else {
    const desired = {
      name: listing.name,
      subtitle: listing.subtitle,
      privacyPolicyUrl: listing.privacyPolicyUrl,
    };
    const localization = await findLocalization(
      client,
      `/v1/appInfos/${editable.id}/appInfoLocalizations?limit=50`,
      listing.locale
    );
    const changes = changedFields(localization?.attributes, desired);
    if (!Object.keys(changes).length) log(`App information (${listing.locale}): up to date`);
    else {
      log(`App information (${listing.locale}): ${verb} ${Object.keys(changes).join(', ')}`);
      if (!dryRun && localization)
        await client.request('PATCH', `/v1/appInfoLocalizations/${localization.id}`, {
          data: {
            type: 'appInfoLocalizations',
            id: localization.id,
            attributes: changes,
          },
        });
      else if (!dryRun)
        await client.request('POST', '/v1/appInfoLocalizations', {
          data: {
            type: 'appInfoLocalizations',
            attributes: { locale: listing.locale, ...desired },
            relationships: {
              appInfo: { data: { type: 'appInfos', id: editable.id } },
            },
          },
        });
    }

    const primary = editable.relationships?.primaryCategory?.data?.id ?? null;
    const secondary = editable.relationships?.secondaryCategory?.data?.id ?? null;
    if (primary === listing.primaryCategory && secondary === listing.secondaryCategory)
      log(`Categories: up to date (${primary}, ${secondary})`);
    else {
      log(`Categories: ${verb} to ${listing.primaryCategory}, ${listing.secondaryCategory}`);
      if (!dryRun)
        await client.request('PATCH', `/v1/appInfos/${editable.id}`, {
          data: {
            type: 'appInfos',
            id: editable.id,
            relationships: {
              primaryCategory: {
                data: { type: 'appCategories', id: listing.primaryCategory },
              },
              secondaryCategory: {
                data: { type: 'appCategories', id: listing.secondaryCategory },
              },
            },
          },
        });
    }

    const declaration = await requestOrNull(client, 'GET', `/v1/appInfos/${editable.id}/ageRatingDeclaration`);
    if (!declaration?.data) throw new Error('the app has no age rating declaration to update');
    const ratingChanges = changedFields(declaration.data.attributes, listing.ageRating);
    if (!Object.keys(ratingChanges).length) log('Age rating answers: up to date');
    else {
      log(`Age rating answers: ${verb} ${Object.keys(ratingChanges).length} answer(s)`);
      if (!dryRun)
        await client.request('PATCH', `/v1/ageRatingDeclarations/${declaration.data.id}`, {
          data: {
            type: 'ageRatingDeclarations',
            id: declaration.data.id,
            attributes: ratingChanges,
          },
        });
    }
  }

  if (app.attributes?.contentRightsDeclaration === listing.contentRightsDeclaration) log('Content rights: up to date');
  else {
    log(`Content rights: ${verb} to ${listing.contentRightsDeclaration}`);
    if (!dryRun)
      await client.request('PATCH', `/v1/apps/${app.id}`, {
        data: {
          type: 'apps',
          id: app.id,
          attributes: {
            contentRightsDeclaration: listing.contentRightsDeclaration,
          },
        },
      });
  }
}

async function prepareVersionText(client, version, versions, listing, dryRun) {
  const verb = dryRun ? 'would update' : 'updated';
  const hasLiveVersion = versions.some((candidate) => LIVE_VERSION_STATES.has(candidate.attributes?.appVersionState));
  const desired = {
    description: listing.description,
    keywords: listing.keywords,
    marketingUrl: listing.marketingUrl,
    promotionalText: listing.promotionalText,
    supportUrl: listing.supportUrl,
  };
  const whatsNew = (process.env.GHOSTEX_WHATS_NEW ?? '').trim();
  if (hasLiveVersion) {
    if (!whatsNew) throw new Error("GHOSTEX_WHATS_NEW is empty; every version after the first needs What's New text");
    if (whatsNew.length > LIMITS.whatsNew)
      throw new Error(`What's New is ${whatsNew.length} characters; App Store Connect allows ${LIMITS.whatsNew}`);
    desired.whatsNew = whatsNew;
  } else if (whatsNew) {
    log("What's New: not sent, Apple only takes it once a version has been live");
  }
  const localization = await findLocalization(
    client,
    `/v1/appStoreVersions/${version.id}/appStoreVersionLocalizations?limit=50`,
    listing.locale
  );
  const changes = changedFields(localization?.attributes, desired);
  if (!Object.keys(changes).length) log(`Version text (${listing.locale}): up to date`);
  else {
    log(`Version text (${listing.locale}): ${verb} ${Object.keys(changes).join(', ')}`);
    if (!dryRun && localization)
      await client.request('PATCH', `/v1/appStoreVersionLocalizations/${localization.id}`, {
        data: {
          type: 'appStoreVersionLocalizations',
          id: localization.id,
          attributes: changes,
        },
      });
    else if (!dryRun) {
      const { data } = await client.request('POST', '/v1/appStoreVersionLocalizations', {
        data: {
          type: 'appStoreVersionLocalizations',
          attributes: { locale: listing.locale, ...desired },
          relationships: {
            appStoreVersion: {
              data: { type: 'appStoreVersions', id: version.id },
            },
          },
        },
      });
      return data;
    }
  }
  return localization;
}

async function screenshotSet(client, localizationId, displayType) {
  const { data } = await client.list(
    `/v1/appStoreVersionLocalizations/${localizationId}/appScreenshotSets?filter[screenshotDisplayType]=${displayType}&limit=10`
  );
  return data[0] ?? null;
}

async function remoteScreenshots(client, setId) {
  const { data } = await client.list(
    `/v1/appScreenshotSets/${setId}/appScreenshots?fields[appScreenshots]=fileName,fileSize,sourceFileChecksum,assetDeliveryState&limit=50`
  );
  return data;
}

async function uploadScreenshot(client, setId, file) {
  const { data } = await client.request('POST', '/v1/appScreenshots', {
    data: {
      type: 'appScreenshots',
      attributes: { fileName: file.fileName, fileSize: file.bytes.length },
      relationships: {
        appScreenshotSet: { data: { type: 'appScreenshotSets', id: setId } },
      },
    },
  });
  for (const operation of data.attributes?.uploadOperations ?? []) {
    const headers = Object.fromEntries((operation.requestHeaders ?? []).map(({ name, value }) => [name, value]));
    const response = await fetch(operation.url, {
      method: operation.method,
      headers,
      body: file.bytes.subarray(operation.offset, operation.offset + operation.length),
    });
    if (!response.ok)
      throw new Error(
        `uploading ${file.fileName} (bytes ${operation.offset}+${operation.length}) answered ${response.status}`
      );
  }
  await client.request('PATCH', `/v1/appScreenshots/${data.id}`, {
    data: {
      type: 'appScreenshots',
      id: data.id,
      attributes: { uploaded: true, sourceFileChecksum: file.md5 },
    },
  });
  return data.id;
}

async function waitForScreenshots(client, ids) {
  const deadline = Date.now() + 10 * 60_000;
  const pending = new Set(ids);
  while (pending.size) {
    for (const id of [...pending]) {
      const { data } = await client.request(
        'GET',
        `/v1/appScreenshots/${id}?fields[appScreenshots]=fileName,assetDeliveryState`
      );
      const state = data.attributes?.assetDeliveryState;
      if (state?.state === 'COMPLETE') pending.delete(id);
      else if (state?.state === 'FAILED')
        throw new Error(
          `App Store Connect could not process ${data.attributes?.fileName}: ${JSON.stringify(state.errors ?? [])}`
        );
    }
    if (!pending.size) break;
    if (Date.now() >= deadline) throw new Error(`${pending.size} screenshot(s) were still processing after 10 minutes`);
    await sleep(5000);
  }
}

async function prepareScreenshots(client, localizationId, listing, dryRun) {
  for (const set of listing.screenshotSets) {
    if (!set.files.length) {
      log(`Screenshots ${set.displayType}: no files in ${set.dir}; App Store Connect left as it is`);
      continue;
    }
    let existing = await screenshotSet(client, localizationId, set.displayType);
    const remote = existing ? await remoteScreenshots(client, existing.id) : [];
    const same =
      remote.length === set.files.length &&
      remote.every(
        (shot, index) =>
          shot.attributes?.fileName === set.files[index].fileName &&
          shot.attributes?.sourceFileChecksum === set.files[index].md5 &&
          shot.attributes?.assetDeliveryState?.state === 'COMPLETE'
      );
    if (same) {
      log(`Screenshots ${set.displayType}: up to date (${remote.length})`);
      continue;
    }
    if (dryRun) {
      log(`Screenshots ${set.displayType}: would replace ${remote.length} with ${set.files.length} from ${set.dir}`);
      continue;
    }
    if (!existing) {
      const { data } = await client.request('POST', '/v1/appScreenshotSets', {
        data: {
          type: 'appScreenshotSets',
          attributes: { screenshotDisplayType: set.displayType },
          relationships: {
            appStoreVersionLocalization: {
              data: { type: 'appStoreVersionLocalizations', id: localizationId },
            },
          },
        },
      });
      existing = data;
    }
    for (const shot of remote) await client.request('DELETE', `/v1/appScreenshots/${shot.id}`);
    const ids = [];
    for (const file of set.files) ids.push(await uploadScreenshot(client, existing.id, file));
    await waitForScreenshots(client, ids);
    await client.request('PATCH', `/v1/appScreenshotSets/${existing.id}/relationships/appScreenshots`, {
      data: ids.map((id) => ({ type: 'appScreenshots', id })),
    });
    log(`Screenshots ${set.displayType}: replaced ${remote.length} with ${ids.length} from ${set.dir}`);
  }
}

async function prepareBuild(client, app, version, versionString, buildNumber, timeoutMinutes, dryRun) {
  const build = dryRun
    ? await findBuild(client, app.id, versionString, buildNumber)
    : await waitForValidBuild(client, app.id, versionString, buildNumber, timeoutMinutes);
  if (!build) {
    log(`Build ${versionString} (${buildNumber}): not uploaded yet; the real run waits for it`);
    return;
  }
  const attributes = build.attributes ?? {};
  log(
    `Build ${versionString} (${buildNumber}): ${attributes.processingState}, expired ${attributes.expired ?? '?'}, export compliance ${describeEncryption(attributes.usesNonExemptEncryption)}`
  );
  if (attributes.expired) throw new Error(`build ${versionString} (${buildNumber}) has expired; upload a new one`);
  const current = await requestOrNull(client, 'GET', `/v1/appStoreVersions/${version.id}/relationships/build`);
  if (current?.data?.id === build.id) log('Build: already attached to the version');
  else {
    log(`Build: ${dryRun ? 'would attach' : 'attached'} to the version`);
    if (!dryRun)
      await client.request('PATCH', `/v1/appStoreVersions/${version.id}/relationships/build`, {
        data: { type: 'builds', id: build.id },
      });
  }
}

async function prepareReviewDetail(client, version, listing, dryRun) {
  const { contact, demo } = reviewDetailFromEnv(listing.notesTemplate);
  if (!contact) {
    log('App Review contact: GHOSTEX_REVIEW_CONTACT_* not all set; left as it is in App Store Connect');
    return;
  }
  const signIn = demo.demoAccountRequired ? 'sign-in required' : 'no sign-in';
  log(
    `App Review Information: ${dryRun ? 'would write' : 'wrote'} contact, ${signIn}, notes (${demo.notes.length} characters)`
  );
  if (dryRun) return;
  const attributes = { ...contact, ...demo };
  const existing = await requestOrNull(client, 'GET', `/v1/appStoreVersions/${version.id}/appStoreReviewDetail`);
  if (existing?.data)
    await client.request('PATCH', `/v1/appStoreReviewDetails/${existing.data.id}`, {
      data: {
        type: 'appStoreReviewDetails',
        id: existing.data.id,
        attributes,
      },
    });
  else
    await client.request('POST', '/v1/appStoreReviewDetails', {
      data: {
        type: 'appStoreReviewDetails',
        attributes,
        relationships: {
          appStoreVersion: { data: { type: 'appStoreVersions', id: version.id } },
        },
      },
    });
}

async function prepareMode(client, options) {
  const versionString = options.version;
  const buildNumber = buildNumberFor(versionString);
  const dryRun = options['dry-run'] === true;
  const timeoutMinutes = Number(options['timeout-minutes'] ?? 60);
  if (!(timeoutMinutes > 0)) throw new Error('--timeout-minutes must be a positive number');
  if (dryRun) log('Dry run: nothing is changed in App Store Connect.');
  const listing = loadListing();
  const app = await findApp(client);
  await prepareAppInfo(client, app, listing, dryRun);
  const { version, versions } = await ensureVersion(client, app, versionString, listing, dryRun);
  if (!version) return;
  const localization = await prepareVersionText(client, version, versions, listing, dryRun);
  if (localization) await prepareScreenshots(client, localization.id, listing, dryRun);
  else log('Screenshots: the version text does not exist yet; the real run uploads them');
  await prepareBuild(client, app, version, versionString, buildNumber, timeoutMinutes, dryRun);
  await prepareReviewDetail(client, version, listing, dryRun);
  log(
    `Done${dryRun ? ' (dry run)' : ''}: version ${versionString} is ${dryRun ? 'checked' : 'prepared'}. Check it in App Store Connect, then run submit.`
  );
  remindManualSteps();
}

// ---------- check and submit ----------

function remindManualSteps() {
  log(
    'By hand in App Store Connect (no API): App Privacy, Pricing and Availability, and the EU trader status (Business > Digital Services Act). See tooling/release-mobile/app-store/README.md.'
  );
}

/** Everything `submit` needs, as a list of problems (empty when the version can be submitted). */
async function versionProblems(client, app, version, listing) {
  const problems = [];
  const build = await requestOrNull(
    client,
    'GET',
    `/v1/appStoreVersions/${version.id}/build?fields[builds]=version,processingState,expired,usesNonExemptEncryption`
  );
  const b = build?.data?.attributes;
  if (!b) problems.push('no build attached (run prepare)');
  else {
    log(
      `Build: ${b.version} attached, ${b.processingState}, expired ${b.expired}, export compliance ${describeEncryption(b.usesNonExemptEncryption)}`
    );
    if (b.processingState !== 'VALID') problems.push(`build is ${b.processingState}`);
    if (b.expired) problems.push('build expired');
    if (b.usesNonExemptEncryption !== true && b.usesNonExemptEncryption !== false)
      problems.push('export compliance not answered for the build');
  }
  const localization = await findLocalization(
    client,
    `/v1/appStoreVersions/${version.id}/appStoreVersionLocalizations?limit=50`,
    listing.locale
  );
  const l = localization?.attributes ?? {};
  log(
    `Version text (${listing.locale}): description ${isSet(l.description)}, keywords ${isSet(l.keywords)}, support URL ${isSet(l.supportUrl)}, promotional text ${isSet(l.promotionalText)}, What's New ${isSet(l.whatsNew)}`
  );
  for (const field of ['description', 'keywords', 'supportUrl'])
    if (!l[field]) problems.push(`version ${field} missing`);
  if (localization) {
    for (const set of listing.screenshotSets) {
      const remote = await screenshotSet(client, localization.id, set.displayType);
      const shots = remote ? await remoteScreenshots(client, remote.id) : [];
      const complete = shots.filter((shot) => shot.attributes?.assetDeliveryState?.state === 'COMPLETE').length;
      log(
        `Screenshots ${set.displayType}: ${shots.length} in App Store Connect (${complete} processed), ${set.files.length} in ${set.dir}`
      );
      if (!complete) problems.push(`no screenshots for ${set.displayType}`);
    }
  }
  const detail = await requestOrNull(client, 'GET', `/v1/appStoreVersions/${version.id}/appStoreReviewDetail`);
  const d = detail?.data?.attributes ?? {};
  log(
    `App Review Information: contact ${isSet(d.contactFirstName && d.contactLastName && d.contactPhone && d.contactEmail)}, sign-in ${d.demoAccountRequired ? isSet(d.demoAccountName) : 'not required'}, notes ${d.notes ? `${d.notes.length} characters` : 'missing'}`
  );
  if (!(d.contactFirstName && d.contactLastName && d.contactPhone && d.contactEmail))
    problems.push('App Review contact incomplete');
  if (!d.notes) problems.push('App Review notes missing');
  if (d.demoAccountRequired && !d.demoAccountName) problems.push('App Review sign-in missing');
  return problems;
}

async function checkMode(client, options) {
  const listing = loadListing();
  for (const set of listing.screenshotSets)
    log(`Local screenshots ${set.displayType}: ${set.files.length} in ${set.dir}`);
  const app = await findApp(client);
  log(`Content rights: ${app.attributes?.contentRightsDeclaration ?? 'not answered'}`);
  const { editable, all } = await editableAppInfo(client, app.id);
  log(`App information: ${all.map((info) => info.attributes?.state).join(', ')}`);
  const info = editable ?? all[0];
  if (info) {
    const localization = await findLocalization(
      client,
      `/v1/appInfos/${info.id}/appInfoLocalizations?limit=50`,
      listing.locale
    );
    const a = localization?.attributes ?? {};
    log(
      `  ${listing.locale}: name "${a.name ?? ''}", subtitle ${isSet(a.subtitle)}, privacy policy URL ${isSet(a.privacyPolicyUrl)}`
    );
    log(
      `  Categories: ${info.relationships?.primaryCategory?.data?.id ?? 'none'}, ${info.relationships?.secondaryCategory?.data?.id ?? 'none'}`
    );
    const declaration = await requestOrNull(client, 'GET', `/v1/appInfos/${info.id}/ageRatingDeclaration`);
    const differing = Object.keys(changedFields(declaration?.data?.attributes, listing.ageRating));
    log(
      `  Age rating answers: ${differing.length ? `${differing.length} differ from app-store.json` : 'match app-store.json'}; rating ${info.attributes?.appStoreAgeRating ?? '?'}`
    );
  }
  const versions = await listVersions(client, app.id);
  log(`App Store versions (${versions.length}):`);
  for (const version of versions)
    log(
      `  ${version.attributes?.versionString} ${version.attributes?.appVersionState} (release ${version.attributes?.releaseType ?? '?'}, created ${version.attributes?.createdDate ?? '?'})`
    );
  const target = options.version
    ? versions.find((v) => v.attributes?.versionString === options.version)
    : versions.find((v) => EDITABLE_VERSION_STATES.has(v.attributes?.appVersionState));
  if (target) {
    log(`Version ${target.attributes.versionString} (${target.attributes.appVersionState}):`);
    const problems = await versionProblems(client, app, target, listing);
    log(problems.length ? `Not ready to submit: ${problems.join('; ')}` : 'Ready to submit.');
  } else if (options.version) {
    log(`Version ${options.version} does not exist yet; prepare creates it.`);
  }
  const { data: submissions } = await client.list(
    `/v1/reviewSubmissions?filter[app]=${app.id}&filter[platform]=${PLATFORM}&limit=20`
  );
  for (const submission of submissions.slice(0, 5))
    log(
      `Review submission ${submission.id}: ${submission.attributes?.state} (submitted ${submission.attributes?.submittedDate ?? 'no'})`
    );
  remindManualSteps();
}

async function submitMode(client, options) {
  const versionString = options.version;
  const dryRun = options['dry-run'] === true;
  if (dryRun) log('Dry run: nothing is submitted.');
  const listing = loadListing();
  const app = await findApp(client);
  const versions = await listVersions(client, app.id);
  const version = versions.find((v) => v.attributes?.versionString === versionString);
  if (!version) throw new Error(`App Store version ${versionString} does not exist; run prepare first`);
  const state = version.attributes.appVersionState;
  if (SUBMITTED_STATES.has(state)) {
    log(`Version ${versionString} is already ${state}; nothing to submit.`);
    return;
  }
  if (!EDITABLE_VERSION_STATES.has(state))
    throw new Error(`version ${versionString} is ${state}; it cannot be submitted`);
  const problems = await versionProblems(client, app, version, listing);
  if (problems.length) throw new Error(`version ${versionString} is not ready: ${problems.join('; ')}`);

  const { data: open } = await client.list(
    `/v1/reviewSubmissions?filter[app]=${app.id}&filter[platform]=${PLATFORM}&filter[state]=READY_FOR_REVIEW,WAITING_FOR_REVIEW,IN_REVIEW,UNRESOLVED_ISSUES&include=appStoreVersionForReview&limit=20`
  );
  const unresolved = open.find((s) => s.attributes?.state === 'UNRESOLVED_ISSUES');
  if (unresolved)
    throw new Error(
      "App Review left unresolved issues on an earlier submission; read Apple's message in App Store Connect and reply or resubmit there"
    );
  const pending = open.find((s) => SUBMITTED_STATES.has(s.attributes?.state));
  if (pending)
    throw new Error(`another submission is ${pending.attributes.state}; wait for it or cancel it in App Store Connect`);
  let submission = open.find((s) => s.attributes?.state === 'READY_FOR_REVIEW');
  if (dryRun) {
    log(
      `Dry run: would ${submission ? 'reuse the draft' : 'create a'} review submission, add version ${versionString} and submit it`
    );
    return;
  }
  if (!submission) {
    ({ data: submission } = await client.request('POST', '/v1/reviewSubmissions', {
      data: {
        type: 'reviewSubmissions',
        attributes: { platform: PLATFORM },
        relationships: { app: { data: { type: 'apps', id: app.id } } },
      },
    }));
    log(`Review submission: created ${submission.id}`);
  } else log(`Review submission: reusing draft ${submission.id}`);
  const { data: items } = await client.list(
    `/v1/reviewSubmissions/${submission.id}/items?include=appStoreVersion&limit=50`
  );
  if (items.some((item) => item.relationships?.appStoreVersion?.data?.id === version.id))
    log(`Review submission: version ${versionString} already added`);
  else {
    await client.request('POST', '/v1/reviewSubmissionItems', {
      data: {
        type: 'reviewSubmissionItems',
        relationships: {
          reviewSubmission: { data: { type: 'reviewSubmissions', id: submission.id } },
          appStoreVersion: { data: { type: 'appStoreVersions', id: version.id } },
        },
      },
    });
    log(`Review submission: added version ${versionString}`);
  }
  const { data: submitted } = await client.request('PATCH', `/v1/reviewSubmissions/${submission.id}`, {
    data: { type: 'reviewSubmissions', id: submission.id, attributes: { submitted: true } },
  });
  log(
    `Submitted version ${versionString} to App Review (${submitted?.attributes?.state ?? 'state unknown'}). Release type ${version.attributes.releaseType ?? listing.releaseType}.`
  );
}

function validateMode() {
  const listing = loadListing();
  for (const field of ['name', 'subtitle', 'promotionalText', 'description'])
    log(`${field}: ${listing[field].length}/${LIMITS[field]} characters`);
  log(`keywords: ${Buffer.byteLength(listing.keywords)}/100 bytes`);
  for (const set of listing.screenshotSets)
    log(
      `Screenshots ${set.displayType}: ${set.files.length} in ${set.dir}${set.files.length ? ` (${set.files[0].width}x${set.files[0].height})` : ''}`
    );
  const demoKeys = [
    'GHOSTEX_REVIEW_DEMO_ADDRESS',
    'GHOSTEX_REVIEW_DEMO_USERNAME',
    'GHOSTEX_REVIEW_DEMO_PASSWORD',
    'GHOSTEX_REVIEW_DEMO_SSH_PORT',
  ];
  const saved = Object.fromEntries(demoKeys.map((key) => [key, process.env[key]]));
  try {
    for (const key of demoKeys) delete process.env[key];
    const missing = reviewDetailFromEnv(listing.notesTemplate);
    for (const key of demoKeys) process.env[key] = '';
    const blank = reviewDetailFromEnv(listing.notesTemplate);
    for (const detail of [missing, blank]) {
      if (detail.demo.demoAccountRequired !== false)
        throw new Error('unset demo env must set demoAccountRequired to false');
      if ('demoAccountName' in detail.demo || 'demoAccountPassword' in detail.demo)
        throw new Error('unset demo env must not send demo credentials');
    }
    process.env.GHOSTEX_REVIEW_DEMO_ADDRESS = 'demo.example.com';
    process.env.GHOSTEX_REVIEW_DEMO_SSH_PORT = '22';
    process.env.GHOSTEX_REVIEW_DEMO_USERNAME = 'reviewer';
    process.env.GHOSTEX_REVIEW_DEMO_PASSWORD = '';
    if (reviewDetailFromEnv(listing.notesTemplate).demo.demoAccountRequired !== false)
      throw new Error('a demo address without a password must not require a sign-in');
    process.env.GHOSTEX_REVIEW_DEMO_PASSWORD = 'sample';
    const complete = reviewDetailFromEnv(listing.notesTemplate);
    if (
      complete.demo.demoAccountRequired !== true ||
      complete.demo.demoAccountName !== 'reviewer' ||
      complete.demo.demoAccountPassword !== 'sample' ||
      complete.demo.notes !== missing.demo.notes
    )
      throw new Error('demo username and password were not picked up, or they changed the notes');
    log(
      `review notes: ${missing.demo.notes.length}/${LIMITS.notes} characters; demo account not required unless username and password are both set`
    );
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
  log('Listing files are valid.');
}

async function main(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      version: { type: 'string' },
      'timeout-minutes': { type: 'string' },
      'dry-run': { type: 'boolean' },
    },
  });
  if (values.version !== undefined) buildNumberFor(values.version);
  const mode = positionals[0];
  if (mode === 'validate') return validateMode();
  if (!['check', 'prepare', 'submit'].includes(mode))
    throw new Error(
      'usage: asc-app-store.mjs validate|check|prepare|submit [--version x.y.z] [--dry-run] (see the header)'
    );
  if (mode !== 'check' && !values.version) throw new Error('--version is required');
  const client = ascClientFromEnv();
  if (mode === 'check') return checkMode(client, values);
  if (mode === 'prepare') return prepareMode(client, values);
  return submitMode(client, values);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
