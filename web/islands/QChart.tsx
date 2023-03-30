import {
  StandardWebSocketClient,
  WebSocketClient,
} from "https://deno.land/x/websocket@v0.1.4/mod.ts";
import { useCallback, useEffect, useMemo, useState } from "preact/hooks";
const endpoint = "ws://127.0.0.1:1234";
enum Note {
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
}
// Create array of notes
const notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
  .map((n) => Note[n as keyof typeof Note]);
enum Flavor {
  "Major",
  "Minor",
}
interface Chord {
  flavor: Flavor;
  note: Note;
}
type Observations = {
  x: Note[];
  y: number[][];
  chords: Chord[];
};
type FullQ = {
  x: Note[];
  y: number[][];
};
interface Payload {
  full_q: FullQ;
  bucketed_q: {
    x: Note[];
    y: number[];
  };
  chord: Chord;
  fft: number[];
  beat: boolean;
  observations: Observations;
}
export default function QChart() {
  const [zoomRaw, setZoom] = useState(1);
  const [boost, setBoost] = useState(5);
  const [auto, setAuto] = useState(false);
  const [timeline, setTimeline] = useState<Timeline>([]);
  const [{ chord, bucketed_q, full_q, fft, beat, observations }, setState] =
    useState<
      Payload
    >({
      full_q: { x: [], y: [] },
      bucketed_q: { x: [], y: [] },
      fft: [],
      chord: { flavor: Flavor.Major, note: Note.C },
      beat: false,
      observations: { x: [], y: [], chords: [] },
    });
  useEffect(() => {
    const ws: WebSocketClient = new StandardWebSocketClient(endpoint);
    ws.on("message", (m) => {
      const state = JSON.parse(m.data);
      setState(state);
    });
  }, []);
  useEffect(() => {
    if (
      JSON.stringify(timeline[timeline.length - 1]?.chord) ===
        JSON.stringify(chord)
    ) {
      return;
    }
    setTimeline((t) => [...t, { chord, time: Date.now() }]);
  }, [timeline, chord]);
  const zoom = Math.min(zoomRaw, 10000 / fft.length);
  return (
    <>
      <div>
        <input
          class="m-10"
          type="range"
          min={0.1}
          step={0.1}
          max={15}
          value={boost}
          onInput={({ target: { valueAsNumber } }) => {
            setBoost((_) => valueAsNumber);
          }}
        />
        <label>Booster</label>
      </div>
      {
        <div
          class="h-4 w-4 rounded-full shadow-md border-1 border-black"
          style={{ backgroundColor: beat ? "red" : "white" }}
        >
        </div>
      }
      <div>
        <input
          class="m-10"
          type="checkbox"
          checked={auto}
          onInput={() => setAuto((c) => !c)}
        />
        <label>Auto</label>
      </div>
      <div class="flex flex-row w-full m-10">
        {full_q.y.map((v, i) => (
          <div class="flex flex-1 flex-col">
            {(() => {
              const value = bucketed_q.y[i] / Math.max(...bucketed_q.y);
              return (
                <div
                  class="text-center flex-1 border-1 border-black h-[10em] rounded text-black"
                  style={{
                    backgroundColor: `hsl(${380 * (.5 + .5 * value)},${
                      50 + 50 * value
                    }%, ${100 - 50 * value}%)`,
                  }}
                >
                  {full_q.x[i]}
                </div>
              );
            })()}
            {v.map((x) => Math.min(1, Math.max(0, x * boost))).map((
              value,
            ) => (
              <div
                class="text-center flex-1 border-1 border-black h-[10em] rounded text-black"
                style={{
                  backgroundColor: `hsl(${380 * (.5 + .5 * value)},${
                    50 + 50 * value
                  }%, ${100 - 50 * value}%)`,
                }}
              >
                {full_q.x[i]}
              </div>
            ))}
          </div>
        ))}
      </div>
      <h2 class="m-2 p-2 text-lg">Observations</h2>
      <div class="flex flex-row w-full m-10">
        {observations.y.map((v, i) => (
          <div class="flex flex-1 flex-col">
            {v.map((x) => Math.min(1, Math.max(0, x * boost))).map((
              value,
              j,
            ) => (
              <div
                class="text-center flex-1 border-1 border-black h-[10em] rounded text-black"
                style={{
                  backgroundColor: `hsl(${380 * (.5 + .5 * value)},${
                    50 + 50 * value
                  }%, ${100 - 50 * value}%)`,
                }}
              >
                {observations.x[j]}
              </div>
            ))}
            <div class="text-center flex-1 border-1 border-black h-[10em] rounded text-black">
              {chordString(observations.chords[i])}
            </div>
          </div>
        ))}
      </div>
      <TimelineComponent timeline={timeline} />
      <div class="flex">
        <div class="m-4">{Math.max(...fft)}</div>
        <div class="m-4">{fft.length}</div>
      </div>
      <div>
        <input
          class="m-10"
          type="range"
          min={0}
          step={0.01}
          max={1}
          value={zoomRaw}
          onInput={({ target: { valueAsNumber } }) => {
            setZoom((_) => valueAsNumber);
          }}
        />
        <label>Zoom</label>
      </div>
      <div>
        <svg
          class="w-full h-[10em]"
          viewBox={`0 0 200 50`}
        >
          <polyline
            transform={`scale(1,-1) translate(0,-50)`}
            points={fft.slice(0, Math.round(fft.length * zoom)).map((v, i) =>
              `${
                i / fft.slice(0, Math.round(fft.length * zoom)).length * 200
              } ${v / Math.max(...fft) * 50}`
            ).join(" ")}
            fill="none"
            stroke="black"
          />
        </svg>
      </div>
    </>
  );
}
type Timeline = { chord: Chord; time: number }[];
function TimelineComponent({ timeline }: { timeline: Timeline }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 10);
    return () => clearInterval(interval);
  }, []);
  return (
    <div class="m-2 overflow-hidden w-full h-16 p-2 rounded-lg shadow-lg bg-gray-500">
      <ul class="relative">
        {timeline.map(({ chord, time }, i) => (
          <li
            class="font-bold font-mono absolute m-1 p-1 bg-white rounded-md shadow-md"
            style={{ right: (now - time) / 10, z: i }}
          >
            {chordString(chord)}
          </li>
        ))}
      </ul>
    </div>
  );
}

function chordString(
  chord: Chord,
): import("https://esm.sh/v113/preact@10.11.0/src/index").ComponentChildren {
  return `${chord.note}${chord.flavor.toString() === "Major" ? "" : "m"}`;
}
