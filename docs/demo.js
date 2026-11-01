// The story's pinned panel, and the live Audio panel inside it. The panel draws
// the way the app does: layered waves sized by bass, mids and treble, a soft trail,
// and each app's colour. Only the sound here is simulated.

const POINTS = 96;
const FRAME_MS = 1000 / 30;
const TRAIL_FADE = 0.38;
const ATTACK = 0.55;
const RELEASE = 0.09;
const FAINT = "rgba(240, 234, 222, 0.24)";

const BAND_SHARE = [0.62, 0.26, 0.12];
const BAND_WAVES = [1.5, 3.7, 8.3];
const BAND_DRIFT = [1.6, 2.4, 4];
const BAND_SURGE = [3, 4, 6];

/** The three sounds on the demo, each with its own rhythm. */
const APPS = {
  music: { color: "#4fd17c", volume: 0.8, muted: false, seed: 1, beat: 2.03, weight: [1, 0.7, 0.5] },
  browser: { color: "#5ab6f2", volume: 0.55, muted: false, seed: 2, beat: 1.1, weight: [0.5, 0.9, 0.4] },
  game: { color: "#f0886a", volume: 0.4, muted: false, seed: 3, beat: 3.1, weight: [0.8, 0.5, 0.7] },
};

const clamp = (value) => Math.min(1, Math.max(0, value));

/** What an app is playing right now: a beat that lands hard and dies away. */
function bandsOf(app, seconds) {
  if (app.muted) return [0, 0, 0];
  const phase = (seconds * app.beat + app.seed * 0.3) % 1;
  const kick = Math.exp(-phase * 5);
  const swell = 0.75 + 0.25 * Math.sin(seconds * 0.7 + app.seed);
  return app.weight.map((weight, band) => {
    const body = band === 0 ? 0.25 + 0.75 * kick : 0.5 + 0.5 * Math.sin(seconds * (3 + band) + app.seed);
    return clamp(weight * body * swell);
  });
}

function masterBands(seconds) {
  const apps = Object.values(APPS);
  return [0, 1, 2].map((band) =>
    clamp(Math.max(...apps.map((app) => bandsOf(app, seconds)[band] * app.volume)) * master.volume * 1.4),
  );
}

const master = { volume: 0.62, seed: 5 };

/** Fills a line with three waves, narrowing to nothing at its ends. */
function fillWave(line, phases, bands, seed) {
  for (let index = 0; index < POINTS; index++) {
    const position = index / (POINTS - 1);
    const narrowing = Math.sin(position * Math.PI) ** 0.6;
    let height = 0;
    for (let band = 0; band < 3; band++) {
      const waves = BAND_WAVES[band] + seed * 0.13 * (band + 1);
      const direction = band === 1 ? -1 : 1;
      height +=
        Math.sin(position * Math.PI * 2 * waves + direction * phases[band] + seed) *
        bands[band] *
        BAND_SHARE[band];
    }
    line[index] = height * narrowing;
  }
}

/** One living fader: a canvas, its line, and how it answers a pointer. */
function createFader(element, { read, write, bands, colour, seed, large }) {
  const canvas = element.querySelector("canvas");
  const context = canvas.getContext("2d");
  const line = new Float32Array(POINTS);
  const shown = [0, 0, 0];
  const phases = [0, 0, 0];

  function trace(width, middle, reach) {
    const x = (index) => (index / (POINTS - 1)) * width;
    const y = (index) => middle - line[index] * reach;
    context.beginPath();
    context.moveTo(x(0), y(0));
    for (let index = 1; index < POINTS - 1; index++) {
      context.quadraticCurveTo(x(index), y(index), (x(index) + x(index + 1)) / 2, (y(index) + y(index + 1)) / 2);
    }
    context.lineTo(x(POINTS - 1), y(POINTS - 1));
  }

  function draw(seconds, elapsed) {
    const scale = devicePixelRatio;
    const width = Math.round(canvas.clientWidth * scale);
    const height = Math.round(canvas.clientHeight * scale);
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const target = bands(seconds);
    for (let band = 0; band < 3; band++) {
      shown[band] += (target[band] - shown[band]) * (target[band] > shown[band] ? ATTACK : RELEASE);
      phases[band] += elapsed * (BAND_DRIFT[band] + shown[band] * BAND_SURGE[band]);
    }
    fillWave(line, phases, shown, seed);

    context.globalCompositeOperation = "destination-out";
    context.fillStyle = `rgba(0, 0, 0, ${TRAIL_FADE})`;
    context.fillRect(0, 0, width, height);
    context.globalCompositeOperation = "source-over";
    context.lineJoin = "round";
    context.lineCap = "round";

    const position = read() * width;
    const middle = height / 2;
    const reach = height / 2 - 2 * scale;

    context.save();
    context.beginPath();
    context.rect(position, 0, width - position, height);
    context.clip();
    trace(width, middle, reach);
    context.lineWidth = (large ? 1.5 : 1.25) * scale;
    context.strokeStyle = FAINT;
    context.stroke();
    context.restore();

    const lit = colour(context, position);
    context.save();
    context.beginPath();
    context.rect(0, 0, position, height);
    context.clip();
    trace(width, middle, reach);
    context.lineWidth = (large ? 2 : 1.5) * scale;
    context.strokeStyle = lit;
    context.shadowColor = typeof lit === "string" ? lit : "#d8c29a";
    context.shadowBlur = (large ? 10 : 4) * scale;
    context.stroke();
    context.restore();

    const cap = 1.75 * scale;
    context.fillStyle = typeof lit === "string" ? lit : "#d8c29a";
    context.fillRect(Math.min(width - cap, Math.max(0, position - cap / 2)), 0, cap, height);
  }

  function follow(event) {
    const box = element.getBoundingClientRect();
    write(clamp((event.clientX - box.left) / box.width));
  }
  element.addEventListener("pointerdown", (event) => {
    element.setPointerCapture(event.pointerId);
    follow(event);
  });
  element.addEventListener("pointermove", (event) => {
    if (element.hasPointerCapture(event.pointerId)) follow(event);
  });
  element.addEventListener("keydown", (event) => {
    const step = { ArrowRight: 0.02, ArrowUp: 0.02, ArrowLeft: -0.02, ArrowDown: -0.02 }[event.key];
    if (!step) return;
    event.preventDefault();
    write(clamp(read() + step));
  });

  return { draw };
}

