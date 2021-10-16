#![feature(register_tool, rustc_attrs)]
#![register_tool(creusot)]
#![feature(proc_macro_hygiene, stmt_expr_attributes)]

extern crate creusot_contracts;

use creusot_contracts::*;

// Tests that we can use field access syntax in pearlite.

struct A {
    a: bool,
}

#[trusted]
#[ensures(x.a === x.a)]
pub fn solver(x: A) {}
