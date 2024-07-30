# Changelog

<!-- next-header -->
## [Unreleased] - ReleaseDate 



## [0.1.1]

This release contains a major bug fix for `cargo creusot` fixing the loading of metadata for crates such as `creusot-contracts`. If your proofs were not passing before, this may be why. 

It also bumps the associated version of why3 to 1.7.2 from 1.7.1




The following are the release notes for the first official version of Creusot, a verification tool for safe Rust programs. Using Creusot you can prove -- formally -- that your Rust program behaves in a specific manner. 

Creusot allows you to annotate your code using *contracts* which describe correctness conditions for your program. Creusot then uses SMT solvers to check that these contracts hold for all possible runs of the program. All of this is done statically without running your program, and contracts are erased at compilation time.

Creusot is currently best suited for the verification of code like data-structures or algorithm implementations, and less so for systems which interact heavily with the outside world or exploit concurrency. Notable projects using Creusot include the [CreuSAT](https://github.com/sarsko/creusat) verified SAT solver and the [Sprout](https://github.com/xldenis/cdsat) SMT solver. 

Since this is the first release of Creusot, we will cover the currently implemented features and aspects of Rust which are supported. 

*Creusot is still very experimental software, you should expect some obscure crashes and missing features.* 

## Getting Started with Creusot

To get started as a user with Creusot, we recommend checking out the [README](https://github.com/creusot-rs/creusot/blob/master/README.md).

## Cargo Integration

Creusot provides the `cargo creusot` binary which serves to make verification of crates simple. 
To get started with Creusot, you can run `cargo creusot` on your crate, generating proof obligations in `target/your-crate.mlcfg`. 
These proof obligations can be discharged using Why3 and the Why3 IDE. 

### `cargo creusot setup`

To help you manage your Creusot and Why3 installations, we provide `cargo creusot setup`. 
Through this command you can install, or view the Creusot specific Why3 configuration and prover installations. 

```
Setup and manage Creusot's installation

Usage: creusot setup <COMMAND>

Commands:
  status   Show the current status of the Creusot installation
  install  Setup Creusot or update an existing installation
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

`cargo creusot setup install` can currently download and install binaries for `CVC4`, `CVC5` and `Z3`. Eventually it will also be capable of installing `Alt-Ergo` and `Why3` for you (currently these need to be installed separately using opam). See the README for detailed installation instructions.

'External' versions of supported provers can be provided via the `--external` flag.  

## Supported Language Features

Creusot supports a large portion of the Rust language, some of these language constructs are listed below:

### Basic Language constructs

**Control flow:** All control flow syntax is supported:`while`, `loop`, `for`, `if`, `else`, `let if`, `let else`, `break`, `continue`, etc.. 

**Borrows** Creusot has complete support for all forms of borrowing in Rust. **This includes mutable borrows**. You can create borrows, reborrow fields, store borrows in types, **all operations on borrows are supported**. 

### Traits

**Traits Decls & Impls:** Curently, we support traits with associated methods types, and constants. We also handle implementations of these traits.
GATs are currently unsupported.

**Bounds:** We support trait bounds involving associated types. 
Bounds involving GATs, const generics, or HRTB are unsupported. 

**Trait Objects**: Trait objects are not supported.

### Closures

Creusot supports most closures in Rust, including `FnOnce`, `FnMut` or `Fn`, and we also support `move` closures. Async closures are not supported as Creusot does not support `async` code currently. 

### Iterators

By building off our support for closures and traits, we provide extensive support for iterators and iterator chains in Creusot. 

## Unsupported Language Features

Equally important are the parts of Rust which are unsupported by Creusot, if your code uses these you will need to rewrite it before Creusot can help you verify your code.

### Async

Async code is currently unsupported by Creusot, we do not support generators or coroutines in our encoding. 

### Unsafe

Creusot's verification relies on the Rust type system's ownership so we cannot verify code using raw pointers. 
When raw pointers are encountered, Creusot will not crash but instead generate verification conditions which will be unprovable for almost anything. 
This allows trivial usage of mutable pointers for things like `vec![]`.

### Trait Objects

We do not currently support trait object reasoning. Like with raw pointers we generate verification conditions which are useless for anything beyond the most trivial checks.
This allows usage of `println!()` or similar macros to work. 

### HRTB & GATs

Advanced features like Higher-Ranked Trait Bounds or Generic Associated Types are not currently supported by Creusot.

## Specification Language: Pearlite

To enable verification, Creusot provides a *specification language* called Pearlite which you can use to annotate your code with assertions code must satisfy. Pearlite includes many features that are targeted to make specific kinds of Rust code verifiable, which we will list below.

### Contracts & Assertions

Pearlite provides several forms of contracts you can use to specify your code:

- `#[requires(..)]` specifies a precondition for a function. This accepts a Pearlite expression as argument. Requires clauses will be checked at all program function call sites.
- `#[ensures(..)` specifies a postcondition for a function. Accepts a Pearlite expression. Creusot will check that all function exits uphold the postcondition. 
- `#[variant(..)]` specifies a *variant* which can be used to show that a function terminates. A variant clause must evaluate to a type with a strictly decreasing, finite size (aka a well-founded order in math speak). Examples of this are positive integers, or any Rust type defined with `Box`. This will be enforced at all recursive calls. 
- `#[invariant(..)]` specifies a *loop invariant*, a property which is true throughout every iteration of a loop. Invariants are the only thing Creusot uses to reason about the behavior of loops. This is checked at loop entry and at the end of each loop iteration.
- `proof_assert!(..)` is a Pearlite analogue to Rust's `assert!` macro. It allows you to assert the truth value of a Pearlite expression, which may mention ghost code. 
- `#[trusted]` tells Creusot not to generate any proof obligations for the attached function. 
### Pearlite Expressions

Pearlite is a *total*, *functional* language with a syntax similar to Rust. It extends the syntax of Rust expressions in several minor ways, and more importantly has different *semantics*, implied by its totality.
This means that operations like `a + b` are always defined, even for values which would panic in ordinary Rust. Instead, in Pearlite they would produce an *unspecified value*.

#### Basic Syntax

Pearlite syntax corresponds of basic Rust expressions: variables, literals, let bindings, if-else, match, function calls & type constructors, extended with a few new operations.
We provide quantifiers `forall<x : T, ..> expr` and `exists<x : T, .. > expr` for univeral and existential quantification respectively. 
We also have the postfix "model" operator `@` which is a sugar for Creusot's `ShallowModel` trait. 
The most important operation in Pearlite is the `^` final operator which is used to dereference a mutable borrow at the end of its lifetime. 

Pearlite can be written in two different ways, either as ordinary Rust, in which case you are restricted to the Rust syntax fragment of Pearlite, or using the `pearlite!{}` macro which provides our additional syntax:

```
#[predicate]
fn my_function(x: Int) -> bool { 
    x >= 0 // can use basic pearlite here
}

#[predicate]
fn exists_inverse(y : Int) -> bool {
 pearlite! { exists< x : Int> x + y == 0 }
}
```

Pearlite contract clauses are implicitly wrapped in a `pearlite!` macro invocation.
 
#### Mutable Borrows

Rust code uses lots of mutable borrows, so having an efficient method for reasoning about them and proving code using borrows is important.
Pearlite provides the `^` final value operator which mirrors `*` by dereferencing in the "future" of a borrow. 

Using this we could specify a simple increment function as follows:

```rust
#[requires(x < 1000u32)]
#[ensures(^ x == *x + 1u32)]
fn incr(x : &mut u32) { *x += 1}
```
This postcondition can be read as "the final value of x is its initial value + 1".

Where the `^` final value becomes particularly handy is when functions return mutable borrows, like the following:

```rust
#[ensures(^ result == (^x).0)]
#[ensures(* result == (* x).0)]
fn project(x: &mut (u32, bool)) -> &mut u32 {
    &mut x.0
}
```

By adding this postcondition stating that the final value of the result is the same as the final value of the first field of `x`, Creusot can track updates to the `x` through the returned borrow. 

This kind of code which mirrors a specification for the current and final values is so common that we also provide "logical (re) borrowing" as a syntactic sugar. The prior example could instead be written as:

```rust
#[ensures(result == &mut x.0)]
fn project(x: &mut (u32, bool)) -> &mut u32 {
    &mut x.0
}
```

### Logical Types

In verification, we often want to use abstractions which have nicer mathematical properties than real-world types and values (like unbounded integers, mathematical sets, etc...). 
Several of these types are provided as part of the Pearlite standard library:
- `Int` the type of mathematical integers. You can convert a machine integer to an `Int` by taking its model `x@`.
- `Seq<T>` the type of sequences, useful for reasoning about arrays and vectors
- `FSet<T>` the type of *finite* sets.
- `Map<A, B>` the type of mappings from `A` to `B` (aka, functions between `A` and `B`). 

### Predicates and Logical functions

Users can define their own Pearlite functions and predicates through the `#[logic]` and `#[predicate]` attributes. 
When these attributes are attached to a function, they will be interpreted as Pearlite definitions and available in contracts but not in programs. 

Logical functions and predicates can be given contracts, and will then generate proof obligations to make sure those contracts hold. 
However, it is well defined to call a logical function even with values that do not satisfy its precondition: the output will then be an (unspeficied) value which does not uphold the postcondition.

Several modifiers can be applied to Pearlite functions:

- `#[open(path)]` specifies the *opacity* of a definition, an important concept which controls which proofs can see the body of a definition and which see only a symbol.
- `prophetic`, `#[predicate]` and `#[logic]` can accept a `prophetic` argument `#[predicate(prophetic)]` which enables access to the final operator `^`. 

### Ghost code 

Pearlite currently supports a basic form of ghost code known as *snapshots*. Using the `snapshot!` macro you can take a "copy" of a Rust value at a given point in time to use in your proofs. This can be useful to reason about the evolution of state in loops like the following sort routine:

```rust 
  let old_v = snapshot! { v };
    let mut i = 0;
    #[invariant(sorted_range(v@, 0, i@))]
    #[invariant(v@.permutation_of(old_v@))]
    while i < v.len() {
        if i == 0 || v[i - 1] <= v[i] {
            i += 1;
        } else {
            v.swap(i - 1, i);
            i -= 1;
        }
    }
```

We use the snapshot before the loop to ensure that we are only permutting elements which were originally in our array, not adding or removing them. 

### Trait laws & Refinements

Trait declarations can include logical functions and predicates, along with contracts on their program functions.
Each implementation of the trait will then have to prove that they are at least as precise as the contract, their preconditions must be no more strict and their postcondition cannot be weaker than the trait signature itself. 

Often, there are certain properties which are useful for reasoning about types which implement a given trait, like how `Ord` describes a *total order* and should be transitive, anti-symmetric and reflexive. 
These kinds of properties are called "laws" and can be enforced by adding a trait item marked with the `#[law]` attribute and appropriate contract. 
Creusot will automatically consider all possible laws when generating proof obligations, alleviating the user from having to remind the tool about these properties. 

### Closure Contracts

When working with closures, contracts gain a few additional powers.
First, contracts of closures can mention the captured variables. These will have the same type as the Rust variable, that is, if a closure captures `x: T` by mutable borrow, the contract will still see `x` as having type `T` not `&mut T`. 
Additionally, in postconditions we can use the `old(..)` function to refer to the value of a capture *before* the call. `old(..)` can only be applied to captured variables of a closure. 

Note, the contracts of a closure are not allowed to capture additional variables. 

### Type Invariants

Certain types have additional "validity" properties beyond what is allowed by their constructors. 
These properties can be enforced by implementing the `Invariant` trait provided by Creusot. 
This trait declares a type as having a "type invariant" which must be enforced defined points: when a value is being passed to a function, when a value is being returned, and when a value a borrowed value ends its lifetime. 
Creusot will automatically instrument your code with the relevant invariant checks for types which implement this trait. 

### Extern Specs

Because verified Rust code must inevitably interact with the unverified outside world, we provide the `extern_spec!` macro which allows you to assume that outside functions satisfy a contract. 

The macro accepts a set of Rust modules describing the path of the functions being specified:

```rust
extern_spec! {
    mod std {
        mod mem {
            #[ensures(^dest == src)]
            #[ensures(result == *dest)]
            fn replace<T>(dest: &mut T, src: T) -> T;
        }
    }
}
```

Calls to externally specified functions will have their contracts enforced. 

Extern specs are also allowed to refine trait bounds: this is useful for when you need a generic function to only accept "well behaved" types, like in the following: 

```rust=
extern_spec! {
    mod std {
        mod cmp {
            trait PartialEq<Rhs> {
                #[ensures(result == (self.deep_model() == rhs.deep_model()))]
                fn eq(&self, rhs: &Rhs) -> bool
                where
                    Self: DeepModel,
                    Rhs: DeepModel<DeepModelTy = Self::DeepModelTy>;
            }
        }
    }
}
```

We require that the types of `eq`'s parameter implement the Creusot specific `DeepModel` trait, allowing us to use `deep_model` in its specification.

When an extern spec has refined trait bounds, Creusot will enforce that all trait implementations and function calls respect these bounds.

### `creusot-contracts`

Pearlite is provided as part of the `creusot-contracts` crate which ships with a set of `extern_specs!` for parts of the Rust standard library and various useful logical functions, types and traits. 

Some of the specified types & traits are:
- `Vec`
- `Deque`
- `[T]`
- `std::mem`
- `Box`
- `Range`
- `Clone`, `Copy`, `PartialEq`, `PartialOrd`
- `FromIterator` and `IntoIterator`
- `Iterator`:  We provide support for the following adapters
    - `skip`
    - `take`
    - `repeat`, `cycle`, `empty`, `once`, `collect`
    - `enumerate`
    - `cloned`, `copied`
    - `fuse`
    - `zip`
    - `map` (via `map_inv`)
    - Implementations of iterator for core types like `Vec`, `[T]`, `[T; N]`, `0..n` are supported. 

We also provide logical types like `Int`, `Map`, `FSet`, `Set`, `Seq`, along with useful apis for each of those types. 

## Conclusion

Thanks for your interest in Creusot, we hope it will be of use to you! 

There are many small contributions and features which have gone unlisted, but we hope to have described the essential ones. 
Going forward, we'd like to make more frequent releases, avoiding the issue of having a giant release every 3 years. 

I'd like to thank some of the contributors that have made Creusot possible, this list is not exhaustive:
- Yusuke Matsushita (@shiatsumat)
- Sarek Skotam (@sarsko)
- Jacques-Henri Jourdan (@jhjourdan)
- Dominik Stolz (@voidc)
- David Ewert (@dewert99)
- Arnaud Golfouse (@arnaudgolfouse)
- Armaël Guéneau (@armael)

<!-- next-url -->
[Unreleased]: https://github.com/assert-rs/predicates-rs/compare/v3.1.2...HEAD