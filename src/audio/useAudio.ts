import { useCallback, useEffect, useRef, useState } from "react";
import { act, getTab, onSourceChanged, startLive, stopLive } from "../api";
import type { Bands } from "./signal";

const AUDIO = "audio";

/** How long a fader holds its own value after release, while Windows catches up. */
const SETTLE_MS = 300;

export type Flow = "output" | "input";

export interface AudioDevice {
  id: string;
  name: string;
}

export interface Endpoint {
  devices: AudioDevice[];
  current: string | null;
  volume: number;
  muted: boolean;
}

export interface AudioApp {
  id: string;
  name: string;
  icon: string | null;
  volume: number;
  muted: boolean;
  playing: boolean;
}

export interface Track {
  title: string;
  artist: string;
  /** The program playing it, in lower case and without its extension. */
  app: string;
  art: string | null;
  playing: boolean;
  position: number;
  duration: number;
}

export interface AudioState {
  output: Endpoint;
  input: Endpoint;
  apps: AudioApp[];
  track: Track | null;
}

/** One moment of sound: its bands where Windows shares them, and always its level. */
export interface Moment {
  bands: Bands | null;
  peak: number;
}

interface LiveFrame extends Moment {
  input: number;
  apps: (Moment & { id: string })[];
}

const SILENCE: LiveFrame = { bands: null, peak: 0, input: 0, apps: [] };

type AudioAction =
  | { action: "set_device"; id: string }
  | { action: "set_volume"; flow: Flow; value: number }
  | { action: "set_muted"; flow: Flow; muted: boolean }
  | { action: "set_app_volume"; app: string; value: number }
  | { action: "set_app_muted"; app: string; muted: boolean }
  | { action: "transport"; command: "previous" | "toggle" | "next" };

/** The sound on this computer, kept current by Windows through the source. */
export function useAudio(onLoaded: () => void) {
  const [audio, setAudio] = useState<AudioState | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    const load = () =>
      getTab(AUDIO)
        .then((tab) => {
          setAudio(tab.state as AudioState | null);
          setFailure(tab.error?.message ?? null);
        })
        .catch((error) => setFailure(String(error)))
        .finally(onLoaded);

    load();
    const stop = onSourceChanged((changed) => {
      if (changed === AUDIO) load();
    });
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, [onLoaded]);

  const perform = useCallback(
    (action: AudioAction) =>
      act(AUDIO, action)
        .then(() => setFailure(null))
        .catch((error) => setFailure(String(error))),
    [],
  );

  return { audio, failure, perform };
}

/**
 * Keeps the newest moment of sound while the tab is on screen. It lives outside
 * React state, because faders read it as they draw and nothing else needs it.
 */
export function useLiveFrame(live: boolean) {
  const frame = useRef(SILENCE);

  useEffect(() => {
    if (!live) return;
    startLive<LiveFrame>(AUDIO, (moment) => {
      frame.current = moment;
    });
    return () => {
      frame.current = SILENCE;
      stopLive(AUDIO);
    };
  }, [live]);

  return frame;
}

/**
 * Gives a fader to the person holding it. While dragging, the value on screen is
 * theirs, so echoes from Windows can't pull it back, and only the newest position
 * is sent, so a fast drag never queues up behind a slow device.
 */
export function useHeldValue(actual: number, send: (value: number) => Promise<unknown>) {
  const [held, setHeld] = useState<number | null>(null);
  const next = useRef<number | null>(null);
  const sending = useRef(false);
  const settling = useRef<ReturnType<typeof setTimeout>>(undefined);

  async function flush() {
    if (sending.current) return;
    sending.current = true;
    while (next.current !== null) {
      const value = next.current;
      next.current = null;
      await send(value);
    }
    sending.current = false;
  }

  function change(value: number) {
    clearTimeout(settling.current);
    setHeld(value);
    next.current = value;
    flush();
  }

  function release() {
    clearTimeout(settling.current);
    settling.current = setTimeout(() => setHeld(null), SETTLE_MS);
  }

  return { value: held ?? actual, change, release };
}

/** The least lightness a colour is given, so dark images still glow on black. */
const TINT_MIN_LIGHTNESS = 62;

/** How colourful a pixel must be to count. Below this an image has no colour of its own. */
const MIN_VIVIDNESS = 0.12;

/** Finds the most vivid colour in an image, skipping anything see-through. */
function strongestColour(image: HTMLImageElement): string | null {
  const size = 12;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  context.drawImage(image, 0, 0, size, size);
  const { data } = context.getImageData(0, 0, size, size);

  let best: [number, number, number] | null = null;
  let bestVividness = MIN_VIVIDNESS;
  for (let index = 0; index < data.length; index += 4) {
    if (data[index + 3] < 128) continue;
    const [red, green, blue] = [data[index], data[index + 1], data[index + 2]];
    const vividness = (Math.max(red, green, blue) - Math.min(red, green, blue)) / 255;
    if (vividness > bestVividness) {
      bestVividness = vividness;
      best = [red, green, blue];
    }
  }
  if (!best) return null;

  const [red, green, blue] = best.map((channel) => channel / 255);
  const high = Math.max(red, green, blue);
  const low = Math.min(red, green, blue);
  const lightness = (high + low) / 2;
  const chroma = high - low;
  const hue =
    high === red
      ? ((green - blue) / chroma + 6) % 6
      : high === green
        ? (blue - red) / chroma + 2
        : (red - green) / chroma + 4;
  const saturation = chroma / (1 - Math.abs(2 * lightness - 1));
  const shownLightness = Math.max(lightness * 100, TINT_MIN_LIGHTNESS);
  return `hsl(${hue * 60} ${Math.round(saturation * 100)}% ${shownLightness}%)`;
}

/** Each image's colour, by its URL. Images are read once and remembered. */
const knownTints = new Map<string, string | null>();

/** The colours of a set of images, filling in as each one is read. */
export function useImageTints(images: (string | null | undefined)[]) {
  const [, setRead] = useState(0);
  const key = images.join("|");

  useEffect(() => {
    let current = true;
    for (const url of images) {
      if (!url || knownTints.has(url)) continue;
      const image = new Image();
      image.onload = () => {
        knownTints.set(url, strongestColour(image));
        if (current) setRead((count) => count + 1);
      };
      image.src = url;
    }
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return (url: string | null | undefined) => (url ? (knownTints.get(url) ?? null) : null);
}
