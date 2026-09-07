'use client';

import { useEffect, useState } from 'react';
import release from './data/downloads.json';

type Platform = 'windows' | 'macos' | 'linux' | 'unknown';

function detectPlatform(): Platform {
  const ua = navigator.userAgent;
  if (
    /Android|iPhone|iPad|iPod/i.test(ua) ||
    (/Macintosh/i.test(ua) && navigator.maxTouchPoints > 1)
  )
    return 'unknown';
  if (/Windows NT/i.test(ua)) return 'windows';
  if (/Macintosh|Mac OS X/i.test(ua)) return 'macos';
  if (/Linux|X11/i.test(ua) && !/CrOS/i.test(ua)) return 'linux';
  return 'unknown';
}

export default function Downloads() {
  const [platform, setPlatform] = useState<Platform>('unknown');
  useEffect(() => setPlatform(detectPlatform()), []);
  const downloads = release.downloads.filter(
    (item) => platform === 'unknown' || item.id.startsWith(`${platform}-`),
  );

  return (
    <>
      <div className="platforms detected-downloads">
        {downloads.map(({ id, label, detail, url }) =>
          url ? (
            <a
              className="download-card"
              href={url}
              key={id}
              aria-label={`Download AZULC ${release.version} for ${label}, ${detail}`}
            >
              <span className="pixel">{label}</span>
              <span>{detail}</span>
              <b aria-hidden="true">↓</b>
            </a>
          ) : (
            <div className="download-card unavailable" key={id}>
              <span className="pixel">{label}</span>
              <span>{detail}</span>
              <span>Not available in this release</span>
            </div>
          ),
        )}
      </div>
      {platform === 'macos' && (
        <div className="mac-instructions">
          <h3>Open AZULC on macOS</h3>
          <p>
            Choose Intel for Intel-based Macs or Apple Silicon for M-series
            Macs. Your browser does not reliably identify your Mac’s chip.
          </p>
          <p>
            Move AZULC.app to Applications. If macOS blocks the downloaded app,
            run this command in Terminal to remove its quarantine flag:
          </p>
          <pre>
            <code>
              {'xattr -dr com.apple.quarantine "/Applications/AZULC.app"'}
            </code>
          </pre>
        </div>
      )}
      <p className="download-note">
        Packages are unsigned.{' '}
        {platform === 'linux' &&
          'Linux requires a desktop environment and X11 / Wayland and xkbcommon runtime libraries. '}
        See the release notes for instructions and checksums.
      </p>
      <a className="text-link" href={release.releaseUrl}>
        Downloads for other platforms ↗
      </a>
    </>
  );
}
