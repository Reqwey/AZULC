import { mkdir, writeFile, rename } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const repository = 'https://github.com/Reqwey/AZULC';
const platforms = [
  ['windows-x64', 'Windows', 'x64 · ZIP', /-windows-x64\.zip$/i],
  ['macos-x64', 'macOS Intel', 'x64 · ZIP', /-macos-x64\.zip$/i],
  ['macos-arm64', 'macOS Apple Silicon', 'ARM64 · ZIP', /-macos-arm64\.zip$/i],
  ['linux-x64', 'Linux', 'x64 · tar.gz', /-linux-x64\.tar\.gz$/i],
];

export function buildDownloads(release) {
  if (release.draft || release.prerelease || !release.tag_name) {
    throw new Error('Expected a published stable release.');
  }
  const releaseUrl = `${repository}/releases/tag/${encodeURIComponent(release.tag_name)}`;
  const assetPrefix = `${repository}/releases/download/${encodeURIComponent(release.tag_name)}/`;
  const assets = release.assets ?? [];
  const downloads = platforms.map(([id, label, detail, pattern]) => {
    const matches = assets.filter((asset) => pattern.test(asset.name));
    if (matches.length > 1)
      throw new Error(`Ambiguous release assets for ${id}.`);
    const asset = matches[0];
    if (asset && !asset.browser_download_url.startsWith(assetPrefix)) {
      throw new Error(`Unexpected download URL for ${id}.`);
    }
    return { id, label, detail, url: asset?.browser_download_url ?? null };
  });
  if (!downloads.some((download) => download.url)) {
    throw new Error('Latest release has no recognized platform packages.');
  }
  return { version: release.tag_name, releaseUrl, downloads };
}

async function request(url, authenticated = false) {
  const headers = { 'User-Agent': 'AZULC-website-download-updater' };
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (authenticated) {
    headers.Accept = 'application/vnd.github+json';
    if (token) headers.Authorization = `Bearer ${token}`;
  }
  const response = await fetch(url, {
    headers,
    signal: AbortSignal.timeout(30_000),
  });
  if (!response.ok) throw new Error(`GitHub returned HTTP ${response.status}.`);
  return response;
}

async function latestRelease() {
  try {
    return await (
      await request(
        'https://api.github.com/repos/Reqwey/AZULC/releases/latest',
        true,
      )
    ).json();
  } catch {
    // Public release pages also work when the shared anonymous API quota is exhausted.
    console.warn(
      'GitHub API unavailable; checking the public latest release page.',
    );
    const page = await request(`${repository}/releases/latest`);
    const prefix = `${repository}/releases/tag/`;
    if (!page.url.startsWith(prefix))
      throw new Error('No published latest release found.');
    const tag = decodeURIComponent(
      new URL(page.url).pathname.slice('/Reqwey/AZULC/releases/tag/'.length),
    );
    const html = await (
      await request(
        `${repository}/releases/expanded_assets/${encodeURIComponent(tag)}`,
      )
    ).text();
    const assets = [
      ...html.matchAll(
        /href="(\/Reqwey\/AZULC\/releases\/download\/[^"?#]+)"/g,
      ),
    ].map((match) => ({
      name: decodeURIComponent(match[1].split('/').pop()),
      browser_download_url: `https://github.com${match[1]}`,
    }));
    return { tag_name: tag, assets };
  }
}

async function main() {
  const data = buildDownloads(await latestRelease());
  const directory = new URL('../app/data/', import.meta.url);
  const destination = new URL('downloads.json', directory);
  const temporary = new URL('downloads.json.tmp', directory);
  await mkdir(directory, { recursive: true });
  await writeFile(temporary, `${JSON.stringify(data, null, 2)}\n`);
  await rename(temporary, destination);
  console.log(
    `Updated ${data.version}: ${data.downloads.filter((entry) => entry.url).length}/4 download links.`,
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main().catch((error) => {
    console.error(`Download update failed: ${error.message}`);
    process.exitCode = 1;
  });
}
