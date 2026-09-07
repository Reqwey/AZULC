import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildDownloads } from './update-downloads.mjs';

const asset = (platform) => ({
  name: `AZULC-1.2.3-${platform}.zip`,
  browser_download_url: `https://github.com/Reqwey/AZULC/releases/download/v1.2.3/AZULC-1.2.3-${platform}.zip`,
});
test('keeps Intel and ARM downloads distinct regardless of asset order', () => {
  const data = buildDownloads({
    tag_name: 'v1.2.3',
    assets: [asset('macos-arm64'), asset('macos-x64')],
  });
  assert.match(
    data.downloads.find((d) => d.id === 'macos-x64').url,
    /macos-x64.zip$/,
  );
  assert.match(
    data.downloads.find((d) => d.id === 'macos-arm64').url,
    /macos-arm64.zip$/,
  );
  assert.equal(data.downloads.find((d) => d.id === 'windows-x64').url, null);
});
test('rejects ambiguous packages, external URLs, and prereleases', () => {
  assert.throws(
    () =>
      buildDownloads({
        tag_name: 'v1.2.3',
        assets: [asset('macos-x64'), asset('macos-x64')],
      }),
    /Ambiguous/,
  );
  assert.throws(
    () =>
      buildDownloads({
        tag_name: 'v1.2.3',
        assets: [
          {
            ...asset('macos-x64'),
            browser_download_url: 'https://example.com/package.zip',
          },
        ],
      }),
    /Unexpected/,
  );
  assert.throws(
    () => buildDownloads({ tag_name: 'v1.2.3', prerelease: true }),
    /stable/,
  );
  assert.throws(
    () => buildDownloads({ tag_name: 'v1.2.3', assets: [] }),
    /no recognized/,
  );
});
