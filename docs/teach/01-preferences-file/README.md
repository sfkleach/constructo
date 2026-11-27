# Lesson 01: How to Write Constructo illustrated by Preferences Files

## Overview

In this lesson, we will explore how to write Constructo documents using the
example of a preferences files. Preferences files are commonly used to store
user settings and application configurations. By the end of this lesson, you
will understand the simplest way to manipulate these files effectively with
Constructo.

Constructo itself is a system for reading and writing highly structured data,
much like JSON or YAML. But unlike those two more familiar formats, Constructo
comes with a built-in way of describing the structure of the data.

## Prerequisites

Make sure the `constructo` tool is installed before proceeding with
this lesson. See the [README.md](../../../README.md) file for installation
instructions. 

N.B. If you are an experienced programmer you may find that reading this
tutorial is enough. But the tool allows you to rehearse the example
interactively, which is likely to be helpful if you go on to use Constructo
yourself.

<!-- TODO: Add installation instructions to the README.md file -->

## Our Running Example: Preferences for a File Explorer

Imagine a file explorer: a program that shows the files and folders on your
computer and lets you browse through them. Like most programs, it remembers how
you like things set up, and it keeps those choices in a preferences file that it
reads each time it starts. Our file explorer has settings such as:

- Whether to show hidden files, such as the "dot files" in a home folder.
- How to treat symbolic links: show them as links, follow them to whatever
  they point at, or hide them altogether.
- How to sort the listing: by name, size, type or date modified, and whether
  folders come before files.
- How to lay out the listing: as icons, as a list or as columns.
- Which folder to open in when it starts.
- A list of bookmarked folders for quick access.

Some of these are simple yes-or-no choices, some are a pick from a fixed set of
options, and some are free text or lists. That mix is typical of real
preferences files, and it is what makes them a good place to see what
Constructo adds. A file explorer has to cope with whatever it finds in its
preferences file, so it pays to catch a misspelt option or a missing setting
before the program ever reads it.

## First step - capture the structure

Constructo works best when you define the structure of the data you are trying
to write. So let's turn the description in written language into a set of
_types_. If you have previously worked with a programming language that demands
static typing, like Rust or C#, then you will have an advantage here.

We start by writing a _type-definition_ using the `enum` keyword. It announces
a new type and sets it equal to one or more _variants_. Each variant has a _tag_
associated with it, which tells you exactly what type it belongs to. In our
example we only need one variant and in this case it is normal to reuse the type
name as the tag.

```
enum Preferences = Preferences { ... CONTENTS ... };
```

The contents of the curly braces are a _record_: a list of named _fields_,
separated by commas, where each field gives its name and then, after a colon,
its type. Let's fill in the easy ones first. Two of our settings are simple
yes-or-no choices, so their type is `Bool`, and the folder to start in is a
piece of text, so its type is `String`:

```
enum Preferences =
    Preferences {
        show_hidden: Bool,
        folders_first: Bool,
        start_folder: String
    };
```

We did not have to define `Bool` or `String` ourselves. `String` is built into
Constructo, along with a few other basic types such as numbers, and `Bool` is
defined for us in its _standard prelude_ as either `true` or `false`. Notice
too that we have spread the definition over several lines to make it easier to
read. Constructo does not mind about this layout but this is a good way to
lay it out on the page.

So let's add another piece of the design. The bookmarks are not a single folder
but a list of them. Putting square brackets around a type means "a list of these",
so a list of strings is written `[String]`:

```
enum Preferences =
    Preferences {
        show_hidden: Bool,
        folders_first: Bool,
        start_folder: String,
        bookmarks: [String],
    };
```

That leaves the settings that pick from a fixed set of options. Take the
symbolic links: the explorer can show them, follow them or hide them, and
nothing else makes sense. We could store the choice as a string such as
`"follow"`, but then nothing would stop someone writing `"folow"` or
`"maybe"`. Instead we give the choice a type of its own, with one variant for
each option:

```
// This introduces a Symlinks type that has exactly these possible values.
enum Symlinks = Show, Follow, Hide;
```

