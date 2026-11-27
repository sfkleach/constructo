// A stand-in for a TypeScript `constructo-runtime`: the syntax layer, the tree of a value, and the helpers that need no
// generated type.
//
// The real one would parse and print Constructo text itself. This one asks the `constructo` binary to do it, through
// the json-exact notation, which keeps every kind of value but not the positions in the text. So an error of reading
// says where it is by a path in the value, as an error of writing does.

import { spawnSync } from "node:child_process";

const BINARY = process.env["CONSTRUCTO"] ?? "constructo";

// Errors.

export class CodecError extends Error {
    path: (string | number)[] = [];
    detail: string;

    constructor(detail: string) {
        super(detail);
        this.detail = detail;
    }

    override toString(): string {
        let text = "";
        for (const step of this.path) {
            text += typeof step === "number" ? `[${step}]` : text ? `.${step}` : step;
        }
        return text ? `${text}: ${this.detail}` : this.detail;
    }
}

/** Adds a step to the path of an error that passes out of `f`. */
export function at<R>(step: string | number, f: () => R): R {
    try {
        return f();
    } catch (error) {
        if (error instanceof CodecError) {
            error.path.unshift(step);
        }
        throw error;
    }
}

// The tree of a value, which is public: a program can read and write Constructo with it and no generated code.

export type Arg = { name: string | null; value: Value };
export type Member = { name: string; value: Value };

export type Value =
    | { kind: "nat" | "int" | "float"; text: string }
    | { kind: "string"; text: string }
    | { kind: "array"; items: Value[] }
    | { kind: "record"; members: Member[] }
    | { kind: "tagged"; name: string; body: "none" }
    | { kind: "tagged"; name: string; body: "record"; members: Member[] }
    | { kind: "tagged"; name: string; body: "tuple"; args: Arg[] }
    | { kind: "tagged"; name: string; body: "array"; items: Value[] };

export type Tagged = Extract<Value, { kind: "tagged" }>;

// What a generated codec gives the readers and writers: its names, its consts and its limits.

export interface Context {
    /** The full name of a constructor, from any form that the project can write. */
    fullName(name: string): string;
    /** The form in which the project writes a full name. */
    written(fullName: string): string;
    /** Reads the value of a const with `read`, where the text refers to it. An error in it says which const it is
     * in, and a chain of consts that refer to consts is limited in depth. */
    readConst<T>(name: string, read: (value: Value) => T): T;
}

/** How a codec reads, writes and defaults one type. The generated code has one for each of its types. */
export interface Type<T> {
    read(c: Context, value: Value): T;
    write(c: Context, x: T): Value;
    default(c: Context): T;
}

/** Reads a value as `type`, through the value of a const if the value refers to one. Every reader of a generated
 * codec reads the values inside it this way, so a const is resolved where it is met, and not in a pass of its own. */
export function read<T>(c: Context, type: Type<T>, value: Value): T {
    if (value.kind === "tagged" && value.body === "none" && isConstName(value.name)) {
        return c.readConst(value.name, (v) => read(c, type, v));
    }
    return type.read(c, value);
}

export function bare(name: string): string {
    const at = name.lastIndexOf("::");
    return at < 0 ? name : name.slice(at + 2);
}

export function isConstName(name: string): boolean {
    const local = bare(name);
    return /^[a-z_]/.test(local);
}

export function found(value: Value): string {
    switch (value.kind) {
        case "nat":
        case "int":
        case "float":
            return `the number ${value.text}`;
        case "string":
            return "a string";
        case "array":
            return "an array";
        case "record":
            return "a record";
        case "tagged":
            return bare(value.name);
    }
}

export function expected(what: string, value: Value): CodecError {
    return new CodecError(`Expected ${what} but found ${found(value)}`);
}

export function takes(name: string, body: string): CodecError {
    return new CodecError(`Constructor ${bare(name)} takes ${body}`);
}

