import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_OWNER = 'Ezzy1630';
const REPO_NAME = 'argyph';
const PACKAGE_NAME = 'argyph';
const OUTPUT_PATH = 'badges/downloads.json';

const JSON_HEADERS = {
  Accept: 'application/json',
  'User-Agent': `${REPO_NAME}-download-badge`,
};

export function buildBadge(totalDownloads) {
  return {
    schemaVersion: 1,
    label: 'downloads',
    message: new Intl.NumberFormat('en-US').format(totalDownloads),
    color: 'yellowgreen',
  };
}

export async function calculateDownloads(sources = defaultSources()) {
  const counts = await Promise.all([
    sources.fetchCratesDownloads(),
    sources.fetchNpmDownloads(),
    sources.fetchGitHubReleaseDownloads(),
    sources.fetchHomebrewDownloads(),
  ]);

  return counts.reduce((total, count) => total + count, 0);
}

export function sumGitHubReleaseAssetDownloads(releases) {
  return releases.reduce((releaseTotal, release) => {
    const assets = Array.isArray(release.assets) ? release.assets : [];
    const assetTotal = assets.reduce((total, asset) => {
      return total + toCount(asset.download_count);
    }, 0);

    return releaseTotal + assetTotal;
  }, 0);
}

export function findHomebrewCount(analytics) {
  const items = Array.isArray(analytics.items) ? analytics.items : [];

  return items.reduce((total, item) => {
    const formula = String(item.formula ?? '').toLowerCase();
    if (formula !== PACKAGE_NAME && !formula.endsWith(`/${PACKAGE_NAME}`)) {
      return total;
    }

    return total + toCount(item.count);
  }, 0);
}

export async function fetchCratesDownloads(fetchJson = getJson) {
  const data = await fetchJson(`https://crates.io/api/v1/crates/${PACKAGE_NAME}`);
  return toCount(data?.crate?.downloads);
}

export async function fetchNpmDownloads(fetchJson = getJson) {
  const encodedPackage = encodeURIComponent(PACKAGE_NAME);
  const data = await fetchJson(
    `https://api.npmjs.org/downloads/point/1000-01-01:3000-01-01/${encodedPackage}`,
  );

  return toCount(data?.downloads);
}

export async function fetchGitHubReleaseDownloads(fetchJson = getJson) {
  let page = 1;
  let total = 0;

  while (true) {
    const releases = await fetchJson(
      `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases?per_page=100&page=${page}`,
    );
    if (!Array.isArray(releases) || releases.length === 0) {
      break;
    }

    total += sumGitHubReleaseAssetDownloads(releases);

    if (releases.length < 100) {
      break;
    }
    page += 1;
  }

  return total;
}

export async function fetchHomebrewDownloads(fetchJson = getJson) {
  const data = await fetchJson(
    'https://formulae.brew.sh/api/analytics/install-on-request/365d.json',
  );

  return findHomebrewCount(data);
}

export function defaultSources() {
  return {
    fetchCratesDownloads,
    fetchNpmDownloads,
    fetchGitHubReleaseDownloads,
    fetchHomebrewDownloads,
  };
}

async function getJson(url) {
  const response = await fetch(url, { headers: JSON_HEADERS });
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

function toCount(value) {
  const count = Number(String(value).replaceAll(',', ''));
  return Number.isFinite(count) && count > 0 ? count : 0;
}

async function writeBadge(path = OUTPUT_PATH) {
  const totalDownloads = await calculateDownloads();
  const badge = buildBadge(totalDownloads);
  const json = `${JSON.stringify(badge, null, 2)}\n`;

  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, json, 'utf8');

  return badge;
}

async function main() {
  const badge = await writeBadge();
  console.log(`Updated ${OUTPUT_PATH}: downloads ${badge.message}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
