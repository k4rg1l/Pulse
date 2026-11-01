// The hero's background changes slowly, like a desktop wallpaper.

const PHOTOS = ["wallpaper", "lake", "clouds", "stars", "ocean", "cliffs", "forest"];

const LATEST_RELEASE = "https://api.github.com/repos/k4rg1l/Pulse/releases/latest";

/** How long each photo stays before the next begins to appear. */
const HOLD_MS = 11000;

/** Matches the transition on .backdrop in site.css. */
const FADE_MS = 2800;

/** Matches the media query that chooses the first photo in site.css. */
const LARGE_SCREEN = "(min-width: 1921px), (min-resolution: 1.5dppx)";

const photoWidth = matchMedia(LARGE_SCREEN).matches ? 3840 : 1920;

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Resolves once this tab is on screen, so a hidden tab downloads nothing. */
function untilVisible() {
  if (!document.hidden) return Promise.resolve();
  return new Promise((resolve) => {
    document.addEventListener("visibilitychange", resolve, { once: true });
  });
}

/** Downloads and decodes a photo, so its first frame is complete when it appears. */
async function prepare(name) {
  const url = `backgrounds/${name}-${photoWidth}.jpg`;
  const image = new Image();
  image.src = url;
  await image.decode();
  return url;
}

/**
 * Fades the new photo in over the current one, which stays solid underneath.
 * Fading both at once would let the dark page show through halfway.
 */
async function reveal(url, incoming, outgoing) {
  outgoing.classList.remove("backdrop-front");
  incoming.style.backgroundImage = `url("${url}")`;
  incoming.classList.add("backdrop-front", "backdrop-showing");
  await wait(FADE_MS);
  outgoing.classList.remove("backdrop-showing");
}

async function rotate() {
  let layers = [...document.querySelectorAll(".backdrop")];
  let current = 0;

  for (;;) {
    await wait(HOLD_MS);
    await untilVisible();
    const next = (current + 1) % PHOTOS.length;
    try {
      const url = await prepare(PHOTOS[next]);
      await reveal(url, layers[1], layers[0]);
      layers = [layers[1], layers[0]];
    } catch {
      // A photo that fails to load is skipped, and the current one simply stays.
    }
    current = next;
  }
}

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
if (!reducedMotion && !navigator.connection?.saveData) rotate();

/** Names the newest release beside the download button, so it never goes stale. */
async function showVersion() {
  const response = await fetch(LATEST_RELEASE);
  if (!response.ok) return;
  const { tag_name: tag } = await response.json();
  document.getElementById("version").textContent = ` Version ${tag.replace(/^v/, "")}.`;
}

// Without GitHub the line simply reads as it did, so a failure is not worth showing.
showVersion().catch(() => {});