This time there are three variants, separated by commas. They are plain tags and
none of them carries a record, or any other data: the tag is the whole value.
Hence a `Symlinks` value is exactly one of `Show`, `Follow` or `Hide`. Now we
can use our new type for a field, just as we used `Bool` and `String`:

```
enum Preferences =
    Preferences {
        show_hidden: Bool,
        symlinks: Symlinks,
        folders_first: Bool,
        start_folder: String,
        bookmarks: [String],
    };

enum Symlinks = Show, Follow, Hide;
```

Notice that `Preferences` uses `Symlinks` before it has been defined. That is
fine, because the order of the definitions in a file does not matter, so you
can put the most important type first, which is likely to be helpful to anyone
who reads it afterwards.

How should the File Explorer decide on how the listing is sorted and laid out?

These work in just the same way, so they get the same treatment: a type each,
`SortKey` and `Layout`, and a field each, `sort_by` and `layout`. Finally, we
add some comments. Anything from `//` to the end of the line is a comment, which
Constructo ignores. Comments are there for the people who read the file. Here is
the finished type-definition. Save it in a file called `explorer.types`.

Here's what you should end up with - and we have created a
[explorer.types](explorer.types) file already for reference - but you should try
creating this yourself to get a feel for how it fits together.

```
// The preferences of an imaginary file explorer.
enum Preferences =
    Preferences {
        show_hidden: Bool,
        symlinks: Symlinks,
        sort_by: SortKey,
        folders_first: Bool,
        layout: Layout,
        start_folder: String,
        bookmarks: [String],
    };

// How to treat a symbolic link.
enum Symlinks = Show, Follow, Hide;

// What the listing is sorted by.
enum SortKey = Name, Size, Type, Modified;

// How the listing is laid out.
enum Layout = Icons, List, Columns;
```

## Checking our type definitions

Before we write any preferences, we should check that the types themselves make
sense and we have not made a mistake. That is the job of `constructo check`. In
a terminal, navigate to the folder containing `explorer.types` and run the
following command:

```console
$ constructo check explorer.types
$
```

No news is good news: when the check succeeds, `constructo` prints nothing.
That suits scripts and build tools, but while you are learning it can be
reassuring to see a result, and the `-v` (verbose) option asks for one:

```console
$ constructo check -v explorer.types
OK: explorer.types (data-types)
OK: checked 1 file(s)
```

The note `(data-types)` shows that `constructo` recognised this as a file of
types, because it starts with the keyword `enum`.

Now let's see what happens when something is wrong. Suppose we misspell
`String` as `Strng` in the `start_folder` field:

```console
$ constructo check explorer.types
ERROR: Unknown type Strng (explorer.types, line 9, col 23)
```

Constructo knows every type that has been defined, so it spots a name that is
not one of them and tells us exactly where it is. Or suppose we forget the
semi-colon at the end of the last line:

```console
$ constructo check explorer.types
ERROR: Expected comma or semi-colon (explorer.types, line 21, col 1); in enum Layout
```

Line 21 is just past the end of the file. Constructo reached the end while it
was still waiting for either another variant of `Layout`, after a comma, or
the semi-colon that finishes the definition. When an error is reported at a
place that looks fine, the mistake is often just before it.

Put the mistakes right, check again, and we are ready to write some
preferences.

## Writing Preferences in Constructo

