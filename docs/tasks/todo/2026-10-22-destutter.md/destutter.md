# Task: Implement destuttering

NOTE: This is a thought-experiment at this stage. Destuttering needs the 
ability to textually infer names but this creates the problem that we cannot
elaborate a Constructo document without a "stylesheet", meaning that the 
doocument is not self-contained. 

- The embedded option is a way of handling it but adds a clunkiness to the syntax.
- Another idea would be to add an option that illustrates the style
  ```
  enum PascalCase<PascalCase> = PascalCase{camelCase: _};
  const camelCase = _;
  ```
- Or we allow positional & mandatory arguments (like XML)
  ```
  enum Widget = Widget{debug: String, Window, Image, Text};
  ```
- Or we limit destuttering to the situation when types are specified.
- Or we limit destuttering to when we specify the destuttering options.
- Or we support not writing attributes and allowing the options to be filled in
  order, also requires the types to be specified. (However expensive
  type-checking is not forced just because types are supplied.)

Consider the following example - we can see 'stuttering' in the repetition of
keywords and their related constructors e.g. `window: Window`.

```
Widget {
    debug: "on",
    window: Window {
        title: "Sample Konfabulator Widget",
        name: "main_window",
        width: 500,
        height: 500
    },
    image: Image {
        src: "Images/Sun.png",
        name: "sun1",
        hOffset: 250,
        vOffset: 250,
        alignment: "center"
    },
    text: Text {
        data: "Click Here",
        size: 36,
        style: "bold",
        name: "text1",
        hOffset: 250,
        vOffset: 100,
        alignment: "center",
        onMouseUp: "sun1.opacity = (sun1.opacity / 100) * 90;"
    }
}
```

To reduce the stutter, we permit a shortcut that allows inferring the field name
from the constructor name. By default we use PascalCase for constructor-names
and camelCase for field-names - so if we omit the field specifier `window` we
can infer it from the constructor `Window`. 

```
Widget {
    debug: "on",
    Window {
        title: "Sample Konfabulator Widget",
        ...
    },
    Image {
        src: "Images/Sun.png",
        ...
    },
    Text {
        data: "Click Here",
        ...
    }
}
```

Since the preferred case-convention affects the consistent parsing of a
Constructo document, it needs to be included in the document as an _embedded
option_. For example, if we used UPPERCASE for field names we would add this
to the document:

```
#field-names=UPPERCASE
Widget {
    debug: "on",
    Window {
        title: "Sample Konfabulator Widget",
        ...
```

And that would be equivalent to:

```
Widget {```
    debug: "on",
    WINDOW: Window {
        title: "Sample Konfabulator Widget",
        ...
```

