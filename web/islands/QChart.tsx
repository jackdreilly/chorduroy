import {
  StandardWebSocketClient,
  WebSocketClient,
} from "https://deno.land/x/websocket@v0.1.4/mod.ts";
import { useEffect, useState } from "preact/hooks";
const endpoint = "ws://127.0.0.1:1234";
type Letter =
  | "C"
  | "D"
  | "E"
  | "F"
  | "G"
  | "A"
  | "B";
type Accidental =
  | "Sharp"
  | "Flat"
  | "Natural";
function noteToString({ letter, accidental }: Note): string {
  function letterToString(letter: Letter): string {
    return letter;
  }
  function accidentalToString(accidental?: Accidental): string {
    switch (accidental) {
      case "Flat":
        return "b";
      case "Sharp":
        return "#";
      default:
        return "";
    }
  }
  return [letterToString(letter), accidentalToString(accidental)].join("");
}
interface Note {
  letter: Letter;
  accidental?: Accidental;
}
enum Flavor {
  "Major",
  "Minor",
}
interface Chord {
  chord_type: Flavor;
  root: Note;
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
  const [timeline, setTimeline] = useState<Timeline>([]);
  const [{ chord, bucketed_q, full_q, beat, observations }, setState] =
    useState<
      Payload
    >({
      full_q: { x: [], y: [] },
      bucketed_q: { x: [], y: [] },
      fft: [],
      chord: { chord_type: Flavor.Major, root: { letter: "C" } },
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
  const boost = 2;
  return (
    <>
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
                  {noteToString(full_q.x[i])}
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
                {noteToString(full_q.x[i])}
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
                {noteToString(observations.x[j])}
              </div>
            ))}
            <div class="text-center font-bold flex-1 border-1 border-black h-[10em] rounded text-black">
              {chordString(observations.chords[i])}
            </div>
          </div>
        ))}
      </div>
      <TimelineComponent timeline={timeline} />
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
  return `${noteToString(chord.root)}${
    chord.chord_type.toString() === "Major" ? "" : "m"
  }`;
}