Let's now try to write a preferences file that fits our type definitions. Create
a file called `explorer.prefs` (constructo doesn't mind what extension you use)
and start by adding the `Preferences` block that matches our type definition:

```
Preferences {
    // ... Further contents will go here. Notice how the tag and braces match 
    // the tag and braces of the definition ...
};
```

The `//` begins a comment in both type-definitions and values files. Comments 
are ignored by the `constructo` tool. Let's check what the tool thinks of our
initial incomplete preferences file.

```console
$ constructo check explorer.prefs
ERROR: Unknown constructor Preferences (explorer.prefs, line 1, col 1)
```

That's right, `constructo` does not yet know about the `Preferences` constructor
yet. So let's include the type definitions when checking the preferences file.
We have to make sure we provide all the type definition files first, before the
file with the preference-value itself.

```console
$ constructo check explorer.types explorer.prefs
ERROR: Missing field show_hidden in Preferences (explorer.prefs, line 1, col 1)
```

Now `constructo` makes more progress. It recognises the `Preferences`
constructor but notices that a required field is missing. Let's add show_hidden
and see what happens. Our preferences file should now look like this:

```
Preferences {
    show_hidden: true,
    // Further fields will go here ...
};
```

And when we check, Constructo will now point out the next missing required
field, `symlinks`. We would continue adding the required fields one by one until
the preferences file is complete.

```console
$ constructo check explorer.types explorer.prefs
ERROR: Missing field symlinks in Preferences (explorer.prefs, line 1, col 1)
```

Let's add the `symlinks` field to our preferences file. This is a three way
choice between `Show`, `Follow`, and `Hide`. Let's tell it to `Follow` symlinks by default. Our preferences file should now look like this:

```
Preferences {
    show_hidden: true,
    symlinks: Follow,
    // Further fields will go here ...
};
```

Predictably, Constructo now prompts for `sort_by` as the next required field. We would continue adding the required fields in a similar manner until the preferences file is complete. 

Let's now skip ahead to bookmarks, which is the next interesting field in our preferences file. Our `explorer.prefs` file looks like this:

```
Preferences {
    show_hidden: true,
    symlinks: Follow,
    sort_by: Name,
    folders_first: true,
    layout: Icons,
    start_folder: "/home/user",
};
```

Bookmarks are a list of strings representing the paths to the bookmarked locations in the file explorer. Lists are enclosed in square brackets `[]` and can contain items separated by commas. To include bookmarks in our preferences file, we would add a `bookmarks` field with a list of paths. For example:

```
Preferences {
    show_hidden: true,
    symlinks: Follow,
    sort_by: Name,
    folders_first: true,
    layout: Icons,
    start_folder: "/home/user",
    bookmarks: ["/home/user/Documents", "/home/user/Downloads"],
};
```

Now we can check our preferences file with `constructo check` to ensure that all
required fields are present and correctly specified. We can add the `-v` to see
the details of what it accepted.

```console
$ constructo check explorer.types explorer.prefs -v
OK: explorer.types (data-types)
OK: explorer.prefs (data-values, type-checked)
OK: checked 2 file(s)
```

## Recap, What We Learned

We took an informal list of settings and turned it into a checked preferences
file. Along the way we met these ideas:

- **Types come first.** A _type-definition_ starts with `enum`, names the new
  type and lists its _variants_ after the `=`, separated by commas and ended
  by a semi-colon.
- **A variant can carry a record.** `Preferences { ... }` is a tag followed by
  a _record_ of named fields, each written `name: Type`.
- **A variant can be just a tag.** `enum Symlinks = Show, Follow, Hide;` gives
  a fixed set of choices, so a misspelt option is caught rather than read as
  some unknown string.
- **Some types are provided for you.** `String` is built in, `Bool` comes from
  the standard prelude, and `[T]` means "a list of `T`".
- **Order does not matter.** A type can be used before it is defined, so put
  the most important one first.
- **Values mirror their types.** A value is written with the same tag and
  braces as its definition, with each field given a value instead of a type.
- **`//` starts a comment** in both types files and values files.
- **`constructo check` does the checking.** It prints nothing on success, `-v`
  shows what it accepted, and an error gives the line and column. When an
  error points at a place that looks fine, look just before it.
- **Types files go before values files.** `constructo check explorer.types
  explorer.prefs` loads the types and then checks the preferences against
  them, reporting missing fields one at a time.


## Where to go next

Now you know a little bit about what Constructo looks like, how to write it
and how to check it. There are several ways to continue from here. If you want
a complete step-by-step course, just follow the increasing numbered lessons.

- The next step is reading a Constructo file into your
  program and using the values in it. See
  [02-reading-constructo-files](../02-reading-constructo-files/README.md) for
  that.
- You might want to explore more complex types and how to define them in
  Constructo, such as nested records or enums with multiple variants.
- You could also look into how to handle optional fields and default values in
  Constructo, which can make your preferences files more flexible.
- You may also want to learn about how to structure larger projects with
  multiple types and values files, and how to organize them for
  maintainability.