/** The fields of a record, in the order of `names`. Every field is required, and no other is allowed. */
export function fields<const N extends readonly string[]>(
    members: Member[],
    names: N,
    owner: string,
): { [K in keyof N]: Value } {
    const slots = new Map<string, Value>();
    for (const { name, value } of members) {
        if (!names.includes(name)) {
            throw new CodecError(`Unknown field ${name} in ${bare(owner)}`);
        }
        if (slots.has(name)) {
            throw new CodecError(`Duplicate field ${name}`);
        }
        slots.set(name, value);
    }
    const out = names.map((name) => {
        const value = slots.get(name);
        if (value === undefined) {
            throw new CodecError(`Missing field ${name} in ${bare(owner)}`);
        }
        return value;
    });
    return out as { [K in keyof N]: Value };
}

/** The arguments of a tuple constructor, in the order of its parameters. A parameter with a name may be left out, and
 * is then `undefined`; one without a name may not. */
export function args<const P extends readonly (string | null)[]>(
    given: Arg[],
    params: P,
    owner: string,
): { [K in keyof P]: Value | undefined } {
    const slots: (Value | undefined)[] = params.map(() => undefined);
    const count = `Constructor ${bare(owner)} takes ${params.length} value(s) but has ${given.length}`;
    let position = 0;
    for (const { name, value } of given) {
        const i = name === null ? position++ : params.indexOf(name);
        if (name !== null && i < 0) {
            throw new CodecError(`Unknown argument ${name} in ${bare(owner)}`);
        }
        if (i >= params.length) {
            throw new CodecError(count);
        }
        if (slots[i] !== undefined) {
            throw new CodecError(`Argument ${params[i]} is given more than once in ${bare(owner)}`);
        }
        slots[i] = value;
    }
    params.forEach((param, i) => {
        if (param === null && slots[i] === undefined) {
            throw new CodecError(count);
        }
    });
    return slots as { [K in keyof P]: Value | undefined };
}

/** An argument, or the default of its type if it was left out (decision 18). */
export function argument<T>(c: Context, type: Type<T>, value: Value | undefined, param: string, owner: string): T {
    return at(param, () => {
        if (value !== undefined) {
            return read(c, type, value);
        }
        try {
            return type.default(c);
        } catch (error) {
            if (error instanceof CodecError) {
                const why = error.toString();
                const message = `Argument ${param} of ${bare(owner)} is left out, and its type has no default`;
                throw new CodecError(`${message}: ${why}`);
            }
            throw error;
        }
    });
}

// Adoption.

/** What an adopted function throws to refuse a value. Any other error is a fault in the program, and passes through
 * the codec as it is. */
export class Refused extends Error {}

/** Calls an adopted function, and makes its refusal an error of the codec. */
export function adopted<R>(f: () => R): R {
    try {
        return f();
    } catch (error) {
        if (error instanceof Refused) {
            throw new CodecError(error.message);
        }
        throw error;
    }
}

/** The one value of an argument that has no name, which `args` has made sure is given. */
export function given(value: Value | undefined): Value {
    if (value === undefined) {
        throw new CodecError("An argument without a name is missing");
    }
    return value;
}

// The basic types and those of the prelude. They are here and not in the generated code, since TypeScript has no
// orphan rule, and the names that Bool and Option need come from the codec.

function mismatch(what: string, x: unknown): CodecError {
    return new CodecError(`Expected ${what} but found ${typeof x} ${String(x)}`);
}

/** A Nat is a `number`, so one that is larger than 2^53 - 1 does not fit, and is refused, as a `u64` refuses one
 * that is larger than 2^64 - 1. */
export const nat: Type<number> = {
    read(_c, v) {
        if (v.kind === "nat") {
            const n = Number(v.text);
            if (Number.isSafeInteger(n)) {
                return n;
            }
        }
        if (v.kind === "nat" || v.kind === "int" || v.kind === "float") {
            throw new CodecError(`The number ${v.text} does not fit Nat`);
        }
        throw expected("Nat", v);
    },
    write(_c, x) {
        if (typeof x !== "number" || !Number.isSafeInteger(x) || x < 0) {
            throw mismatch("a Nat", x);
        }
        return { kind: "nat", text: String(x) };
    },
    default: () => 0,
};

