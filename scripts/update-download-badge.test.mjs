import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildBadge,
  calculateDownloads,
  findHomebrewCount,
  sumGitHubReleaseAssetDownloads,
} from './update-download-badge.mjs';

test('sums every source into one total download count', async () => {
  const total = await calculateDownloads({
    fetchCratesDownloads: async () => 43,
    fetchNpmDownloads: async () => 266,
    fetchGitHubReleaseDownloads: async () => 133,
    fetchHomebrewDownloads: async () => 7,
  });

  assert.equal(total, 449);
});

test('sums all GitHub release asset download counts without filtering asset names', () => {
  const releases = [
    {
      assets: [
        { name: 'argyph-x86_64-unknown-linux-gnu.tar.xz', download_count: 12 },
        { name: 'argyph-x86_64-unknown-linux-gnu.tar.xz.sha256', download_count: 10 },
      ],
    },
    {
      assets: [
        { name: 'argyph.dxt', download_count: 3 },
      ],
    },
  ];

  assert.equal(sumGitHubReleaseAssetDownloads(releases), 25);
});

test('finds Homebrew analytics entries for a tap formula or bare formula', () => {
  const analytics = {
    items: [
      { formula: 'other', count: '99' },
      { formula: 'ezzy1630/argyph/argyph', count: '1,005' },
      { formula: 'argyph', count: '2' },
    ],
  };

  assert.equal(findHomebrewCount(analytics), 1007);
});

test('builds Shields endpoint JSON for the combined count', () => {
  assert.deepEqual(buildBadge(1234), {
    schemaVersion: 1,
    label: 'downloads',
    message: '1,234',
    color: 'yellowgreen',
  });
});
