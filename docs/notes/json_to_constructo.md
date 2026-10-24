# Translating JSON to Constructo

Here is a simple JSON file that I often use as a simple, but not trivial, 
example.

```json
{
    "widget": {
        "debug": "on",
        "window": {
            "title": "Sample Konfabulator Widget",
            "name": "main_window",
            "width": 500,
            "height": 500
        },
        "image": {
            "src": "Images/Sun.png",
            "name": "sun1",
            "hOffset": 250,
            "vOffset": 250,
            "alignment": "center"
        },
        "text": {
            "data": "Click Here",
            "size": 36,
            "style": "bold",
            "name": "text1",
            "hOffset": 250,
            "vOffset": 100,
            "alignment": "center",
            "onMouseUp": "sun1.opacity = (sun1.opacity / 100) * 90;"
        }
    }
}
```

This is how I would expect it would be in Constructo. First the data-value. Notice
that we don't need the data-type to write Constructo. The data-types are there to 
help us detect mistakes and to document the form:

```
Widget{
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

The data-types to go with this are:

```
enum Widget = Widget{debug: String, window: Window, image: Image, text: Text};
enum Window = Window{title: String, name: String, width: Nat, height: Nat };
enum Image = Image{ 
    src: String, 
    name: String,
    hOffset: Nat,
    vOffset: Nat,
    alignment: String
};
enum Text = Text {
    data: String,
    size: Nat,
    style: String,
    name: String,
    hOffset: Nat,
    vOffset: Nat,
    alignment: String,
    onMouseUp: String
};
```