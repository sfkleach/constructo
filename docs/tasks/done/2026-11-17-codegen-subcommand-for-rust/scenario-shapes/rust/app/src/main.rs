//! The program of the scenario: it reads a drawing, writes it back, and shows what is refused.

mod adopt;
mod paint;
mod shapes;

use shapes::Drawing;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("the path of a values file, and of a file to write to if wanted");
    let text = std::fs::read_to_string(path).expect("a readable file");
    let codec = shapes::codec();

    let drawing: Drawing = codec.read(&text).unwrap_or_else(|e| panic!("{e}"));
    println!("--- Read\n{drawing:#?}");

    let written = codec.write(&drawing).unwrap_or_else(|e| panic!("{e}"));
    println!("--- Written\n{written}");
    if let Some(out) = std::env::args().nth(2) {
        std::fs::write(out, &written).expect("a writable file");
    }

    let again: Drawing = codec.read(&written).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(drawing, again);
    println!("--- Read back: the same");

    let id = r#"ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8")"#;
    let other = r#"com.example.ids::Uuid("00000000-0000-0000-0000-000000000001")"#;
    let ids: Vec<uuid::Uuid> = codec
        .read(&format!("[{id}, {other}]"))
        .unwrap_or_else(|e| panic!("{e}"));
    println!("--- A Vec of a foreign adopted type\n{ids:?}");

    println!("--- Refused");
    let styled = |item: &str| format!("Styled {{ item: {item}, colour: null, offset: {{ dx: 0, dy: 0 }} }}");
    let refused: Vec<(&str, String)> = vec![
        (
            "a constructor of another type",
            format!(r#"Drawing({id}, "T", [{}])"#, styled("Point(1.0, 2.0)")),
        ),
        (
            "a name that is not in the run",
            format!(r#"Drawing({id}, "T", [{}])"#, styled("Hexagon")),
        ),
        (
            "a bare name that the project does not have",
            r#"Drawing(Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"), "T")"#.into(),
        ),
        (
            "a string that is not a UUID",
            r#"Drawing(ids::Uuid("not-a-uuid"), "T")"#.into(),
        ),
        ("the id left out", r#"Drawing(title: "T")"#.into()),
        (
            "a Nat that does not fit u64",
            format!(r#"Drawing({id}, "T", [], 18446744073709551616)"#),
        ),
        (
            "a Float that does not fit f64",
            format!(r#"Drawing({id}, "T", [{}])"#, styled("Circle { radius: 1.0e400 }")),
        ),
        (
            "an unknown const",
            format!(r#"Drawing({id}, "T", [{}])"#, styled("unit_square")),
        ),
        (
            "a missing field",
            format!(r#"Drawing({id}, "T", [Styled {{ item: Dot, colour: null, offset: {{ dx: 0 }} }}])"#),
        ),
    ];
    for (label, text) in &refused {
        match codec.read::<Drawing>(text) {
            Ok(d) => println!("{label}: ACCEPTED {d:?}"),
            Err(e) => println!("{label}: {e}"),
        }
    }

    let mut huge = drawing.clone();
    huge.shapes[0].item = shapes::Shape::Circle(shapes::Circle { radius: 1e300 });
    huge.shapes[1].item = shapes::Shape::Circle(shapes::Circle { radius: 1e-7 });
    let text = codec.write(&huge).unwrap_or_else(|e| panic!("{e}"));
    let back: Drawing = codec.read(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(huge, back);
    println!("--- Large and small floats written and read back");

    let mut custom = drawing.clone();
    custom.shapes[0].colour = Some(paint::Colour::Custom(0x336699));
    match codec.write(&custom) {
        Ok(text) => println!("a custom colour: WRITTEN {text}"),
        Err(e) => println!("a custom colour: {e}"),
    }
}
