# Task: Design and Implement Non-finite values

## Overview: The Monogram Project

Following the design of Monogram, which has a substantially compatible and
more mature design, we will adopt the following non-finite floating-point 
values. 

    0n1.0 = positive infinity (Float)
    -0n1.0 = negative infinity (Float)
    0n0.0 = nullity / NaN (Float)


Note that "nullity" relates to Anderson's transreal arithmetic, which provides a
clean extended real-arithmetic as a supporting theory for a variant of floating
point arithmetic without the nonsensical NaN or minus zero values. The
deliberate ambiguity allows Constructo to have the interpretation of nullity/NaN
depending the targeted language.

### Aside: Nonsense in IEEE Floating Point Arithmetic

- NaN is defined as not equal to itself, which is inconsistent. It is not merely
  not-a-number, it is not-a-possible-value. Or equality is bugged, one are free
  to choose between these bad options.
- Minus zero is equal to zero yet has different properties. This is another
  axiomatic inconsistency; an equality that cannot be substituted. Obviously it
  can play the role of a negative infinitesimal but there is no positive
  counterpart not any inexactness bit, which makes it very tricky to use.


### Reserved Values 

Note that these values are reserved for future expansion (they correspond to
Nat and Int versions of the same concepts)

    0n1
    +0n1
    -0n1
    0n0
    +0n0
    -0n0


