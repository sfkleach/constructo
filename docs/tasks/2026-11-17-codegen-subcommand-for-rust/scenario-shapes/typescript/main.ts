// The program of the scenario: it reads a drawing, writes it back, and shows what is refused.

import { deepStrictEqual } from "node:assert";
import { readFileSync, writeFileSync } from "node:fs";

import * as rt from "./constructo-runtime/index.ts";
import * as paint from "./paint.ts";
import * as shapes from "./shapes/index.ts";
import { Drawing } from "./shapes/index.ts";
import { Uuid } from "./shapes/com_example_ids.ts";

const ID = 'ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8")';

function styled(item: string): string {
    return `Styled { item: ${item}, colour: null, offset: { dx: 0, dy: 0 } }`;
}

function attempt(label: string, f: () => unknown): void {
    try {
        console.log(`${label}: ACCEPTED ${JSON.stringify(f())}`);
    } catch (error) {
        if (!(error instanceof rt.CodecError)) {
            throw error;
        }
        console.log(`${label}: ${error.toString()}`);
    }
}

const [, , input, output] = process.argv;
if (input === undefined) {
    throw new Error("the path of a values file, and of a file to write to if wanted");
}
const codec = shapes.codec();

const drawing: Drawing = codec.read(Drawing, readFileSync(input, "utf8"));
console.log(`--- Read\n${JSON.stringify(drawing)}`);

const written = codec.write(Drawing, drawing);
console.log(`--- Written\n${written}`);
if (output !== undefined) {
    writeFileSync(output, written);
}

deepStrictEqual(codec.read(Drawing, written), drawing);
console.log("--- Read back: the same");

const other = 'com.example.ids::Uuid("00000000-0000-0000-0000-000000000001")';
const ids: string[] = codec.read(rt.array(Uuid), `[${ID}, ${other}]`);
console.log(`--- An array of an adopted type\n${JSON.stringify(ids)}`);

console.log("--- Refused when read");
const refused: [string, string][] = [
    ["a constructor of another type", `Drawing(${ID}, "T", [${styled("Point(1.0, 2.0)")}])`],
    ["a name that is not in the run", `Drawing(${ID}, "T", [${styled("Hexagon")}])`],
    ["a bare name that the project does not have", 'Drawing(Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"), "T")'],
    ["a string that is not a UUID", 'Drawing(ids::Uuid("not-a-uuid"), "T")'],
    ["the id left out", 'Drawing(title: "T")'],
    ["a Nat larger than 2^53 - 1", `Drawing(${ID}, "T", [], 9007199254740993)`],
    ["a Float that does not fit number", `Drawing(${ID}, "T", [${styled("Circle { radius: 1.0e400 }")}])`],
    ["an unknown const", `Drawing(${ID}, "T", [${styled("unit_square")}])`],
    ["a missing field", `Drawing(${ID}, "T", [Styled { item: Dot, colour: null, offset: { dx: 0 } }])`],
    ["a syntax error", `Drawing(${ID}, "T"`],
];
for (const [label, text] of refused) {
    attempt(label, () => codec.read(Drawing, text));
}

const huge = structuredClone(drawing);
huge.shapes = [
    { item: { $kind: "Circle", radius: 1e300 }, colour: null, offset: { dx: 0, dy: 0 } },
    { item: { $kind: "Circle", radius: 1e-7 }, colour: null, offset: { dx: 0, dy: 0 } },
    { item: { $kind: "Circle", radius: 2 }, colour: null, offset: { dx: 0, dy: 0 } },
];
deepStrictEqual(codec.read(Drawing, codec.write(Drawing, huge)), huge);
console.log("--- Large, small and whole floats written and read back");

console.log("--- Refused when written");
const bad: [string, (d: Drawing) => void][] = [
    ["a custom colour", (d) => (d.shapes[0]!.colour = new paint.Colour(0x336699))],
    ["a fraction for a Nat", (d) => (d.layer = 1.5)],
    ["a negative Nat", (d) => (d.layer = -1)],
    ["a Nat larger than 2^53 - 1", (d) => (d.layer = 2 ** 60)],
    ["a NaN", (d) => (d.shapes[0]!.item = { $kind: "Circle", radius: NaN })],
    ["a string from untyped code", (d) => ((d as { title: unknown }).title = 5)],
];
for (const [label, change] of bad) {
    const changed = codec.read(Drawing, written);
    change(changed);
    attempt(label, () => codec.write(Drawing, changed));
}

// What the type checker refuses, before the program runs. `tsc` fails if a line marked here has no error.
function typeErrors(): void {
    // @ts-expect-error A number is not a Drawing.
    codec.write(Drawing, 5);
    // @ts-expect-error A Shape needs its $kind.
    const s: shapes.Shape = { radius: 1 };
    // @ts-expect-error Styled needs the type of its item.
    codec.read(shapes.Styled, "");
    void s;
}
void typeErrors;
