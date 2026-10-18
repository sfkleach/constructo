# Task: Implement the constructo tool with the `check` subcommand

## Overview

The `constructo` CLI tool will implement several subcommands, including `check`,
`query`, `convert` and `complete`. This task focusses on the initial
implementation using just a single subcommand.

Our implementation language will be Rust and our implementation will support 
both `constructo` as a CLI tool and also as a library for processing Constructo
files.

We will proceed step-by-step - mainly so I can catch up and review the changes.

## Step 1: Skeleton Implementation

Provide a skeleton implementation that simply processes the command line
and prints the options back to the terminal. The syntax for the check 
subcommand is:

```
constructo check [--stdin] FILES...
```

Note that the `--stdin` option is intended to process the standard input as 
if it was another file. However at this stage, it is just echoed back to the
terminal.

