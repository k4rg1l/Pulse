import { useEffect, useRef, type KeyboardEvent, type PointerEvent } from "react";
import { advancePhases, fillWave, type Bands } from "./signal";
import type { Moment } from "./useAudio";

/** Points on the line, enough for a smooth curve at any panel size. */
const LINE_POINTS = 96;

/** How quickly a band rises to a sound, so the line hits on the beat. */
const ATTACK = 0.55;

/** How quickly a band falls away, so the line melts back between beats. */
const RELEASE = 0.09;

/** How quickly colours in a blend follow each app's loudness. */
const BLEND_EASE = 0.08;

/** How much of the last frame fades each time, which leaves a soft trail. */
const TRAIL_FADE = 0.38;

/** The line redraws this often. Matches FRAME in the audio worker. */
const FRAME_MS = 1000 / 30;

/** Below this a band is silence, and the line needs no more drawing. */
const AT_REST = 0.004;

/** The light is drawn this many times smaller, since the blur removes all detail. */
const AURA_SHRINK = 16;

/** How bright the light behind the line is at full sound. */
const AURA_STRENGTH = 0.42;

/** A key press moves the fader this far. */
const KEY_STEP = 0.02;

/** How far the track stretches when pulled past its end, as a share of its width. */
const STRETCH_LIMIT = 0.05;

/** The drag distance past the end, in track widths, that reaches half of that. */
const STRETCH_SOFTNESS = 0.12;

/** One colour in a blend, and how much of the blend it should take up. */
export interface BlendPart {
  color: string;
  weight: number;
}

interface LivingFaderProps {
  label: string;
  /** Position of the fader, from 0 to 1. */
  value: number;
  onChange: (value: number) => void;
  /** The user has let go. */
  onRelease?: () => void;
  /** Whether sound is reaching this fader. Without it the line lies flat. */
  sounding: boolean;
  /** The latest moment of sound, read as the line draws. */
  moment: () => Moment | undefined;
  /** Gives each line its own shape. */
  seed: number;
  /** Colour of the lit part of the line. */
  tint: string;
  /** Colours to blend across the lit part, in place of the tint while there are any. */
  blend?: () => BlendPart[];
  /** False when nobody can see the line, so it costs nothing. */
  live: boolean;
  large?: boolean;
}

const clamp = (value: number) => Math.min(1, Math.max(0, value));

/** Resistance past the end of the track: more pull, less give. */
function stretchFor(overshoot: number): number {
  const distance = Math.abs(overshoot);
  return (STRETCH_LIMIT * distance) / (distance + STRETCH_SOFTNESS);
}

/** A level alone, for sound Windows won't share, as with a microphone. */
function bandsFromPeak(peak: number): Bands {
  const level = Math.sqrt(Math.min(1, peak));
  return [level, level * 0.6, level * 0.3];
}

/**
 * A volume control that is also the sound it controls. Layered waves run the
 * length of the track, sized by the real bass, mids and treble, lit up to the
 * fader's position and faint beyond it. Silence is a flat line, like the heartbeat
 * in Pulse's icon. Each frame fades rather than clears, so movement leaves a soft
 * trail, and behind the lit part, soft light swells with the sound.
 */
