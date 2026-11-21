// The program's own colours, which existed before Constructo was used. A colour is any RGB value, and only three of
// them have names in the drawing format.

export class Colour {
    readonly rgb: number;

    constructor(rgb: number) {
        this.rgb = rgb;
    }
}

export const RED = new Colour(0xff0000);
export const GREEN = new Colour(0x00ff00);
export const BLUE = new Colour(0x0000ff);
