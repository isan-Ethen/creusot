//! Detection of loops and mutually recursive functions.
//!
//! Care is taken around the interaction with traits, like the following example:
//! ```
//! # use creusot_contracts::*;
//! pub trait Foo {
//!     #[terminates]
//!     fn foo() {}
//! }
//!
//! impl Foo for i32 {
//!     #[terminates]
//!     fn foo() {
//!         bar::<i32>(); // infinite recursion !
//!     }
//! }
//!
//! #[terminates]
//! pub fn bar<T: Foo>() {
//!     T::foo();
//! }
//! ```

use crate::{
    backend::is_trusted_function, contracts_items, ctx::TranslationCtx, specification::contract_of,
    traits::TraitResolved,
};
use indexmap::{IndexMap, IndexSet};
use petgraph::{graph, visit::EdgeRef as _};
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::{
    thir,
    ty::{EarlyBinder, FnDef, GenericArgKind, GenericArgs, ParamEnv, TyCtxt, TyKind},
};
use rustc_span::Span;

/// Validate that a `#[terminates]` function cannot loop indefinitely. This includes:
/// - forbidding program function from using loops of any kind (this should be relaxed
/// later).
/// - forbidding (mutual) recursive calls, especially when traits are involved.
///
/// Note that for logical functions, these are relaxed: we don't check loops, nor simple
/// recursion, because why3 will handle it for us.
pub(crate) fn validate_terminates(ctx: &mut TranslationCtx) {
    ctx.tcx.dcx().abort_if_errors(); // There may have been errors before, if a `#[terminates]` calls a non-`#[terminates]`.

    let CallGraph { graph: mut call_graph, additional_data } = CallGraph::build(ctx);

    // Detect simple recursion, and loops
    for fun_index in call_graph.node_indices() {
        let fun_inst = call_graph.node_weight(fun_index).unwrap();
        let def_id = fun_inst.def_id;
        if let Some(self_edge) = call_graph.edges_connecting(fun_index, fun_index).next() {
            let (self_edge, span) = (self_edge.id(), *self_edge.weight());
            if additional_data[&fun_index].is_pearlite {
                call_graph.remove_edge(self_edge);
                continue;
            }
            call_graph.remove_edge(self_edge);
            if !additional_data[&fun_index].has_variant && def_id.is_local() {
                let fun_span = ctx.tcx.def_span(def_id);
                let mut error =
                    ctx.error(fun_span, "Recursive function without a `#[variant]` clause");
                error.span_note(span, "Recursive call happens here");
                error.emit();
            }
        };
        if let Some(loop_span) = additional_data[&fun_index].has_loops {
            let fun_span = ctx.tcx.def_span(def_id);
            let mut error = if contracts_items::is_ghost_closure(ctx.tcx, def_id) {
                ctx.error(fun_span, "`ghost!` block must not contain loops.")
            } else {
                ctx.error(fun_span, "`#[terminates]` function must not contain loops.")
            };
            error.span_note(loop_span, "looping occurs here");
            error.emit();
        }
    }

    // detect mutual recursion
    let cycles = petgraph::algo::kosaraju_scc(&call_graph);
    for mut cycle in cycles {
        if cycle.iter().all(|n| !call_graph.node_weight(*n).unwrap().def_id.is_local()) {
            // The cycle needs to involve at least one function in the current crate.
            continue;
        }
        let Some(root) = cycle.pop() else {
            continue;
        };
        if cycle.is_empty() {
            // Need more than 2 components.
            continue;
        }
        let in_cycle: IndexSet<_> = cycle.into_iter().collect();
        let mut cycle = Vec::new();
        // Build the cycle in the right order.
        petgraph::visit::depth_first_search(&call_graph, std::iter::once(root), |n| {
            use petgraph::visit::Control;
            match n {
                petgraph::visit::DfsEvent::Discover(n, _) => {
                    if in_cycle.contains(&n) {
                        cycle.push(n);
                        Control::Continue
                    } else if n == root {
                        Control::Continue
                    } else {
                        Control::Prune
                    }
                }
                petgraph::visit::DfsEvent::BackEdge(_, n) if n == root => Control::Break(()),
                _ => Control::Continue,
            }
        });

        cycle.reverse();

        let root_def_id = call_graph.node_weight(root).unwrap().def_id;
        let mut next_instance = root;
        let mut error = ctx.error(
            ctx.def_span(root_def_id),
            &format!(
                "Mutually recursive functions: when calling `{}`...",
                ctx.tcx.def_path_str(root_def_id)
            ),
        );
        let mut instance;
        while let Some(id) = cycle.pop() {
            instance = next_instance;
            next_instance = id;
            if let Some(e) = call_graph.edges_connecting(instance, next_instance).next() {
                let span = *e.weight();
                let d1 = call_graph.node_weight(instance).unwrap().def_id;
                let d2 = call_graph.node_weight(next_instance).unwrap().def_id;
                error.span_note(
                    span,
                    format!(
                        "then `{}` calls `{}`...",
                        ctx.tcx.def_path_str(d1),
                        ctx.tcx.def_path_str(d2)
                    ),
                );
            }
        }
        instance = next_instance;
        next_instance = root;
        if let Some(e) = call_graph.edges_connecting(instance, next_instance).next() {
            let span = *e.weight();
            let d1 = call_graph.node_weight(instance).unwrap().def_id;
            let d2 = call_graph.node_weight(next_instance).unwrap().def_id;
            error.span_note(
                span,
                format!(
                    "finally `{}` calls `{}`.",
                    ctx.tcx.def_path_str(d1),
                    ctx.tcx.def_path_str(d2)
                ),
            );
        }

        error.emit();
    }

    ctx.tcx.dcx().abort_if_errors();
}