export const int: Type<number> = {
    read(_c, v) {
        if (v.kind === "nat" || v.kind === "int") {
            const n = Number(v.text);
            if (Number.isSafeInteger(n)) {
                return n;
            }
        }
        if (v.kind === "nat" || v.kind === "int" || v.kind === "float") {
            throw new CodecError(`The number ${v.text} does not fit Int`);
        }
        throw expected("Int", v);
    },
    write(_c, x) {
        if (typeof x !== "number" || !Number.isSafeInteger(x)) {
            throw mismatch("an Int", x);
        }
        return { kind: "int", text: x < 0 ? String(x) : `+${x}` };
    },
    default: () => 0,
};

export const float: Type<number> = {
    read(_c, v) {
        if (v.kind === "nat" || v.kind === "int" || v.kind === "float") {
            const f = Number(v.text);
            if (Number.isFinite(f)) {
                return f;
            }
            throw new CodecError(`The number ${v.text} does not fit Float`);
        }
        throw expected("Float", v);
    },
    write(_c, x) {
        if (typeof x !== "number") {
            throw mismatch("a Float", x);
        }
        if (!Number.isFinite(x)) {
            throw new CodecError(`The Float ${x} cannot be written`);
        }
        // JavaScript writes `1` and `1e+300`, and a Constructo Float needs a point, as in `1.0` and `1.0e+300`.
        const text = String(x);
        const [mantissa = text, exponent] = text.split("e");
        const point = mantissa.includes(".") ? mantissa : `${mantissa}.0`;
        return { kind: "float", text: exponent === undefined ? point : `${point}e${exponent}` };
    },
    default: () => 0,
};

export const string: Type<string> = {
    read(_c, v) {
        if (v.kind === "string") {
            return v.text;
        }
        throw expected("String", v);
    },
    write(_c, x) {
        if (typeof x !== "string") {
            throw mismatch("a String", x);
        }
        return { kind: "string", text: x };
    },
    default: () => "",
};

export const bool: Type<boolean> = {
    read(c, v) {
        if (v.kind === "tagged" && v.body === "none") {
            const name = c.fullName(v.name);
            if (name === "constructo.prelude::True") {
                return true;
            }
            if (name === "constructo.prelude::False") {
                return false;
            }
        }
        throw expected("Bool", v);
    },
    write(c, x) {
        if (typeof x !== "boolean") {
            throw mismatch("a Bool", x);
        }
        return unit(c.written(x ? "constructo.prelude::True" : "constructo.prelude::False"));
    },
    default: () => false,
};

export function array<T>(item: Type<T>): Type<T[]> {
    return {
        read(c, v) {
            if (v.kind !== "array") {
                throw expected("an array", v);
            }
            return v.items.map((x, i) => at(i, () => read(c, item, x)));
        },
        write(c, x) {
            if (!Array.isArray(x)) {
                throw mismatch("an array", x);
            }
            return { kind: "array", items: x.map((y, i) => at(i, () => item.write(c, y))) };
        },
        default: () => [],
    };
}

/** `Option<T>` is `T | null`, which is exact where no value of `T` is `null`. The generator uses it only there, and
 * boxes the others, as for Python. */
export function option<T>(item: Type<T>): Type<T | null> {
    return {
        read(c, v) {
            if (v.kind === "tagged") {
                const name = c.fullName(v.name);
                if (name === "constructo.prelude::None") {
                    if (v.body !== "none") {
                        throw takes(name, "no fields");
                    }
                    return null;
                }
                if (name === "constructo.prelude::Some") {
                    if (v.body !== "tuple") {
                        throw takes(name, "a tuple");
                    }
                    const [x] = args(v.args, [null], name);
                    return read(c, item, given(x));
                }
            }
            throw expected("Option", v);
        },
        write(c, x) {
            if (x === null) {
                return unit(c.written("constructo.prelude::None"));
            }
            return taggedTuple(c.written("constructo.prelude::Some"), [{ name: null, value: item.write(c, x) }]);
        },
        default: () => null,
    };
}