export function LivingFader({
  label,
  value,
  onChange,
  onRelease,
  sounding,
  moment,
  seed,
  tint,
  blend,
  live,
  large = false,
}: LivingFaderProps) {
  const stageRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const auraRef = useRef<HTMLCanvasElement>(null);
  const latest = useRef({ value, sounding, tint, moment, blend });
  latest.current = { value, sounding, tint, moment, blend };

  useEffect(() => {
    const canvas = canvasRef.current;
    const aura = auraRef.current;
    const context = canvas?.getContext("2d");
    const auraContext = aura?.getContext("2d");
    if (!canvas || !aura || !context || !auraContext) return;

    const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const faint = getComputedStyle(canvas).getPropertyValue("--fader-faint");
    const line = new Float32Array(LINE_POINTS);
    const bands: Bands = [0, 0, 0];
    const phases: Bands = [0, 0, 0];
    const mix = new Map<string, number>();
    let frame = 0;
    let lastDrawn = 0;
    let onScreen = true;
    let drawnAt = "";

    const resize = () => {
      const scale = window.devicePixelRatio;
      canvas.width = Math.round(canvas.clientWidth * scale);
      canvas.height = Math.round(canvas.clientHeight * scale);
      aura.width = Math.ceil(canvas.width / AURA_SHRINK);
      aura.height = Math.ceil(canvas.height / AURA_SHRINK);
      drawnAt = "";
    };
    resize();
    const sizes = new ResizeObserver(() => {
      resize();
      draw(performance.now());
    });
    sizes.observe(canvas);
    const visibility = new IntersectionObserver(([entry]) => {
      onScreen = entry.isIntersecting;
    });
    visibility.observe(canvas);

    function follow(seconds: number) {
      const { sounding, moment } = latest.current;
      const heard = sounding && !calm ? moment() : undefined;
      const target = heard?.bands ?? bandsFromPeak(heard?.peak ?? 0);
      for (let band = 0; band < 3; band++) {
        const ease = target[band] > bands[band] ? ATTACK : RELEASE;
        bands[band] += (target[band] - bands[band]) * ease;
        if (bands[band] < AT_REST && target[band] === 0) bands[band] = 0;
      }
      advancePhases(phases, bands, seconds);
      fillWave(line, phases, bands, seed);
    }

    /** Eases each colour toward its share, so the blend drifts rather than flickers. */
    function followBlend() {
      const parts = latest.current.blend?.() ?? [];
      const total = parts.reduce((sum, part) => sum + part.weight, 0) || 1;
      for (const color of mix.keys()) {
        if (!parts.some((part) => part.color === color)) {
          mix.set(color, (mix.get(color) ?? 0) * (1 - BLEND_EASE));
          if ((mix.get(color) ?? 0) < 0.01) mix.delete(color);
        }
      }
      for (const { color, weight } of parts) {
        const share = mix.get(color) ?? 0;
        mix.set(color, share + (weight / total - share) * BLEND_EASE);
      }
    }

    /** The line's colour: a gradient of the blend over the lit length, or the tint. */
    function colourOf(target: CanvasRenderingContext2D, length: number, tint: string) {
      if (mix.size === 0) return tint;
      const [only] = mix.keys();
      if (mix.size === 1) return only;

      const total = [...mix.values()].reduce((sum, share) => sum + share, 0);
      const gradient = target.createLinearGradient(0, 0, Math.max(1, length), 0);
      let reached = 0;
      for (const [color, share] of mix) {
        gradient.addColorStop(Math.min(1, (reached + share / 2) / total), color);
        reached += share;
      }
      return gradient;
    }

    function trace(
      target: CanvasRenderingContext2D,
      width: number,
      middle: number,
      reach: number,
    ) {
      const xOf = (index: number) => (index / (LINE_POINTS - 1)) * width;
      const yOf = (index: number) => middle - line[index] * reach;

      // Each point pulls the line toward it without being a corner on it.
      target.beginPath();
      target.moveTo(xOf(0), yOf(0));
      for (let index = 1; index < LINE_POINTS - 1; index++) {
        const midX = (xOf(index) + xOf(index + 1)) / 2;
        const midY = (yOf(index) + yOf(index + 1)) / 2;
        target.quadraticCurveTo(xOf(index), yOf(index), midX, midY);
      }
      target.lineTo(xOf(LINE_POINTS - 1), yOf(LINE_POINTS - 1));
    }

    /** The light takes the shape of the sound, lit up to the fader's position. */
    function drawAura(tint: string, value: number, level: number) {
      const width = aura!.width;
      const height = aura!.height;
      auraContext!.clearRect(0, 0, width, height);
      if (level === 0) return;

      auraContext!.save();
      auraContext!.beginPath();
      auraContext!.rect(0, 0, value * width, height);
      auraContext!.clip();
      trace(auraContext!, width, height / 2, height / 2);
      auraContext!.lineJoin = "round";
      auraContext!.lineWidth = height * (large ? 0.12 : 0.28);
      auraContext!.strokeStyle = colourOf(auraContext!, value * width, tint);
      auraContext!.globalAlpha = AURA_STRENGTH * level;
      auraContext!.stroke();
      auraContext!.restore();
    }

    function draw(now: number) {
      follow(lastDrawn === 0 ? 0 : (now - lastDrawn) / 1000);
      followBlend();
      const { value, tint } = latest.current;
      const level = Math.max(...bands);
      const resting = level === 0;
      const key = `${value}/${tint}/${mix.size}`;
      if (resting && key === drawnAt) return;

      const { width, height } = canvas!;
      const scale = window.devicePixelRatio;
      const middle = height / 2;
      const reach = height / 2 - 2 * scale;
      const position = value * width;

      drawAura(tint, value, level);
      if (resting) {
        context!.clearRect(0, 0, width, height);
      } else {
        context!.globalCompositeOperation = "destination-out";
        context!.fillStyle = `rgba(0, 0, 0, ${TRAIL_FADE})`;
        context!.fillRect(0, 0, width, height);
        context!.globalCompositeOperation = "source-over";
      }
      drawnAt = resting ? key : "";
      context!.lineJoin = "round";
      context!.lineCap = "round";

      context!.save();
      context!.beginPath();
      context!.rect(position, 0, width - position, height);
      context!.clip();
      trace(context!, width, middle, reach);
      context!.lineWidth = (large ? 1.5 : 1.25) * scale;
      context!.strokeStyle = faint;
      context!.stroke();
      context!.restore();

      const colour = colourOf(context!, position, tint);
      context!.save();
      context!.beginPath();
      context!.rect(0, 0, position, height);
      context!.clip();
      trace(context!, width, middle, reach);
      context!.strokeStyle = colour;
      context!.lineWidth = (large ? 2 : 1.5) * scale;
      context!.stroke();
      context!.restore();

      // Held inside the track, as half of it would be cut off at either end.
      const cap = (large ? 1.75 : 1.5) * scale;
      const capStart = Math.min(width - cap, Math.max(0, position - cap / 2));
      const lastColour = [...mix.keys()].pop();
      context!.fillStyle = typeof colour === "string" ? colour : (lastColour ?? tint);
      context!.fillRect(capStart, 0, cap, height);
    }

    function loop(now: number) {
      if (onScreen && now - lastDrawn >= FRAME_MS) {
        draw(now);
        lastDrawn = now;
      }
      frame = requestAnimationFrame(loop);
    }

    if (live) frame = requestAnimationFrame(loop);
    else draw(performance.now());

    return () => {
      cancelAnimationFrame(frame);
      sizes.disconnect();
      visibility.disconnect();
    };
  }, [live, seed, large]);

  function positionOf(event: PointerEvent<HTMLDivElement>) {
    const box = event.currentTarget.getBoundingClientRect();
    return (event.clientX - box.left) / box.width;
  }

  function followPointer(event: PointerEvent<HTMLDivElement>) {
    const position = positionOf(event);
    onChange(clamp(position));

    const stage = stageRef.current;
    if (!stage) return;
    const overshoot = position - clamp(position);
    stage.style.transformOrigin = overshoot < 0 ? "right" : "left";
    stage.style.transform = `scaleX(${1 + stretchFor(overshoot)})`;
  }

  function press(event: PointerEvent<HTMLDivElement>) {
    event.currentTarget.setPointerCapture(event.pointerId);
    stageRef.current?.classList.remove("fader-settling");
    followPointer(event);
  }

  function drag(event: PointerEvent<HTMLDivElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) followPointer(event);
  }

  function release() {
    onRelease?.();
    const stage = stageRef.current;
    if (!stage) return;
    stage.classList.add("fader-settling");
    stage.style.transform = "";
  }

  function nudge(event: KeyboardEvent) {
    const moves: Record<string, number> = {
      ArrowRight: value + KEY_STEP,
      ArrowUp: value + KEY_STEP,
      ArrowLeft: value - KEY_STEP,
      ArrowDown: value - KEY_STEP,
      Home: 0,
      End: 1,
    };
    const next = moves[event.key];
    if (next === undefined) return;
    event.preventDefault();
    onChange(clamp(next));
    onRelease?.();
  }

  return (
    <div
      className={`fader${large ? " fader-large" : ""}`}
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value * 100)}
      onPointerDown={press}
      onPointerMove={drag}
      onPointerUp={release}
      onPointerCancel={release}
      onKeyDown={nudge}
    >
      <div className="fader-stage" ref={stageRef}>
        <canvas className="fader-aura" ref={auraRef} aria-hidden="true" />
        <canvas className="fader-line" ref={canvasRef} aria-hidden="true" />
      </div>
    </div>
  );
}
