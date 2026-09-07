import Downloads from './downloads';
import DownloadScroll from './download-scroll';
import release from './data/downloads.json';

const repo = 'https://github.com/Reqwey/AZULC';
const features = [
  [
    'new-instance-wizard',
    'CREATE YOUR INSTANCE',
    'Your version. Your way to play.',
    'Start with vanilla or add your favorite mod loader. Choose a Minecraft version, a compatible loader, and a name to create an isolated instance.',
    'Vanilla / Fabric / Forge / NeoForge',
    'Create an instance',
  ],
  [
    'modpack-list',
    'FIND YOUR NEXT WORLD',
    'Find your next modpack right here.',
    'Browse CurseForge and Modrinth without leaving the launcher. Pick a modpack and a release, or import a local CurseForge, Modrinth, or MultiMC / Prism archive.',
    'CurseForge / Modrinth / Local import',
    'Browse modpacks',
  ],
  [
    'modpack-install-pipline',
    'EVERY STEP, VISIBLE',
    'Know exactly where your install stands.',
    'Follow the game, loader, and modpack content through one installation pipeline. See file counts, transfer speeds, and live logs. Cancel when needed; retry with verified files already in place.',
    'Parallel downloads / Official & mirror sources / Cancel & retry',
    'Install pipeline',
  ],
  [
    'instance-mods',
    'A PLACE FOR EVERYTHING',
    'A space for every instance.',
    'Keep worlds, mods, resource packs, and screenshots organized by instance. Download mods, shaders, and data packs with their required dependencies, keeping each setup separate.',
    'Isolated instances / Content downloads / Required dependencies',
    'Manage instance content',
  ],
  [
    'launch',
    'READY. SET. PLAY.',
    'Press play. Stay in the know.',
    'Discover Java automatically or choose a runtime for each instance. Monitor concurrent launches independently, with game readiness, live logs, and failure details in view.',
    'Java discovery / Independent launch monitoring / Play time',
    'Launch monitor',
  ],
];
function Screen({
  name,
  caption,
  eager = false,
}: {
  name: string;
  caption: string;
  eager?: boolean;
}) {
  return (
    <figure className="screen">
      <a
        href={`/screenshots/AZULC-${name}.png`}
        target="_blank"
        rel="noreferrer"
        aria-label={`View full screenshot: ${caption}`}
      >
        <img
          src={`/screenshots/AZULC-${name}.png`}
          alt={caption}
          width="2380"
          height="1435"
          loading={eager ? 'eager' : 'lazy'}
        />
      </a>
    </figure>
  );
}
export default function Home() {
  return (
    <>
      <a className="skip" href="#main">
        Skip to content
      </a>
      <header className="header">
        <a className="brand" href="#">
          <img src="/assets/app-icon.png" alt="" width="36" height="36" />
          <span>
            AZULC<small>Azusa Minecraft Launcher</small>
          </span>
        </a>
        <nav aria-label="Main navigation">
          <a href="#features">Features</a>
          <a href="#about">About</a>
          <a href={repo}>GitHub ↗</a>
        </nav>
        <a className="button small" href="#download">
          Get the launcher ↓
        </a>
      </header>
      <main id="main">
        <section className="hero wrap">
          <div className="eyebrow">◆ &nbsp; NATIVE. PIXEL. MINECRAFT.</div>
          <div className="hero-copy">
            <div>
              <h1>
                Your Minecraft.
                <br />
                <em>Starts here.</em>
              </h1>
              <p>
                AZULC is a native Minecraft launcher built in Rust.
                <br />
                Install, manage, and launch, from vanilla to modpacks.
              </p>
              <div className="actions">
                <a className="button" href="#download">
                  Get AZULC <span>↓</span>
                </a>
                <a className="text-link" href="#features">
                  Explore features ↘
                </a>
              </div>
            </div>
            <div className="hero-note">
              <span className="wordmark">AZULC</span>
              <span>Azusa Minecraft Launcher</span>
              <span className="note-line">Built on Iced UI</span>
              <span className="status">▪ GPL-3.0-or-later</span>
            </div>
          </div>
          <div className="hero-screen">
            <a
              href="/screenshots/AZULC-home.png"
              target="_blank"
              rel="noreferrer"
              aria-label="View full home screenshot"
            >
              <img
                src="/screenshots/AZULC-home.png"
                alt="Home · Your instances, worlds, and play time"
                width="2380"
                height="1435"
                loading="eager"
              />
            </a>
          </div>
          <div className="compatibility">
            <span>Play your way</span>
            <strong>Vanilla</strong>
            <strong>Fabric</strong>
            <strong>Forge</strong>
            <strong>NeoForge</strong>
            <span className="compat-end">ALL IN ONE PLACE</span>
          </div>
        </section>
        <section id="features" className="features wrap">
          <div className="section-heading">
            <div>
              <span className="eyebrow">FROM SETUP TO PLAY</span>
              <h2>From a fresh instance to your next world.</h2>
            </div>
            <p>Everything you use, right where you need it.</p>
          </div>
          {features.map(([image, label, title, text, tags, caption], i) => (
            <article
              className={`feature ${i % 2 ? 'reverse' : ''}`}
              key={image}
            >
              <div className="feature-copy">
                <span className="feature-number">
                  0{i + 1} <span>/</span>
                </span>
                <span className="eyebrow">{label}</span>
                <h3>{title}</h3>
                <p>{text}</p>
                <div className="tags">
                  {tags.split(' / ').map((t) => (
                    <span key={t}>{t}</span>
                  ))}
                </div>
              </div>
              <Screen name={image} caption={caption} />
            </article>
          ))}
        </section>
        <section className="details wrap">
          {[
            [
              '@',
              'Your accounts, together',
              'Sign in with a Microsoft device code and manage multiple Microsoft accounts.',
            ],
            [
              '◇',
              'New releases and old favorites',
              'Browse releases, snapshots, legacy versions, and April Fools builds.',
            ],
            [
              '↓',
              'Choose how you download',
              'Configure official and mirror download routes, with parallel transfers for the files you need.',
            ],
          ].map(([icon, title, text]) => (
            <article key={title}>
              <span className="detail-icon">{icon}</span>
              <h3>{title}</h3>
              <p>{text}</p>
            </article>
          ))}
        </section>
        <section id="about" className="about wrap">
          <div>
            <span className="eyebrow">BUILT IN THE OPEN</span>
            <h2>
              Built native.
              <br />
              Open to everyone.
            </h2>
            <p>
              Built with Rust and Iced, AZULC separates the UI, application
              state, and services to keep every install and launch step
              observable.
            </p>
            <p>
              AZULC is open source under GPL-3.0-or-later. Explore the code,
              report an issue, or contribute to the project.
            </p>
            <a className="text-link" href={repo}>
              Explore the project on GitHub ↗
            </a>
          </div>
          <figure className="architecture">
            <img
              src="/assets/three-layer-shuttle.svg"
              width="900"
              height="1240"
              alt="AZULC three-layer architecture: UI, application state, and services"
              loading="lazy"
            />
          </figure>
        </section>
        <section className="download wrap">
          <img
            className="brand-banner"
            src="/assets/readme-header.png"
            width="1600"
            height="520"
            alt="Azusa Minecraft Launcher"
            loading="lazy"
          />
          <div className="download-content">
            <span className="eyebrow">YOUR NEXT SESSION STARTS HERE</span>
            <h2>Ready for your next session.</h2>
            <p>
              Download {release.version} for your device.{' '}
              <a className="text-link" href={release.releaseUrl}>
                Release notes ↗
              </a>
            </p>
            <Downloads />
            <a className="text-link" href={`${repo}/tree/main#run`}>
              Read the build instructions ↗
            </a>
          </div>
        </section>
      </main>
      <footer className="footer wrap">
        <a className="pixel" href="#">
          AZULC ↑
        </a>
        <p>
          Azusa Minecraft Launcher
          <br />
          <span>
            Not an official Minecraft product. Not affiliated with Mojang or
            Microsoft.
          </span>
        </p>
        <a href={`${repo}/tree/main#license`}>GPL-3.0-or-later ↗</a>
      </footer>
      <div id="download" aria-hidden="true" />
      <DownloadScroll />
    </>
  );
}