/** The master line is every app that is playing, blended by how loud each is. */
function masterColour(context, length, seconds) {
  const parts = Object.values(APPS)
    .map((app) => ({ color: app.color, weight: bandsOf(app, seconds)[0] * app.volume + (app.muted ? 0 : 0.2) }))
    .filter((part) => part.weight > 0.01 && Object.values(APPS).some((app) => app.color === part.color && !app.muted));
  if (parts.length === 0) return "#d8c29a";
  if (parts.length === 1) return parts[0].color;

  const total = parts.reduce((sum, part) => sum + part.weight, 0);
  const gradient = context.createLinearGradient(0, 0, Math.max(1, length), 0);
  let reached = 0;
  for (const { color, weight } of parts) {
    gradient.addColorStop(Math.min(1, (reached + weight / 2) / total), color);
    reached += weight;
  }
  return gradient;
}

function startDemo() {
  const percent = (name, value) => {
    document.querySelector(`[data-percent="${name}"]`).textContent = APPS[name].muted
      ? "Muted"
      : `${Math.round(value * 100)}%`;
    document.querySelector(`[data-fader="${name}"]`).setAttribute("aria-valuenow", Math.round(value * 100));
  };

  let seconds = 0;
  const faders = [
    createFader(document.querySelector('[data-fader="master"]'), {
      read: () => master.volume,
      write: (value) => {
        master.volume = value;
        document.getElementById("demo-volume").textContent = Math.round(value * 100);
        document.querySelector('[data-fader="master"]').setAttribute("aria-valuenow", Math.round(value * 100));
      },
      bands: masterBands,
      colour: (context, length) => masterColour(context, length, seconds),
      seed: master.seed,
      large: true,
    }),
    ...Object.entries(APPS).map(([name, app]) =>
      createFader(document.querySelector(`[data-fader="${name}"]`), {
        read: () => app.volume,
        write: (value) => {
          app.volume = value;
          percent(name, value);
        },
        bands: (now) => bandsOf(app, now).map((band) => band * (0.4 + app.volume * 0.6)),
        colour: () => app.color,
        seed: app.seed,
      }),
    ),
  ];

  for (const button of document.querySelectorAll("[data-mute]")) {
    button.addEventListener("click", () => {
      const name = button.dataset.mute;
      APPS[name].muted = !APPS[name].muted;
      button.setAttribute("aria-pressed", APPS[name].muted);
      percent(name, APPS[name].volume);
    });
  }

  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let visible = false;
  new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
  }).observe(document.querySelector(".demo"));

  let last = 0;
  function loop(now) {
    requestAnimationFrame(loop);
    const elapsed = (now - last) / 1000;
    if (!visible || calm || now - last < FRAME_MS) return;
    last = now;
    seconds = now / 1000;
    for (const fader of faders) fader.draw(seconds, Math.min(elapsed, 0.1));
  }
  requestAnimationFrame(loop);
  if (calm) for (const fader of faders) fader.draw(0, 0);
}

function startStory() {
  const story = document.querySelector(".story");
  const frames = [...story.querySelectorAll(".story-frame")];
  const steps = [...story.querySelectorAll(".story-step")];
  frames.forEach((frame) => frame.style.setProperty("--frame", frame.dataset.frame));
  story.classList.add("story-live");
  steps[0].classList.add("story-step-showing");

  // The step crossing the middle of the screen chooses the panel.
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const showing = entry.target.dataset.step;
        steps.forEach((step) => step.classList.toggle("story-step-showing", step === entry.target));
        frames.forEach((frame) =>
          frame.classList.toggle("story-frame-showing", frame.dataset.frame === showing),
        );
      }
    },
    { rootMargin: "-50% 0px -50% 0px" },
  );
  steps.forEach((step) => observer.observe(step));
}

startStory();
startDemo();