struct CallGraph<'tcx> {
    graph: graph::DiGraph<FunctionInstance<'tcx>, Span>,
    additional_data: IndexMap<graph::NodeIndex, FunctionData>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FunctionInstance<'tcx> {
    def_id: DefId,
    generic_args: &'tcx GenericArgs<'tcx>,
}

#[derive(Default)]
struct BuildFunctionsGraph<'tcx> {
    graph: graph::DiGraph<FunctionInstance<'tcx>, Span>,
    additional_data: IndexMap<graph::NodeIndex, FunctionData>,
    instance_to_index: IndexMap<FunctionInstance<'tcx>, graph::NodeIndex>,
    to_visit: Vec<(ToVisit<'tcx>, graph::NodeIndex)>,
}

#[derive(Default)]
struct FunctionData {
    /// `true` if the function is a pearlite function (e.g. `#[logic]`).
    is_pearlite: bool,
    /// `true` if the function has a `#[variant]` annotation.
    ///
    /// For now, mutually recursive functions are never allowed, so this only matter for
    /// the simple recursion check.
    has_variant: bool,
    /// `Some` if the function contains a loop construct (contains the location of the loop).
    ///
    /// The body of external function are not visited, so this field will be `false`.
    has_loops: Option<Span>,
}

impl<'tcx> BuildFunctionsGraph<'tcx> {
    /// Insert a new instance function in the graph, or fetch the pre-existing instance.
    ///
    /// If it wasn't already in the graph, push it onto the `to_visit` stack.
    fn insert_instance(
        &mut self,
        tcx: TyCtxt<'tcx>,
        caller_def_id: DefId,
        instance: FunctionInstance<'tcx>,
    ) -> graph::NodeIndex {
        match self.instance_to_index.entry(instance) {
            indexmap::map::Entry::Occupied(n) => *n.get(),
            indexmap::map::Entry::Vacant(entry) => {
                let next_visit = if let Some(local) = instance.def_id.as_local() {
                    ToVisit::Local { function_def_id: local, generic_args: instance.generic_args }
                } else {
                    ToVisit::Extern {
                        caller_def_id,
                        function_def_id: instance.def_id,
                        generic_args: instance.generic_args,
                    }
                };
                let node = self.graph.add_node(instance);
                self.additional_data.insert(
                    node,
                    FunctionData {
                        is_pearlite: contracts_items::is_pearlite(tcx, instance.def_id),
                        has_variant: contracts_items::has_variant_clause(tcx, instance.def_id),
                        has_loops: None,
                    },
                );
                self.to_visit.push((next_visit, node));
                entry.insert(node);
                node
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ToVisit<'tcx> {
    /// The function is defined in the crate
    Local { function_def_id: LocalDefId, generic_args: &'tcx GenericArgs<'tcx> },
    /// The function is defined in another crate.
    ///
    /// We keep the generic parameters it was instantiated with, so that trait
    /// parameters can be specialized to specific instances.
    Extern { caller_def_id: DefId, function_def_id: DefId, generic_args: &'tcx GenericArgs<'tcx> },
}
impl<'tcx> ToVisit<'tcx> {
    fn def_id(&self) -> DefId {
        match self {
            ToVisit::Local { function_def_id, .. } => function_def_id.to_def_id(),
            ToVisit::Extern { function_def_id, .. } => *function_def_id,
        }
    }
}

impl<'tcx> CallGraph<'tcx> {
    /// Build the call graph of all functions appearing in the current crate,
    /// exclusively for the purpose of termination checking.
    ///
    /// In particular, this means it only contains `#[terminates]` functions.
    fn build(ctx: &mut TranslationCtx<'tcx>) -> Self {
        let mut build_call_graph = BuildFunctionsGraph::default();

        for d in ctx.hir().body_owners() {
            if !(contracts_items::is_pearlite(ctx.tcx, d.to_def_id())
                || contract_of(ctx, d.to_def_id()).terminates)
            {
                // Only consider functions marked with `terminates`: we already ensured that a `terminates` functions only calls other `terminates` functions.
            } else {
                let generic_args = GenericArgs::identity_for_item(ctx.tcx, d);
                let def_id = d.to_def_id();
                build_call_graph.insert_instance(
                    ctx.tcx,
                    def_id,
                    FunctionInstance { def_id, generic_args },
                );
            }
        }

        while let Some((visit, caller_node)) = build_call_graph.to_visit.pop() {
            let caller_def_id = visit.def_id();
            if is_trusted_function(ctx.tcx, caller_def_id)
                || contracts_items::is_no_translate(ctx.tcx, caller_def_id)
            {
                // FIXME: does this work with trait functions marked `#[terminates]`/`#[pure]` ?
                build_call_graph.additional_data[&caller_node] =
                    FunctionData { is_pearlite: false, has_variant: false, has_loops: None };
            } else {
                match visit {
                    // Function defined in the current crate: visit its body
                    ToVisit::Local { function_def_id: local_id, generic_args } => {
                        let caller_def_id = local_id.to_def_id();
                        let param_env = ctx.tcx.param_env(caller_def_id);
                        let tcx = ctx.tcx;
                        let (thir, expr) = ctx.thir_body(local_id).unwrap();
                        let thir = thir.borrow();
                        let mut visitor = FunctionCalls {
                            thir: &thir,
                            tcx,
                            generic_args,
                            param_env,
                            calls: IndexSet::new(),
                            has_loops: None,
                        };
                        <FunctionCalls as thir::visit::Visitor>::visit_expr(
                            &mut visitor,
                            &thir[expr],
                        );
                        let (visited_calls, has_loops) = (visitor.calls, visitor.has_loops);

                        for (function_def_id, span, subst) in visited_calls {
                            if !ctx.tcx.is_mir_available(function_def_id) {
                                continue;
                            }

                            let next_node = build_call_graph.insert_instance(
                                ctx.tcx,
                                caller_def_id,
                                FunctionInstance { def_id: function_def_id, generic_args: subst },
                            );

                            build_call_graph.graph.add_edge(caller_node, next_node, span);
                        }
                        build_call_graph.additional_data[&caller_node].has_loops = has_loops;
                    }
                    // Function defined in another crate: assume all the functions corresponding to its trait bounds can be called.
                    ToVisit::Extern { caller_def_id, function_def_id, generic_args } => {
                        if contracts_items::is_ghost_from_fn(ctx.tcx, function_def_id) {
                            // This is a `ghost!` call, so it needs special handling.
                            let &[_, ty] = generic_args.as_slice() else {
                                unreachable!();
                            };
                            let GenericArgKind::Type(ty) = ty.unpack() else { unreachable!() };
                            let TyKind::Closure(ghost_def_id, ghost_args_ty) = ty.kind() else {
                                unreachable!()
                            };
                            build_call_graph.insert_instance(
                                ctx.tcx,
                                caller_def_id,
                                FunctionInstance {
                                    def_id: *ghost_def_id,
                                    generic_args: ghost_args_ty,
                                },
                            );
                        } else {
                            for bound in ctx.tcx.param_env(function_def_id).caller_bounds() {
                                let Some(clause) = bound.as_trait_clause() else { continue };
                                let tcx = ctx.tcx;
                                let param_env = tcx.param_env(caller_def_id);
                                let subst = tcx
                                    .instantiate_bound_regions_with_erased(clause)
                                    .trait_ref
                                    .args;
                                let subst = EarlyBinder::bind(subst).instantiate(tcx, generic_args);

                                for &item_id in tcx
                                    .associated_item_def_ids(clause.skip_binder().trait_ref.def_id)
                                {
                                    if !tcx.def_kind(item_id).is_fn_like() {
                                        continue;
                                    }

                                    let TraitResolved::Instance(called_id, called_generic_args) =
                                        TraitResolved::resolve_item(tcx, param_env, item_id, subst)
                                    else {
                                        // We could not find a concrete function to call,
                                        // so we don't consider this to be an actual call: we cannot resolve it to any concrete function yet.
                                        continue;
                                    };

                                    let span = ctx.tcx.def_span(function_def_id);
                                    let next_node = build_call_graph.insert_instance(
                                        ctx.tcx,
                                        function_def_id,
                                        FunctionInstance {
                                            def_id: called_id,
                                            generic_args: called_generic_args,
                                        },
                                    );

                                    build_call_graph.graph.add_edge(caller_node, next_node, span);

                                    build_call_graph.additional_data[&next_node].has_variant =
                                        contracts_items::has_variant_clause(ctx.tcx, item_id);
                                }
                            }
                        }
                        // build_call_graph.additional_data[&caller_node] =
                        // FunctionData { is_pearlite: false, has_variant: true, has_loops: None };
                    }
                }
            }
        }

        Self { graph: build_call_graph.graph, additional_data: build_call_graph.additional_data }
    }
}

/// Gather the functions calls that appear in a particular function, and store them in `calls`.
struct FunctionCalls<'thir, 'tcx> {
    thir: &'thir thir::Thir<'tcx>,
    tcx: TyCtxt<'tcx>,
    /// Generic arguments that the function was intantiated with.
    generic_args: &'tcx GenericArgs<'tcx>,
    param_env: ParamEnv<'tcx>,
    /// Contains:
    /// - The id of the _called_ function
    /// - The span of the call (for error messages)
    /// - The generic arguments instantiating the call
    calls: IndexSet<(DefId, Span, &'tcx GenericArgs<'tcx>)>,
    /// `true` if the function contains a loop construct.
    has_loops: Option<Span>,
}

impl<'thir, 'tcx> thir::visit::Visitor<'thir, 'tcx> for FunctionCalls<'thir, 'tcx> {
    fn thir(&self) -> &'thir thir::Thir<'tcx> {
        self.thir
    }

    fn visit_expr(&mut self, expr: &'thir thir::Expr<'tcx>) {
        match expr.kind {
            thir::ExprKind::Call { fun, fn_span, .. } => {
                if let &FnDef(def_id, subst) = self.thir[fun].ty.kind() {
                    let subst = EarlyBinder::bind(self.tcx.erase_regions(subst))
                        .instantiate(self.tcx, self.generic_args);
                    let (def_id, subst) = if TraitResolved::is_trait_item(self.tcx, def_id) {
                        match TraitResolved::resolve_item(self.tcx, self.param_env, def_id, subst) {
                            TraitResolved::Instance(id, subst) => (id, subst),
                            _ => (def_id, subst),
                        }
                    } else {
                        (def_id, subst)
                    };
                    self.calls.insert((def_id, fn_span, subst));
                }
            }
            thir::ExprKind::Closure(box thir::ClosureExpr { closure_id, .. }) => {
                let (thir, expr) = self.tcx.thir_body(closure_id).unwrap_or_else(|_| {
                    crate::error::Error::from(crate::error::InternalError("Cannot fetch THIR body"))
                        .emit(self.tcx)
                });
                let thir = thir.borrow();

                let mut closure_visitor = FunctionCalls {
                    thir: &thir,
                    tcx: self.tcx,
                    generic_args: GenericArgs::identity_for_item(self.tcx, closure_id.to_def_id()),
                    param_env: self.param_env,
                    calls: std::mem::take(&mut self.calls),
                    has_loops: None,
                };
                thir::visit::walk_expr(&mut closure_visitor, &thir[expr]);
                self.calls = closure_visitor.calls;
            }
            thir::ExprKind::Loop { .. } => self.has_loops = Some(expr.span),
            _ => {}
        }
        thir::visit::walk_expr(self, expr);
    }
}