// Text, through the binary.

function run(argv: string[], input: string): string {
    const done = spawnSync(BINARY, [...argv, "--stdin"], { input, encoding: "utf8" });
    if (done.status !== 0) {
        throw new CodecError(done.stderr.trim().replace(/^ERROR: /, ""));
    }
    return done.stdout;
}

export function parse(text: string): Value[] {
    const items = JSON.parse(run(["convert", "--to=json-exact", "--to-array"], text)) as unknown[];
    return items.map(fromExact);
}

export function parseOne(text: string): Value {
    const values = parse(text);
    const [only] = values;
    if (values.length !== 1 || only === undefined) {
        throw new CodecError(`Expected one value but found ${values.length}`);
    }
    return only;
}

export function render(value: Value): string {
    return run(["convert", "--from=json-exact"], JSON.stringify(toExact(value)));
}

type Exact = { [key: string]: unknown };

function fromExact(input: unknown): Value {
    const j = input as Exact;
    for (const kind of ["nat", "int", "float"] as const) {
        if (kind in j) {
            return { kind, text: j[kind] as string };
        }
    }
    if ("string" in j) {
        return { kind: "string", text: j["string"] as string };
    }
    if ("const" in j) {
        return { kind: "tagged", name: j["const"] as string, body: "none" };
    }
    const members = (o: unknown): Member[] =>
        Object.entries(o as Exact).map(([name, v]) => ({ name, value: fromExact(v) }));
    const items = (a: unknown): Value[] => (a as unknown[]).map(fromExact);
    if ("constructor" in j && typeof j["constructor"] === "string") {
        const name = j["constructor"];
        if ("record" in j) {
            return { kind: "tagged", name, body: "record", members: members(j["record"]) };
        }
        if ("array" in j) {
            return { kind: "tagged", name, body: "array", items: items(j["array"]) };
        }
        if ("tuple" in j) {
            const positional = items(j["tuple"]).map((value) => ({ name: null, value }));
            const named = members(j["named"] ?? {}).map(({ name, value }) => ({ name, value }));
            return { kind: "tagged", name, body: "tuple", args: [...positional, ...named] };
        }
        return { kind: "tagged", name, body: "none" };
    }
    if ("array" in j) {
        return { kind: "array", items: items(j["array"]) };
    }
    if ("record" in j) {
        return { kind: "record", members: members(j["record"]) };
    }
    throw new CodecError(`Not a value: ${JSON.stringify(j)}`);
}

function toExact(value: Value): Exact {
    const members = (ms: Member[]): Exact => Object.fromEntries(ms.map(({ name, value: v }) => [name, toExact(v)]));
    switch (value.kind) {
        case "nat":
        case "int":
        case "float":
            return { [value.kind]: value.text };
        case "string":
            return { string: value.text };
        case "array":
            return { array: value.items.map(toExact) };
        case "record":
            return { record: members(value.members) };
        case "tagged":
            switch (value.body) {
                case "none":
                    return isConstName(value.name) ? { const: value.name } : { constructor: value.name };
                case "record":
                    return { constructor: value.name, record: members(value.members) };
                case "array":
                    return { constructor: value.name, array: value.items.map(toExact) };
                case "tuple": {
                    const out: Exact = {
                        constructor: value.name,
                        tuple: value.args.filter((a) => a.name === null).map((a) => toExact(a.value)),
                    };
                    const named = value.args.filter((a): a is Member => a.name !== null);
                    if (named.length > 0) {
                        out["named"] = members(named);
                    }
                    return out;
                }
            }
    }
}

// Builders, for writing.

export function unit(name: string): Value {
    return { kind: "tagged", name, body: "none" };
}

export function taggedRecord(name: string, members: Member[]): Value {
    return { kind: "tagged", name, body: "record", members };
}

export function taggedTuple(name: string, given: Arg[]): Value {
    return { kind: "tagged", name, body: "tuple", args: given };
}

export function taggedArray(name: string, items: Value[]): Value {
    return { kind: "tagged", name, body: "array", items };
}
