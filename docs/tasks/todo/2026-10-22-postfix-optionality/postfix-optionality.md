# Task: Implement the Postfix `?` constructor and type constructor

## Overview

Because `Option<T>` is such a ubiquitous type, we want a convenient shorthand
for `Option<T>`, namely `T?`. And since the constructor `Some(t)` gets used a 
lot, we want to be able to write (say) `0?`, equivalent to `Some(0)`.

## Step 1: `?` as postfix `Some` constructor

Implement a postfix operator `?` in the data-values grammar, which is a 
syntactic shorthand for the `Some(...)` constructor.

# Step 2: `?` as postfix `Option` generic type-constructor

Implement a postfix operator `?` in the data-types grammar, which is a 
syntactic shorthand for the `Option<...>` type constructor.
