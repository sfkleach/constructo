// The functions that the adoption file names. They are the program's own code. A function refuses a value by throwing
// `Refused`; any other error is a fault, and passes through the codec as it is.

import { Refused } from "./constructo-runtime/index.ts";
import * as paint from "./paint.ts";
import type { Colour$Constructors } from "./shapes/index.ts";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

export function uuidFromString(text: string): string {
    if (!UUID.test(text)) {
        throw new Refused(`${JSON.stringify(text)} is not a UUID`);
    }
    return text;
}

export function uuidToString(id: string): string {
    return uuidFromString(id);
}

const COLOURS: [Colour$Constructors, paint.Colour][] = [
    ["Red", paint.RED],
    ["Green", paint.GREEN],
    ["Blue", paint.BLUE],
];

export function colourFromGenerated(colour: Colour$Constructors): paint.Colour {
    const found = COLOURS.find(([name]) => name === colour);
    if (found === undefined) {
        throw new Refused(`${colour} is not a colour`);
    }
    return found[1];
}

export function colourToGenerated(colour: paint.Colour): Colour$Constructors {
    const found = COLOURS.find(([, own]) => own.rgb === colour.rgb);
    if (found === undefined) {
        throw new Refused(`The colour #${colour.rgb.toString(16).padStart(6, "0")} has no Constructo form`);
    }
    return found[0];
}
