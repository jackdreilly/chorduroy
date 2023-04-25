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
const positionalNotes = [
  "C",
  "Db",
  "D",
  "Eb",
  "E",
  "F",
  "Gb",
  "G",
  "Ab",
  "A",
  "Bb",
  "B",
];
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
type SoloMode = "Chord" | "Nearest" | "Transpose";
interface Note {
  letter: Letter;
  accidental?: Accidental;
}
type WebInEvent = ({ type: "InferenceEvent" } & Payload) | { type: "Beat" } | {
  type: "MidiEvent";
  note: number;
  mapped_note: number;
  on: boolean;
};
type WebOutEvent = { SoloMode: SoloMode };
enum Flavor {
  "Major",
  "Minor",
}
interface Chord {
  chord_type: Flavor;
  root: Note;
}
type ChordInference = {
  y: number[];
  chord: Chord;
};
type Scale = {
  root: Note;
  mode: "Major" | "Minor";
};
interface Payload {
  scale: Scale;
  chord: Chord;
  chord_inferences: ChordInference[];
}
export default function QChart() {
  const [timeline, setTimeline] = useState<Timeline>([]);
  const [beat, setBeat] = useState<boolean>(false);
  const [notes, setNotes] = useState<number[]>([]);
  const [mode, setMode] = useState<SoloMode>("Chord");
  const [mappedNotes, setMappedNotes] = useState<number[]>([]);
  const [ws, setWs] = useState<WebSocketClient>();
  const [{ chord, chord_inferences, scale }, setChordInferences] = useState<
    Payload
  >({
    chord: { chord_type: Flavor.Major, root: { letter: "C" } },
    chord_inferences: [],
    scale: { root: { letter: "C" }, mode: "Major" },
  });
  useEffect(() => {
    const ws: WebSocketClient = new StandardWebSocketClient(endpoint);
    ws.on("message", (m) => {
      const event: WebInEvent = JSON.parse(m.data);
      switch (event.type) {
        case "Beat":
          setBeat(true);
          setTimeout(() => setBeat((b) => !b), 100);
          return;
        case "MidiEvent":
          switch (event.on) {
            case true:
              setNotes((n) => [...n, event.note]);
              setMappedNotes((n) => [...n, event.mapped_note]);
              break;
            case false:
              setNotes((n) => n.filter((x) => x !== event.note));
              setMappedNotes((n) => n.filter((x) => x !== event.mapped_note));
              break;
          }
          return;
        case "InferenceEvent":
          setChordInferences(event);
      }
    });
    setWs(ws);
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
  const sliced = chord_inferences.slice(
    Math.max(0, chord_inferences.length - 10),
  );
  sliced.reverse();
  return (
    <div>
      <div class="flex items-center">
        <div class="w-4 m-2">
          {beat && <span class="flex w-3 h-3 bg-red-500 rounded-full"></span>}
        </div>
        <div class="flex flex-col items-center rounded shadow m-2 p-2">
          <div class="text-xs">Scale</div>
          <div class="font-bold">
            {noteToString(scale.root)}
          </div>
        </div>
      </div>

      <h3 class="mb-4 font-semibold text-gray-900 dark:text-white">
        Solo Mode
      </h3>
      <ul class="items-center w-full text-sm font-medium text-gray-900 bg-white border border-gray-200 rounded-lg sm:flex dark:bg-gray-700 dark:border-gray-600 dark:text-white">
        {(["Chord", "Nearest", "Transpose"] as SoloMode[]).map((thisMode) => (
          <li class="w-full border-b border-gray-200 sm:border-b-0 sm:border-r dark:border-gray-600">
            <div class="flex items-center pl-3">
              <input
                id="horizontal-list-radio-license"
                type="radio"
                checked={mode === thisMode}
                onChange={() => {
                  ws?.send(
                    JSON.stringify({ SoloMode: thisMode }),
                  );
                  return setMode(thisMode);
                }}
                name="list-radio"
                class="w-4 h-4 text-blue-600 bg-gray-100 border-gray-300 focus:ring-blue-500 dark:focus:ring-blue-600 dark:ring-offset-gray-700 dark:focus:ring-offset-gray-700 focus:ring-2 dark:bg-gray-600 dark:border-gray-500"
              />
              <label
                for="horizontal-list-radio-license"
                class="w-full py-3 ml-2 text-sm font-medium text-gray-900 dark:text-gray-300"
              >
                {thisMode}
              </label>
            </div>
          </li>
        ))}
      </ul>

      <TimelineComponent timeline={timeline} />
      <div>
        <div class="shadow rounded m-2 p-2">
          <Keyboard notes={notes} />
        </div>
        <div class="shadow rounded m-2 p-2">
          <Keyboard notes={mappedNotes} />
        </div>
      </div>
      <div
        class="grid w-full rounded-lg shadow-lg p-2 text-sm font-mono text-center"
        style={{
          gridTemplateColumns: `repeat(${sliced.length}, minmax(0, 1fr))`,
        }}
      >
        {sliced.map((
          { y, chord },
          i,
        ) => (
          <div
            class="grid"
            style={{
              gridTemplateRows: `repeat(13, minmax(0, 1fr))`,
            }}
          >
            {y.map((v, j) => (
              <div style={{ backgroundColor: colorize(v, Math.max(...y)) }}>
                {positionalNotes[j]}
              </div>
            ))}
            <div class="font-bold">{chordString(chord)}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
type Timeline = { chord: Chord; time: number }[];
function colorize(
  value: number,
  norm: number,
): string {
  value /= norm;
  return `hsl(${380 * (.5 + .5 * value)},${50 + 50 * value}%, ${
    100 - 50 * value
  }%)`;
}

function TimelineComponent({ timeline }: { timeline: Timeline }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 10);
    return () => clearInterval(interval);
  }, []);
  return (
    <div class="m-2 overflow-hidden h-10 rounded shadow bg-gray-300">
      <ul class="relative">
        {timeline.map(({ chord, time }, i) => (
          <li
            class="font-bold font-mono absolute m-1 p-1 bg-white rounded-md shadow-md"
            style={{ left: (now - time) / 10, z: i }}
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

const octaves = 5;

function Keyboard({ notes }: { notes: number[] }) {
  return (
    <div class="flex flex-col h-16">
      <div class="z-1 flex flex-row" style={{ flex: 1 }}>
        {intRange(octaves * 5).map((i) => (
          <div
            class={"border border-black"}
            style={{
              flex: i % 5 < 2 ? 3 / 7 / 2 : 4 / 7 / 3,
              backgroundColor: notes.includes(
                  12 * (3 + Math.floor(i / 5)) + [1, 3, 6, 8, 10][i % 5],
                )
                ? "gray"
                : "white",
            }}
          >
          </div>
        ))}
      </div>
      <div class="z-1 flex flex-row" style={{ flex: 1.5 }}>
        {intRange(octaves * 7).map((i) => (
          <div
            class="flex-1 border border-black bottom-0 relative"
            style={{
              backgroundColor: notes.includes(
                  12 * (3 + Math.floor(i / 7)) + [0, 2, 4, 5, 7, 9, 11][i % 7],
                )
                ? "gray"
                : "white",
            }}
          >
            {i % 7
              ? null
              : (
                <div class="text-center absolute text-xs m-1 left-0 right-0 bottom-0">
                  {Math.floor(i / 7) + 1}
                </div>
              )}
          </div>
        ))}
      </div>
    </div>
  );
}

function intRange(
  start: number,
  end?: number,
): number[] {
  if (end === undefined) {
    end = start;
    start = 0;
  }
  return Array.from({ length: end - start }, (_, i) => i + start);
}
