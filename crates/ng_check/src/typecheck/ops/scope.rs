//! `packages/compiler/src/typecheck/ops/scope.ts`
//!
//! Ownership (`ops/base.rs`): a scope is shared (`ScopeRef`); its op queue and maps are in `RefCell`s. The builders
//! that create ops (`append_node` and the rest) take the scope's `ScopeRef` (`this`), as the ops hold a clone of it.
//! Maps keyed by a template node in TypeScript are keyed by its address (`node_key`) and keep the node beside the
//! value where it is needed again.
//!
//! Angular 22's `BoundaryBlock` and `BoundaryErrorBlock` are not in the fork's AST: their code paths are noted where
//! they would be (`for_nodes`, `append_node`).

use std::cell::RefCell;
use std::rc::Rc;

use oxc_angular_compiler::ast::r3::{
    R3BoundAttribute, R3BoundEvent, R3Component, R3DeferredBlock, R3DeferredBlockTriggers,
    R3Directive, R3Element, R3HostElement, R3Icu, R3IcuPlaceholder, R3LetDeclaration, R3Node,
    R3Reference, R3Variable,
};
use oxc_angular_compiler::parser::html::split_ns_name;

use crate::angular::directive_matching::CssSelector;
use crate::angular::t2_api::{
    DirectiveOwner, ReferenceDeferredTrigger, ReferenceTarget, ScopedNode, TemplateEntity, node_key,
};
use crate::typecheck::api::{DiagnosticCategoryLabel, DirectiveKey, DirectiveRef, Map, Set};
use crate::typecheck::oob::{
    ComponentOrDirective, DirectiveBindingNode, DirectiveHost, ElementOrTemplate,
};
use crate::typecheck::ops::base::{BoxedOp, CircularFallback, ScopeRef, TcbOp, TcbRef};
use crate::typecheck::ops::codegen::TcbExpr;
use crate::typecheck::ops::completions::TcbComponentContextCompletionOp;
use crate::typecheck::ops::content_projection::{
    ElementOrComponent, TcbControlFlowContentProjectionOp,
};
use crate::typecheck::ops::directive_constructor::TcbDirectiveCtorOp;
use crate::typecheck::ops::directive_type::{
    TcbGenericDirectiveTypeWithAnyParamsOp, TcbNonGenericDirectiveTypeOp,
};
use crate::typecheck::ops::element::TcbElementOp;
use crate::typecheck::ops::events::{TcbDirectiveOutputsOp, TcbUnclaimedOutputsOp};
use crate::typecheck::ops::expression::{TcbConditionOp, TcbExpressionOp, tcb_expression};
use crate::typecheck::ops::for_block::TcbForOfOp;
use crate::typecheck::ops::host::TcbHostElementOp;
use crate::typecheck::ops::if_block::TcbIfBlockOp;
use crate::typecheck::ops::inputs::{TcbDirectiveInputsOp, TcbUnclaimedInputsOp};
use crate::typecheck::ops::intersection_observer::TcbIntersectionObserverOp;
use crate::typecheck::ops::r#let::TcbLetDeclarationOp;
use crate::typecheck::ops::references::{
    LocalSymbol, ReferenceOpTarget, TcbInvalidReferenceOp, TcbReferenceOp,
};
use crate::typecheck::ops::schema::TcbDomSchemaCheckerOp;
use crate::typecheck::ops::selectorless::TcbComponentNodeOp;
use crate::typecheck::ops::signal_forms::{
    CustomFormControlType, TcbNativeFieldOp, TcbNativeRadioButtonFieldOp,
    get_custom_field_directive_type, is_form_control, is_native_field,
};
use crate::typecheck::ops::switch_block::TcbSwitchOp;
use crate::typecheck::ops::template::{TcbTemplateBodyOp, TcbTemplateContextOp};
use crate::typecheck::ops::variables::{
    TcbBlockImplicitVariableOp, TcbBlockVariableOp, TcbTemplateVariableOp,
};

/// An entry of the op queue: `TcbOp | TcbExpr | null` (an op to execute, or the memoized result of one)
enum OpSlot<'a, 'b> {
    Op(BoxedOp<'a, 'b>),
    Result(Option<TcbExpr>),
}

/// A value of `varMap`: `number | TcbExpr` (the index of the variable's op, or a pre-resolved identifier)
enum VarSlot {
    OpIndex(usize),
    Expr(TcbExpr),
}

/// A value of `letDeclOpMap`: `{opIndex: number; node: LetDeclaration}`
#[derive(Clone, Copy)]
struct LetDeclOp<'a> {
    op_index: usize,
    node: &'a R3LetDeclaration<'a>,
}

// scope.ts:89 Scope
/// Local scope within the type check block for a particular template.
///
/// The top-level template and each nested `<ng-template>` have their own `Scope`, which exist in a
/// hierarchy. The structure of this hierarchy mirrors the syntactic scopes in the generated type
/// check block, where each nested template is encased in an `if` structure.
///
/// As a template's `TcbOp`s are executed in a given `Scope`, statements are added via
/// `addStatement()`. When this processing is complete, the `Scope` can be turned into a `ts.Block`
/// via `renderToBlock()`.
///
/// If a `TcbOp` requires the output of another, it can call `resolve()`.
pub struct Scope<'a, 'b> {
    /// A queue of operations which need to be performed to generate the TCB code for this scope.
    ///
    /// This array can contain either a `TcbOp` which has yet to be executed, or a `TcbExpr|null`
    /// representing the memoized result of executing the operation. As operations are executed, their
    /// results are written into the `opQueue`, overwriting the original operation.
    ///
    /// If an operation is in the process of being executed, it is temporarily overwritten here with
    /// `INFER_TYPE_FOR_CIRCULAR_OP_EXPR`. This way, if a cycle is encountered where an operation
    /// depends transitively on its own result, the inner operation will infer the least narrow type
    /// that fits instead. This has the same semantics as TypeScript itself when types are referenced
    /// circularly.
    op_queue: RefCell<Vec<OpSlot<'a, 'b>>>,

    /// A map of `Element`s to the index of their `TcbElementOp` in the `opQueue`
    element_op_map: RefCell<Map<usize, usize>>,

    /// A map of `HostElement`s to the index of their `TcbHostElementOp` in the `opQueue`
    host_element_op_map: RefCell<Map<usize, usize>>,

    /// A map of `Component`s to the index of their `TcbComponentNodeOp` in the `opQueue`
    component_node_op_map: RefCell<Map<usize, usize>>,

    /// A map of maps which tracks the index of `TcbDirectiveCtorOp`s in the `opQueue` for each
    /// directive on a `Element` or `Template` node.
    directive_op_map: RefCell<Map<DirectiveOwner<'a>, Map<DirectiveKey, usize>>>,

    /// A map of `Reference`s to the index of their `TcbReferenceOp` in the `opQueue`
    reference_op_map: RefCell<Map<usize, (&'a R3Reference<'a>, usize)>>,

    /// Map of immediately nested <ng-template>s (within this `Scope`) represented by `Template`
    /// nodes to the index of their `TcbTemplateContextOp`s in the `opQueue`.
    template_ctx_op_map: RefCell<Map<usize, usize>>,

    /// Map of variables declared on the template that created this `Scope` (represented by
    /// `Variable` nodes) to the index of their `TcbVariableOp`s in the `opQueue`, or to
    /// pre-resolved variable identifiers.
    var_map: RefCell<Map<usize, (&'a R3Variable<'a>, VarSlot)>>,

    /// A map of the names of `LetDeclaration`s to the index of their op in the `opQueue`.
    ///
    /// Assumes that there won't be duplicated `@let` declarations within the same scope.
    let_decl_op_map: RefCell<Map<String, LetDeclOp<'a>>>,

    /// Statements for this template.
    ///
    /// Executing the `TcbOp`s in the `opQueue` populates this array.
    statements: RefCell<Vec<TcbExpr>>,

    tcb: TcbRef<'a, 'b>,
    parent: Option<ScopeRef<'a, 'b>>,
    guard: Option<TcbExpr>,
}

impl<'a: 'b, 'b> Scope<'a, 'b> {
    // scope.ts:161 getForLoopContextVariableTypes
    /// Gets names of the for loop context variables and their types.
    fn get_for_loop_context_variable_types() -> Map<&'static str, &'static str> {
        [
            ("$first", "boolean"),
            ("$last", "boolean"),
            ("$even", "boolean"),
            ("$odd", "boolean"),
            ("$index", "number"),
            ("$count", "number"),
        ]
        .into_iter()
        .collect()
    }

    // scope.ts:172 constructor
    fn new(tcb: TcbRef<'a, 'b>, parent: Option<ScopeRef<'a, 'b>>, guard: Option<TcbExpr>) -> Self {
        Self {
            op_queue: RefCell::new(Vec::new()),
            element_op_map: RefCell::default(),
            host_element_op_map: RefCell::default(),
            component_node_op_map: RefCell::default(),
            directive_op_map: RefCell::default(),
            reference_op_map: RefCell::default(),
            template_ctx_op_map: RefCell::default(),
            var_map: RefCell::default(),
            let_decl_op_map: RefCell::default(),
            statements: RefCell::new(Vec::new()),
            tcb,
            parent,
            guard,
        }
    }

    // scope.ts:189 forNodes
    /// Constructs a `Scope` given either a `Template` or a list of `Node`s.
    ///
    /// `tcb`: the overall context of TCB generation. `parent_scope`: the `Scope` of the parent template (if any) or
    /// `None` if this is the root `Scope`. `scoped_node`: node that provides the scope around the child nodes (e.g. a
    /// `Template` node exposing variables to its children): a `Template`, `IfBlockBranch`, `ForLoopBlock` or
    /// `HostElement` (any other is ignored, as TypeScript's `instanceof` chain ignores it). `children`: child nodes
    /// that should be appended to the TCB. `guard`: an expression that is applied to this scope for type narrowing
    /// purposes.
    ///
    /// # Panics
    /// On an unknown `@for` context variable (TypeScript throws).
    pub fn for_nodes(
        tcb: &TcbRef<'a, 'b>,
        parent_scope: Option<&ScopeRef<'a, 'b>>,
        scoped_node: Option<ScopedNode<'a>>,
        children: Option<&'a [R3Node<'a>]>,
        guard: Option<TcbExpr>,
    ) -> ScopeRef<'a, 'b> {
        let scope = Rc::new(Scope::new(Rc::clone(tcb), parent_scope.map(Rc::clone), guard));

        if parent_scope.is_none() && tcb.env.config().enable_template_type_checker {
            // Add an autocompletion point for the component context.
            scope.push_op(TcbComponentContextCompletionOp::new(Rc::clone(&scope)));
        }

        // If given an actual `Template` instance, then process any additional information it
        // has.
        match scoped_node {
            Some(ScopedNode::Template(scoped_node)) => {
                // The template's variable declarations need to be added as `TcbVariableOp`s.
                let mut var_map: Map<&str, &'a R3Variable<'a>> = Map::default();

                for v in &scoped_node.variables {
                    // Validate that variables on the `Template` are only declared once.
                    if let Some(first_decl) = var_map.get(v.name.as_str()) {
                        tcb.oob_recorder.duplicate_template_var(&tcb.id, v, first_decl);
                    } else {
                        var_map.insert(v.name.as_str(), v);
                    }
                    Scope::register_variable(
                        &scope,
                        v,
                        Box::new(TcbTemplateVariableOp::new(
                            Rc::clone(tcb),
                            Rc::clone(&scope),
                            scoped_node,
                            v,
                        )),
                    );
                }
            }
            Some(ScopedNode::IfBlockBranch(scoped_node)) => {
                if let (Some(expression), Some(expression_alias)) =
                    (&scoped_node.expression, &scoped_node.expression_alias)
                {
                    Scope::register_variable(
                        &scope,
                        expression_alias,
                        Box::new(TcbBlockVariableOp::new(
                            Rc::clone(tcb),
                            Rc::clone(&scope),
                            tcb_expression(expression, tcb, &scope),
                            expression_alias,
                        )),
                    );
                }
            }
            // scope.ts:233 `BoundaryErrorBlock` (not in the fork's AST): each of its `contextVariables` is registered
            // with a `TcbBlockVariableOp` whose type is `(err as Error)` for `$error` and `(() => {})` for `$reset`
            // (any other throws `Unrecognized context variable ${variable.value}`).
            Some(ScopedNode::ForLoopBlock(scoped_node)) => {
                // Register the variable for the loop so it can be resolved by
                // children. It'll be declared once the loop is created.
                let loop_initializer = TcbExpr::new(tcb.allocate_id())
                    .add_parse_span_info(scoped_node.item.source_span);
                scope.var_map.borrow_mut().insert(
                    node_key(&scoped_node.item),
                    (&scoped_node.item, VarSlot::Expr(loop_initializer)),
                );

                let for_loop_context_variable_types = Scope::get_for_loop_context_variable_types();

                for variable in &scoped_node.context_variables {
                    let Some(r#type) = for_loop_context_variable_types.get(variable.value.as_str())
                    else {
                        panic!("Unrecognized for loop context variable {}", variable.name);
                    };

                    let r#type = TcbExpr::new(*r#type);
                    Scope::register_variable(
                        &scope,
                        variable,
                        Box::new(TcbBlockImplicitVariableOp::new(
                            Rc::clone(tcb),
                            Rc::clone(&scope),
                            r#type,
                            variable,
                        )),
                    );
                }
            }
            Some(ScopedNode::HostElement(scoped_node)) => {
                // `scope.appendNode(scopedNode)`
                Scope::append_host_element(&scope, scoped_node);
            }
            _ => {}
        }
        if let Some(children) = children {
            for node in children {
                Scope::append_node(&scope, node);
            }
        }
        // Once everything is registered, we need to check if there are `@let`
        // declarations that conflict with other local symbols defined after them.
        let variables: Vec<&'a R3Variable<'a>> =
            scope.var_map.borrow().values().map(|(v, _)| *v).collect();
        for variable in variables {
            Scope::check_conflicting_let(&scope, TemplateEntity::Variable(variable));
        }
        let references: Vec<&'a R3Reference<'a>> =
            scope.reference_op_map.borrow().values().map(|(r, _)| *r).collect();
        for r#ref in references {
            Scope::check_conflicting_let(&scope, TemplateEntity::Reference(r#ref));
        }
        scope
    }

    // scope.ts:290 registerVariable
    /// Registers a local variable with a scope.
    fn register_variable(scope: &Scope<'a, 'b>, variable: &'a R3Variable<'a>, op: BoxedOp<'a, 'b>) {
        let op_index = scope.push_boxed_op(op);
        scope
            .var_map
            .borrow_mut()
            .insert(node_key(variable), (variable, VarSlot::OpIndex(op_index)));
    }

    // scope.ts:315 resolve
    /// Look up a `ts.Expression` representing the value of some operation in the current `Scope`,
    /// including any parent scope(s). This method always returns a mutable clone of the
    /// `ts.Expression` with the comments cleared.
    ///
    /// `node`: a `Node` of the operation in question. The lookup performed will depend on
    /// the type of this node:
    ///
    /// Assuming `directive` is not present, then `resolve` will return:
    ///
    /// * `Element` - retrieve the expression for the element DOM node
    /// * `Template` - retrieve the template context variable
    /// * `Variable` - retrieve a template let- variable
    /// * `LetDeclaration` - retrieve a template `@let` declaration
    /// * `Reference` - retrieve variable created for the local ref
    ///
    /// `directive`: if present, a directive type on a `Element` or `Template` to
    /// look up instead of the default for an element or template node.
    ///
    /// # Panics
    /// When the node does not resolve in this scope or a parent (TypeScript throws).
    pub fn resolve(&self, node: LocalSymbol<'a>, directive: Option<&DirectiveRef>) -> TcbExpr {
        // Attempt to resolve the operation locally.
        if let Some(res) = self.resolve_local(node, directive) {
            res
        } else if let Some(parent) = &self.parent {
            // Check with the parent.
            parent.resolve(node, directive)
        } else {
            panic!("Could not resolve {node:?} / {:?}", directive.map(|directive| &directive.name));
        }
    }

    // scope.ts:331 addStatement
    /// Add a statement to this scope.
    pub fn add_statement(&self, stmt: TcbExpr) {
        self.statements.borrow_mut().push(stmt);
    }

    // scope.ts:338 render
    /// Get the statements.
    pub fn render(&self) -> Vec<TcbExpr> {
        let mut i = 0;
        while i < self.op_queue.borrow().len() {
            // Optional statements cannot be skipped when we are generating the TCB for use
            // by the TemplateTypeChecker.
            let skip_optional = !self.tcb.env.config().enable_template_type_checker;
            self.execute_op(i, skip_optional);
            i += 1;
        }
        self.statements.borrow().clone()
    }

    // scope.ts:352 guards
    /// Returns an expression of all template guards that apply to this scope, including those of
    /// parent scopes. If no guards have been applied, null is returned.
    pub fn guards(&self) -> Option<TcbExpr> {
        let mut parent_guards: Option<TcbExpr> = None;
        if let Some(parent) = &self.parent {
            // Start with the guards from the parent scope, if present.
            parent_guards = parent.guards();
        }

        match (&self.guard, parent_guards) {
            // This scope does not have a guard, so return the parent's guards as is.
            (None, parent_guards) => parent_guards,
            // There's no guards from the parent scope, so this scope's guard represents all available
            // guards.
            (Some(guard), None) => Some(guard.clone()),
            // Both the parent scope and this scope provide a guard, so create a combination of the two.
            // It is important that the parent guard is used as left operand, given that it may provide
            // narrowing that is required for this scope's guard to be valid.
            (Some(guard), Some(parent_guards)) => {
                Some(TcbExpr::new(format!("({}) && ({})", parent_guards.p(), guard.p())))
            }
        }
    }

    // scope.ts:376 isLocal
    /// Returns whether a template symbol is defined locally within the current scope.
    pub fn is_local(&self, node: TemplateEntity<'a>) -> bool {
        match node {
            TemplateEntity::Variable(node) => self.var_map.borrow().contains_key(&node_key(node)),
            TemplateEntity::LetDeclaration(node) => {
                self.let_decl_op_map.borrow().contains_key(node.name.as_str())
            }
            TemplateEntity::Reference(node) => {
                self.reference_op_map.borrow().contains_key(&node_key(node))
            }
        }
    }

    // scope.ts:397 createChildScope
    /// Constructs a `Scope` given either a `Template` or a list of `Node`s.
    /// This is identical to `Scope.forNodes` which we can't reference in some ops due to
    /// circular dependencies.
    pub fn create_child_scope(
        &self,
        parent_scope: &ScopeRef<'a, 'b>,
        scoped_node: Option<ScopedNode<'a>>,
        children: Option<&'a [R3Node<'a>]>,
        guard: Option<TcbExpr>,
    ) -> ScopeRef<'a, 'b> {
        Scope::for_nodes(&self.tcb, Some(parent_scope), scoped_node, children, guard)
    }

    // scope.ts:406 resolveLocal
    fn resolve_local(
        &self,
        r#ref: LocalSymbol<'a>,
        directive: Option<&DirectiveRef>,
    ) -> Option<TcbExpr> {
        let key = r#ref.key();
        if let LocalSymbol::Reference(_) = r#ref
            && let Some(&(_, op_index)) = self.reference_op_map.borrow().get(&key)
        {
            return Some(self.resolve_op(op_index));
        }
        if let LocalSymbol::LetDeclaration(node) = r#ref
            && let Some(let_decl) = self.let_decl_op_map.borrow().get(node.name.as_str()).copied()
        {
            return Some(self.resolve_op(let_decl.op_index));
        }
        if let LocalSymbol::Variable(_) = r#ref {
            // Resolving a context variable for this template.
            // Execute the `TcbVariableOp` associated with the `Variable`.
            let slot = match self.var_map.borrow().get(&key) {
                Some((_, VarSlot::OpIndex(op_index))) => Some(Ok(*op_index)),
                Some((_, VarSlot::Expr(expr))) => {
                    Some(Err(TcbExpr::new(expr.print(true /* ignoreComments */))))
                }
                None => None,
            };
            match slot {
                Some(Ok(op_index)) => return Some(self.resolve_op(op_index)),
                Some(Err(expr)) => return Some(expr),
                None => {}
            }
        }
        if let LocalSymbol::Template(_) = r#ref
            && directive.is_none()
            && let Some(op_index) = self.template_ctx_op_map.borrow().get(&key).copied()
        {
            // Resolving the context of the given sub-template.
            // Execute the `TcbTemplateContextOp` for the template.
            return Some(self.resolve_op(op_index));
        }
        if let (Some(owner), Some(directive)) = (r#ref.as_directive_owner(), directive) {
            let dir_index = {
                let directive_op_map = self.directive_op_map.borrow();
                directive_op_map
                    .get(&owner)
                    .map(|dir_map| dir_map.get(&DirectiveKey(Rc::clone(directive))).copied())
            };
            if let Some(dir_index) = dir_index {
                // Resolving a directive on an element or sub-template.
                return dir_index.map(|op_index| self.resolve_op(op_index));
            }
        }
        let op_index = match r#ref {
            // Resolving the DOM node of an element in this template.
            LocalSymbol::Element(_) => self.element_op_map.borrow().get(&key).copied(),
            LocalSymbol::Component(_) => self.component_node_op_map.borrow().get(&key).copied(),
            LocalSymbol::HostElement(_) => self.host_element_op_map.borrow().get(&key).copied(),
            _ => None,
        };
        op_index.map(|op_index| self.resolve_op(op_index))
    }

    // scope.ts:453 resolveOp
    /// Like `executeOp`, but assert that the operation actually returned `TcbExpr`.
    fn resolve_op(&self, op_index: usize) -> TcbExpr {
        let Some(res) = self.execute_op(op_index, /* skipOptional */ false) else {
            panic!("Error resolving operation, got null");
        };
        res
    }

    // scope.ts:468 executeOp
    /// Execute a particular `TcbOp` in the `opQueue`.
    ///
    /// This method replaces the operation in the `opQueue` with the result of execution (once done)
    /// and also protects against a circular dependency from the operation to itself by temporarily
    /// setting the operation's result to a special expression.
    fn execute_op(&self, op_index: usize, skip_optional: bool) -> Option<TcbExpr> {
        let op = {
            let mut op_queue = self.op_queue.borrow_mut();
            let op = match &op_queue[op_index] {
                OpSlot::Result(res) => {
                    return res
                        .as_ref()
                        .map(|op| TcbExpr::new(op.print(true /* ignoreComments */)));
                }
                OpSlot::Op(op) => op,
            };

            if skip_optional && op.optional() {
                return None;
            }

            // Set the result of the operation in the queue to its circular fallback. If executing this
            // operation results in a circular dependency, this will prevent an infinite loop and allow for
            // the resolution of such cycles.
            let fallback = match op.circular_fallback() {
                CircularFallback::Op(op) => OpSlot::Op(op),
                CircularFallback::Expr(expr) => OpSlot::Result(Some(expr)),
            };
            let OpSlot::Op(op) = std::mem::replace(&mut op_queue[op_index], fallback) else {
                unreachable!()
            };
            op
        };
        let mut res = op.execute();
        if let Some(expr) = &res {
            res = Some(TcbExpr::new(expr.print(true /* ignoreComments */)));
        }
        // Once the operation has finished executing, it's safe to cache the real result.
        self.op_queue.borrow_mut()[op_index] = OpSlot::Result(res.clone());
        res
    }

    /// `this.opQueue.push(op) - 1`
    fn push_op(&self, op: impl TcbOp<'a, 'b> + 'b) -> usize {
        self.push_boxed_op(Box::new(op))
    }

    /// `this.opQueue.push(op) - 1`
    fn push_boxed_op(&self, op: BoxedOp<'a, 'b>) -> usize {
        let mut op_queue = self.op_queue.borrow_mut();
        op_queue.push(OpSlot::Op(op));
        op_queue.len() - 1
    }

    // scope.ts:491 appendNode
    fn append_node(this: &ScopeRef<'a, 'b>, node: &'a R3Node<'a>) {
        let tcb = &this.tcb;
        let config = tcb.env.config();
        match node {
            R3Node::Element(node) => {
                let node: &'a R3Element<'a> = node;
                let op_index =
                    this.push_op(TcbElementOp::new(Rc::clone(tcb), Rc::clone(this), node));
                this.element_op_map.borrow_mut().insert(node_key(node), op_index);
                if config.control_flow_preventing_content_projection
                    != DiagnosticCategoryLabel::Suppress
                {
                    Scope::append_content_projection_check_op(
                        this,
                        ElementOrComponent::Element(node),
                    );
                }
                Scope::append_directives_and_inputs_of_element_like_node(
                    this,
                    DirectiveOwner::Element(node),
                );
                Scope::append_outputs_of_element_like_node(
                    this,
                    DirectiveOwner::Element(node),
                    Some(&node.inputs),
                    &node.outputs,
                );
                Scope::append_selectorless_directives(this, DirectiveOwner::Element(node));
                Scope::append_children(this, &node.children);
                Scope::check_and_append_references_of_node(this, DirectiveHost::Element(node));
            }
            R3Node::Template(node) => {
                let node = &**node;
                // Template children are rendered in a child scope.
                Scope::append_directives_and_inputs_of_element_like_node(
                    this,
                    DirectiveOwner::Template(node),
                );
                Scope::append_outputs_of_element_like_node(
                    this,
                    DirectiveOwner::Template(node),
                    Some(&node.inputs),
                    &node.outputs,
                );
                Scope::append_selectorless_directives(this, DirectiveOwner::Template(node));
                let ctx_index =
                    this.push_op(TcbTemplateContextOp::new(Rc::clone(tcb), Rc::clone(this)));
                this.template_ctx_op_map.borrow_mut().insert(node_key(node), ctx_index);
                if config.check_template_bodies {
                    this.push_op(TcbTemplateBodyOp::new(Rc::clone(tcb), Rc::clone(this), node));
                } else if config.always_check_schema_in_template_bodies {
                    Scope::append_deep_schema_checks(this, &node.children);
                }
                Scope::check_and_append_references_of_node(this, DirectiveHost::Template(node));
            }
            R3Node::Component(node) => Scope::append_component_node(this, node),
            R3Node::DeferredBlock(node) => Scope::append_deferred_block(this, node),
            R3Node::IfBlock(node) => {
                this.push_op(TcbIfBlockOp::new(Rc::clone(tcb), Rc::clone(this), node));
            }
            // scope.ts:522 `BoundaryBlock` (not in the fork's AST): `this.opQueue.push(new TcbBoundaryOp(...))`.
            R3Node::SwitchBlock(node) => {
                this.push_op(TcbSwitchOp::new(Rc::clone(tcb), Rc::clone(this), node));
            }
            R3Node::ForLoopBlock(node) => {
                this.push_op(TcbForOfOp::new(Rc::clone(tcb), Rc::clone(this), node));
                if let Some(empty) = &node.empty
                    && config.check_control_flow_bodies
                {
                    Scope::append_children(this, &empty.children);
                }
            }
            R3Node::BoundText(node) => {
                this.push_op(TcbExpressionOp::new(Rc::clone(tcb), Rc::clone(this), &node.value));
            }
            R3Node::Icu(node) => Scope::append_icu_expressions(this, node),
            R3Node::Content(node) => Scope::append_children(this, &node.children),
            R3Node::LetDeclaration(node) => {
                let node: &'a R3LetDeclaration<'a> = node;
                let op_index =
                    this.push_op(TcbLetDeclarationOp::new(Rc::clone(tcb), Rc::clone(this), node));
                if this.is_local(TemplateEntity::LetDeclaration(node)) {
                    tcb.oob_recorder.conflicting_declaration(&tcb.id, node);
                } else {
                    this.let_decl_op_map
                        .borrow_mut()
                        .insert(node.name.to_string(), LetDeclOp { op_index, node });
                }
            }
            R3Node::HostElement(node) => Scope::append_host_element(this, node),
            _ => {}
        }
    }

    // scope.ts:547 appendChildren
    fn append_children(this: &ScopeRef<'a, 'b>, children: &'a [R3Node<'a>]) {
        for child in children {
            Scope::append_node(this, child);
        }
    }

    // scope.ts:553 checkAndAppendReferencesOfNode
    fn check_and_append_references_of_node(this: &ScopeRef<'a, 'b>, node: DirectiveHost<'a>) {
        let tcb = &this.tcb;
        let references: &'a [R3Reference<'a>] = match node {
            DirectiveHost::Element(node) => &node.references,
            DirectiveHost::Template(node) => &node.references,
            DirectiveHost::Component(node) => &node.references,
            DirectiveHost::Directive(node) => &node.references,
        };
        for r#ref in references {
            let target = tcb.bound_target.get_reference_target(r#ref);

            let ctx_index = match target {
                None => {
                    // The reference is invalid if it doesn't have a target, so report it as an error.
                    tcb.oob_recorder.missing_reference_target(&tcb.id, r#ref);

                    // Any usages of the invalid reference will be resolved to a variable of type any.
                    this.push_op(TcbInvalidReferenceOp::new(Rc::clone(tcb), Rc::clone(this)))
                }
                Some(ReferenceTarget::Template(target)) => this.push_op(TcbReferenceOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    r#ref,
                    host_as_owner(node),
                    ReferenceOpTarget::Template(target),
                )),
                Some(ReferenceTarget::Element(target)) => this.push_op(TcbReferenceOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    r#ref,
                    host_as_owner(node),
                    ReferenceOpTarget::Element(target),
                )),
                Some(ReferenceTarget::Directive { directive, .. }) => {
                    this.push_op(TcbReferenceOp::new(
                        Rc::clone(tcb),
                        Rc::clone(this),
                        r#ref,
                        host_as_owner(node),
                        ReferenceOpTarget::Directive(directive),
                    ))
                }
            };
            this.reference_op_map.borrow_mut().insert(node_key(r#ref), (r#ref, ctx_index));
        }
    }

    // scope.ts:574 appendDirectivesAndInputsOfElementLikeNode
    /// `node`: an `Element` or `Template`
    fn append_directives_and_inputs_of_element_like_node(
        this: &ScopeRef<'a, 'b>,
        node: DirectiveOwner<'a>,
    ) {
        let tcb = &this.tcb;
        // Collect all the inputs on the element.
        let mut claimed_inputs: Set<String> = Set::default();

        // Don't resolve directives when selectorless is enabled and treat all the inputs on the element
        // as unclaimed. In selectorless the inputs are defined either in component or directive nodes.
        let directives = tcb.bound_target.get_directives_of_node(node);

        let Some(directives) = directives.filter(|directives| !directives.is_empty()) else {
            // If there are no directives, then all inputs are unclaimed inputs, so queue an operation
            // to add them if needed.
            if let DirectiveOwner::Element(element) = node {
                this.push_op(TcbUnclaimedInputsOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    &element.inputs,
                    LocalSymbol::Element(element),
                    Some(claimed_inputs.clone()),
                ));

                // Skip DOM schema checks for elements matched as foreign components.
                // An element can never match both an Angular directive and a foreign component
                // without throwing a fatal error, so we are guaranteed that directives is empty
                // and we only need to intercept in this directiveless block.
                let is_foreign = tcb.bound_target.get_foreign_component(element).is_some();
                if !is_foreign {
                    this.push_op(TcbDomSchemaCheckerOp::new(
                        Rc::clone(tcb),
                        node,
                        /* checkElement */ true,
                        Some(claimed_inputs),
                    ));
                }
            }
            return;
        };

        let host = owner_as_host(node);
        Scope::report_conflicting_bindings(this, host);

        let element_or_template = match node {
            DirectiveOwner::Element(node) => Some(ElementOrTemplate::Element(node)),
            DirectiveOwner::Template(node) => Some(ElementOrTemplate::Template(node)),
            _ => None,
        };
        if let Some(element_or_template) = element_or_template {
            let enclosing_blocks = tcb.bound_target.get_defer_blocks_of_node(node);
            let is_deferred = !enclosing_blocks.is_empty();

            for dir_meta in &directives {
                if !dir_meta.is_explicitly_deferred {
                    continue;
                }

                if !is_deferred {
                    // This node has directives/components that were defer-loaded (included into
                    // `@Component.deferredImports`), but the node itself was used outside of a
                    // `@defer` block, which is the error.
                    tcb.oob_recorder.deferred_component_used_eagerly(
                        &tcb.id,
                        element_or_template,
                        dir_meta,
                        None,
                        None,
                    );
                } else if let Some(deferred_blocks) = &dir_meta.deferred_blocks {
                    // `b.definedName !== null && dirMeta.deferredBlocks.has(b.definedName)`: the fork's `@defer`
                    // blocks have no name (`definedName` is always null).
                    let is_allowed_in_block = false;
                    if !is_allowed_in_block {
                        // `enclosingBlocks[enclosingBlocks.length - 1].definedName ?? 'unnamed'`
                        let current_block_name = "unnamed";
                        let declared_blocks: Vec<String> =
                            deferred_blocks.iter().cloned().collect();
                        tcb.oob_recorder.deferred_component_used_eagerly(
                            &tcb.id,
                            element_or_template,
                            dir_meta,
                            Some(current_block_name),
                            Some(&declared_blocks),
                        );
                    }
                }
            }
        }

        if let DirectiveOwner::Element(element) = node {
            let matched_components: Vec<&DirectiveRef> =
                directives.iter().filter(|dir| dir.is_component).collect();
            if matched_components.len() > 1 {
                tcb.oob_recorder.multiple_matching_components(
                    &tcb.id,
                    element,
                    &matched_components.iter().map(|dir| dir.name.clone()).collect::<Vec<_>>(),
                );
            }
        }

        let mut dir_map: Map<DirectiveKey, usize> = Map::default();
        for (i, dir) in directives.iter().enumerate() {
            Scope::append_directive_inputs(this, dir, host, &mut dir_map, &directives, Some(i));
        }
        this.directive_op_map.borrow_mut().insert(node, dir_map);

        // After expanding the directives, we might need to queue an operation to check any unclaimed
        // inputs.
        if let DirectiveOwner::Element(element) = node {
            // Go through the directives and remove any inputs that it claims from `elementInputs`.
            for dir in &directives {
                for property_name in dir.inputs.property_names() {
                    claimed_inputs.insert(property_name);
                }
            }

            this.push_op(TcbUnclaimedInputsOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                &element.inputs,
                LocalSymbol::Element(element),
                Some(claimed_inputs.clone()),
            ));
            // Check the element against the DOM schema unless it is matched by an Angular component
            // or by a directive that explicitly targets the element's tag name (such as `<router-outlet>`).
            // Directives that only match attributes (like `[formControl]`) do not claim the element name.
            let check_element = this.requires_unknown_element_check(element, Some(&directives));
            this.push_op(TcbDomSchemaCheckerOp::new(
                Rc::clone(tcb),
                node,
                check_element,
                Some(claimed_inputs),
            ));
        }
    }

    // scope.ts:677 appendOutputsOfElementLikeNode
    /// `node`: an `Element`, `Template` or `HostElement`
    fn append_outputs_of_element_like_node(
        this: &ScopeRef<'a, 'b>,
        node: DirectiveOwner<'a>,
        bindings: Option<&'a [R3BoundAttribute<'a>]>,
        events: &'a [R3BoundEvent<'a>],
    ) {
        let tcb = &this.tcb;
        // Collect all the outputs on the element.
        let mut claimed_outputs: Set<String> = Set::default();

        // Don't resolve directives when selectorless is enabled and treat all the outputs on the
        // element as unclaimed. In selectorless the outputs are defined either in component or
        // directive nodes.
        let directives = tcb.bound_target.get_directives_of_node(node);

        let Some(directives) = directives.filter(|directives| !directives.is_empty()) else {
            // If there are no directives, then all outputs are unclaimed outputs, so queue an operation
            // to add them if needed.
            if let DirectiveOwner::Element(element) = node {
                this.push_op(TcbUnclaimedOutputsOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    LocalSymbol::Element(element),
                    events,
                    bindings,
                    Some(claimed_outputs),
                    false,
                    false,
                ));
            }
            return;
        };

        // Queue operations for all directives to check the relevant outputs for a directive.
        for dir in &directives {
            this.push_op(TcbDirectiveOutputsOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                node,
                bindings,
                events,
                Rc::clone(dir),
            ));
        }

        // After expanding the directives, we might need to queue an operation to check any unclaimed
        // outputs.
        if matches!(node, DirectiveOwner::Element(_) | DirectiveOwner::HostElement(_)) {
            // Go through the directives and register any outputs that it claims in `claimedOutputs`.
            for dir in &directives {
                for output_property in dir.outputs.property_names() {
                    claimed_outputs.insert(output_property);
                }
            }

            this.push_op(TcbUnclaimedOutputsOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                LocalSymbol::from(node),
                events,
                bindings,
                Some(claimed_outputs),
                /* hasDirectives */ true,
                /* hasComponent */ directives.iter().any(|dir| dir.is_component),
            ));
        }
    }

    // scope.ts:731 appendInputsOfSelectorlessNode
    /// `node`: a `Component` or `Directive`
    fn append_inputs_of_selectorless_node(this: &ScopeRef<'a, 'b>, node: DirectiveHost<'a>) {
        let tcb = &this.tcb;
        let owner = host_as_owner(node);
        // Only resolve the directives that were brought in by this specific directive.
        let directives = tcb.bound_target.get_directives_of_node(owner);
        let mut claimed_inputs: Set<String> = Set::default();

        if let Some(directives) = directives.filter(|directives| !directives.is_empty()) {
            let mut dir_map: Map<DirectiveKey, usize> = Map::default();
            for (i, dir) in directives.iter().enumerate() {
                Scope::append_directive_inputs(this, dir, node, &mut dir_map, &directives, Some(i));

                for property_name in dir.inputs.property_names() {
                    claimed_inputs.insert(property_name);
                }
            }
            this.directive_op_map.borrow_mut().insert(owner, dir_map);
        }

        Scope::report_conflicting_bindings(this, node);

        // In selectorless all directive inputs have to be claimed.
        match node {
            DirectiveHost::Directive(directive) => {
                for input in &directive.inputs {
                    if !claimed_inputs.contains(input.name.as_str()) {
                        tcb.oob_recorder.unclaimed_directive_binding(
                            &tcb.id,
                            directive,
                            DirectiveBindingNode::BoundAttribute(input),
                        );
                    }
                }

                for attr in &directive.attributes {
                    if !claimed_inputs.contains(attr.name.as_str()) {
                        tcb.oob_recorder.unclaimed_directive_binding(
                            &tcb.id,
                            directive,
                            DirectiveBindingNode::TextAttribute(attr),
                        );
                    }
                }
            }
            DirectiveHost::Component(component) => {
                let check_element = component.tag_name.is_some();
                this.push_op(TcbUnclaimedInputsOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    &component.inputs,
                    LocalSymbol::Component(component),
                    Some(claimed_inputs.clone()),
                ));
                this.push_op(TcbDomSchemaCheckerOp::new(
                    Rc::clone(tcb),
                    owner,
                    check_element,
                    Some(claimed_inputs),
                ));
            }
            DirectiveHost::Element(_) | DirectiveHost::Template(_) => {
                unreachable!("a Component or Directive")
            }
        }
    }

    // scope.ts:773 appendOutputsOfSelectorlessNode
    /// `node`: a `Component` or `Directive`
    fn append_outputs_of_selectorless_node(this: &ScopeRef<'a, 'b>, node: DirectiveHost<'a>) {
        let tcb = &this.tcb;
        let owner = host_as_owner(node);
        let (inputs, outputs): (&'a [R3BoundAttribute<'a>], &'a [R3BoundEvent<'a>]) = match node {
            DirectiveHost::Component(node) => (&node.inputs, &node.outputs),
            DirectiveHost::Directive(node) => (&node.inputs, &node.outputs),
            DirectiveHost::Element(_) | DirectiveHost::Template(_) => {
                unreachable!("a Component or Directive")
            }
        };
        // Only resolve the directives that were brought in by this specific directive.
        let directives = tcb.bound_target.get_directives_of_node(owner);
        let mut claimed_outputs: Set<String> = Set::default();

        if let Some(directives) = directives.filter(|directives| !directives.is_empty()) {
            for dir in &directives {
                this.push_op(TcbDirectiveOutputsOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    owner,
                    Some(inputs),
                    outputs,
                    Rc::clone(dir),
                ));

                for output_property in dir.outputs.property_names() {
                    claimed_outputs.insert(output_property);
                }
            }
        }

        // In selectorless all directive outputs have to be claimed.
        if let DirectiveHost::Directive(directive) = node {
            for output in &directive.outputs {
                if !claimed_outputs.contains(output.name.as_str()) {
                    tcb.oob_recorder.unclaimed_directive_binding(
                        &tcb.id,
                        directive,
                        DirectiveBindingNode::BoundEvent(output),
                    );
                }
            }
        } else {
            this.push_op(TcbUnclaimedOutputsOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                LocalSymbol::from(owner),
                outputs,
                Some(inputs),
                Some(claimed_outputs),
                false,
                false,
            ));
        }
    }

    // scope.ts:804 appendDirectiveInputs
    fn append_directive_inputs(
        this: &ScopeRef<'a, 'b>,
        dir: &DirectiveRef,
        node: DirectiveHost<'a>,
        dir_map: &mut Map<DirectiveKey, usize>,
        all_directive_matches: &[DirectiveRef],
        directive_index: Option<usize>,
    ) {
        let tcb = &this.tcb;
        let node_is_form_control = is_form_control(all_directive_matches);
        let custom_form_control_type =
            if node_is_form_control { get_custom_field_directive_type(dir) } else { None };

        let directive_op = Scope::get_directive_op(
            this,
            dir,
            host_as_owner(node),
            custom_form_control_type,
            directive_index,
        );
        let dir_index = this.push_boxed_op(directive_op);
        dir_map.insert(DirectiveKey(Rc::clone(dir)), dir_index);

        if is_native_field(dir, node, all_directive_matches) {
            let DirectiveHost::Element(element) = node else {
                unreachable!("isNativeField: `node is Element`");
            };
            // `(node.name === 'input' && node.attributes.find((attr) => attr.name === 'type')?.value) || null`
            let input_type: Option<&'a str> = if element.name == "input" {
                element
                    .attributes
                    .iter()
                    .find(|attr| attr.name == "type")
                    .map(|attr| attr.value.as_str())
                    .filter(|value| !value.is_empty())
            } else {
                None
            };

            if input_type == Some("radio") {
                this.push_op(TcbNativeRadioButtonFieldOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    element,
                ));
            } else {
                this.push_op(TcbNativeFieldOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    element,
                    input_type,
                ));
            }
        }

        this.push_op(TcbDirectiveInputsOp::new(
            Rc::clone(tcb),
            Rc::clone(this),
            node,
            Rc::clone(dir),
            node_is_form_control,
            custom_form_control_type,
        ));
    }

    // scope.ts:835 getDirectiveOp
    fn get_directive_op(
        this: &ScopeRef<'a, 'b>,
        dir: &DirectiveRef,
        node: DirectiveOwner<'a>,
        custom_field_type: Option<CustomFormControlType>,
        directive_index: Option<usize>,
    ) -> BoxedOp<'a, 'b> {
        let tcb = &this.tcb;
        if !dir.is_generic {
            // The most common case is that when a directive is not generic, we use the normal
            // `TcbNonDirectiveTypeOp`.
            return Box::new(TcbNonGenericDirectiveTypeOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                node,
                Rc::clone(dir),
                directive_index,
            ));
        } else if !dir.requires_inline_type_ctor || tcb.env.config().use_inline_type_constructors {
            // For generic directives, we use a type constructor to infer types. If a directive requires
            // an inline type constructor, then inlining must be available to use the
            // `TcbDirectiveCtorOp`. If not we, we fallback to using `any` – see below.
            return Box::new(TcbDirectiveCtorOp::new(
                Rc::clone(tcb),
                Rc::clone(this),
                node,
                Rc::clone(dir),
                custom_field_type,
                directive_index,
            ));
        }

        // If inlining is not available, then we give up on inferring the generic params, and use
        // `any` type for the directive's generic parameters.
        Box::new(TcbGenericDirectiveTypeWithAnyParamsOp::new(
            Rc::clone(tcb),
            Rc::clone(this),
            node,
            Rc::clone(dir),
            directive_index,
        ))
    }

    // scope.ts:857 appendSelectorlessDirectives
    /// `node`: an `Element`, `Template` or `Component`
    fn append_selectorless_directives(this: &ScopeRef<'a, 'b>, node: DirectiveOwner<'a>) {
        let tcb = &this.tcb;
        let node_directives: &'a [R3Directive<'a>] = match node {
            DirectiveOwner::Element(node) => &node.directives,
            DirectiveOwner::Template(node) => &node.directives,
            DirectiveOwner::Component(node) => &node.directives,
            DirectiveOwner::Directive(_) | DirectiveOwner::HostElement(_) => {
                unreachable!("an Element, Template or Component")
            }
        };
        for directive in node_directives {
            // Check that the directive exists.
            if !tcb.bound_target.referenced_directive_exists(&directive.name) {
                tcb.oob_recorder.missing_named_template_dependency(
                    &tcb.id,
                    ComponentOrDirective::Directive(directive),
                );
                continue;
            }

            // Check that the class is a directive class.
            let directives =
                tcb.bound_target.get_directives_of_node(DirectiveOwner::Directive(directive));
            if directives.as_ref().is_none_or(|directives| {
                directives.is_empty()
                    || directives.iter().any(|dir| dir.is_component || !dir.is_standalone)
            }) {
                tcb.oob_recorder.incorrect_template_dependency_type(
                    &tcb.id,
                    ComponentOrDirective::Directive(directive),
                );
                continue;
            }

            Scope::append_inputs_of_selectorless_node(this, DirectiveHost::Directive(directive));
            Scope::append_outputs_of_selectorless_node(this, DirectiveHost::Directive(directive));
            Scope::check_and_append_references_of_node(this, DirectiveHost::Directive(directive));
        }
    }

    // scope.ts:882 appendDeepSchemaChecks
    fn append_deep_schema_checks(this: &ScopeRef<'a, 'b>, nodes: &'a [R3Node<'a>]) {
        let tcb = &this.tcb;
        for node in nodes {
            let children: &'a [R3Node<'a>] = match node {
                R3Node::Element(node) => &node.children,
                R3Node::Template(node) => &node.children,
                _ => continue,
            };

            if let R3Node::Element(node) = node {
                let node: &'a R3Element<'a> = node;
                let mut claimed_inputs: Set<String> = Set::default();
                // (TypeScript pushes into the array the binder returns, which is the binder's own list of the
                // element's directives; here it is a copy.)
                let mut directives =
                    tcb.bound_target.get_directives_of_node(DirectiveOwner::Element(node));

                for dir_node in &node.directives {
                    let directive_results = tcb
                        .bound_target
                        .get_directives_of_node(DirectiveOwner::Directive(dir_node));

                    if let Some(directive_results) =
                        directive_results.filter(|results| !results.is_empty())
                    {
                        directives.get_or_insert_with(Vec::new).extend(directive_results);
                    }
                }

                if let Some(directives) =
                    directives.as_ref().filter(|directives| !directives.is_empty())
                {
                    for dir in directives {
                        for property_name in dir.inputs.property_names() {
                            claimed_inputs.insert(property_name);
                        }
                    }
                }
                let is_foreign = tcb.bound_target.get_foreign_component(node).is_some();
                if !is_foreign {
                    let check_element =
                        this.requires_unknown_element_check(node, directives.as_deref());
                    this.push_op(TcbDomSchemaCheckerOp::new(
                        Rc::clone(tcb),
                        DirectiveOwner::Element(node),
                        check_element,
                        Some(claimed_inputs),
                    ));
                }
            }

            Scope::append_deep_schema_checks(this, children);
        }
    }

    // scope.ts:919 appendIcuExpressions
    fn append_icu_expressions(this: &ScopeRef<'a, 'b>, node: &'a R3Icu<'a>) {
        let tcb = &this.tcb;
        for (_, variable) in &node.vars {
            this.push_op(TcbExpressionOp::new(Rc::clone(tcb), Rc::clone(this), &variable.value));
        }
        for (_, placeholder) in &node.placeholders {
            if let R3IcuPlaceholder::BoundText(placeholder) = placeholder {
                this.push_op(TcbExpressionOp::new(
                    Rc::clone(tcb),
                    Rc::clone(this),
                    &placeholder.value,
                ));
            }
        }
    }

    // scope.ts:930 appendContentProjectionCheckOp
    fn append_content_projection_check_op(this: &ScopeRef<'a, 'b>, root: ElementOrComponent<'a>) {
        let tcb = &this.tcb;
        let owner = match root {
            ElementOrComponent::Element(node) => DirectiveOwner::Element(node),
            ElementOrComponent::Component(node) => DirectiveOwner::Component(node),
        };
        let meta = tcb
            .bound_target
            .get_directives_of_node(owner)
            .and_then(|directives| directives.into_iter().find(|meta| meta.is_component));

        if let Some(meta) = meta
            && let Some(selectors) =
                meta.ng_content_selectors.as_ref().filter(|selectors| !selectors.is_empty())
        {
            // We don't need to generate anything for components that don't have projection
            // slots, or they only have one catch-all slot (represented by `*`).
            if selectors.len() > 1 || (selectors.len() == 1 && selectors[0] != "*") {
                this.push_op(TcbControlFlowContentProjectionOp::new(
                    Rc::clone(tcb),
                    root,
                    selectors.clone(),
                    meta.name.clone(),
                ));
            }
        }
    }

    // scope.ts:947 appendComponentNode
    fn append_component_node(this: &ScopeRef<'a, 'b>, node: &'a R3Component<'a>) {
        let tcb = &this.tcb;
        // TODO(crisbeto): should we still append the children if the component is invalid?
        // Check that the referenced class exists.
        if !tcb.bound_target.referenced_directive_exists(&node.component_name) {
            tcb.oob_recorder
                .missing_named_template_dependency(&tcb.id, ComponentOrDirective::Component(node));
            return;
        }

        // Check that the class is a component.
        let directives = tcb.bound_target.get_directives_of_node(DirectiveOwner::Component(node));
        if directives.as_ref().is_none_or(|directives| {
            directives.is_empty()
                || directives.iter().all(|dir| !dir.is_component || !dir.is_standalone)
        }) {
            tcb.oob_recorder
                .incorrect_template_dependency_type(&tcb.id, ComponentOrDirective::Component(node));
            return;
        }

        let op_index = this.push_op(TcbComponentNodeOp::new(Rc::clone(tcb), Rc::clone(this), node));
        this.component_node_op_map.borrow_mut().insert(node_key(node), op_index);
        if tcb.env.config().control_flow_preventing_content_projection
            != DiagnosticCategoryLabel::Suppress
        {
            Scope::append_content_projection_check_op(this, ElementOrComponent::Component(node));
        }
        Scope::append_inputs_of_selectorless_node(this, DirectiveHost::Component(node));
        Scope::append_outputs_of_selectorless_node(this, DirectiveHost::Component(node));
        Scope::append_selectorless_directives(this, DirectiveOwner::Component(node));
        Scope::append_children(this, &node.children);
        Scope::check_and_append_references_of_node(this, DirectiveHost::Component(node));
    }

    // scope.ts:978 appendDeferredBlock
    fn append_deferred_block(this: &ScopeRef<'a, 'b>, block: &'a R3DeferredBlock<'a>) {
        Scope::append_deferred_triggers(this, block, &block.triggers);
        Scope::append_deferred_triggers(this, block, &block.prefetch_triggers);

        // Only the `when` hydration trigger needs to be checked.
        if let Some(when) = &block.hydrate_triggers.when {
            this.push_op(TcbConditionOp::new(Rc::clone(&this.tcb), Rc::clone(this), &when.value));
        }

        Scope::append_children(this, &block.children);

        if let Some(placeholder) = &block.placeholder {
            Scope::append_children(this, &placeholder.children);
        }

        if let Some(loading) = &block.loading {
            Scope::append_children(this, &loading.children);
        }

        if let Some(error) = &block.error {
            Scope::append_children(this, &error.children);
        }
    }

    // scope.ts:1002 appendDeferredTriggers
    fn append_deferred_triggers(
        this: &ScopeRef<'a, 'b>,
        block: &'a R3DeferredBlock<'a>,
        triggers: &'a R3DeferredBlockTriggers<'a>,
    ) {
        let tcb = &this.tcb;
        if let Some(when) = &triggers.when {
            this.push_op(TcbConditionOp::new(Rc::clone(tcb), Rc::clone(this), &when.value));
        }

        if let Some(viewport) = &triggers.viewport
            && let Some(options) = &viewport.options
        {
            this.push_op(TcbIntersectionObserverOp::new(Rc::clone(tcb), Rc::clone(this), options));
        }

        if let Some(hover) = &triggers.hover {
            this.validate_reference_based_deferred_trigger(
                block,
                ReferenceDeferredTrigger::Hover(hover),
            );
        }

        if let Some(interaction) = &triggers.interaction {
            this.validate_reference_based_deferred_trigger(
                block,
                ReferenceDeferredTrigger::Interaction(interaction),
            );
        }

        if let Some(viewport) = &triggers.viewport {
            this.validate_reference_based_deferred_trigger(
                block,
                ReferenceDeferredTrigger::Viewport(viewport),
            );
        }
    }

    // scope.ts:1024 appendHostElement
    fn append_host_element(this: &ScopeRef<'a, 'b>, node: &'a R3HostElement<'a>) {
        let tcb = &this.tcb;
        let op_index = this.push_op(TcbHostElementOp::new(Rc::clone(tcb), Rc::clone(this), node));
        let directives = tcb.bound_target.get_directives_of_node(DirectiveOwner::HostElement(node));

        if let Some(directives) = directives.filter(|directives| !directives.is_empty()) {
            let mut directive_op_map: Map<DirectiveKey, usize> = Map::default();

            for directive in &directives {
                let directive_op = Scope::get_directive_op(
                    this,
                    directive,
                    DirectiveOwner::HostElement(node),
                    None,
                    None,
                );
                directive_op_map
                    .insert(DirectiveKey(Rc::clone(directive)), this.push_boxed_op(directive_op));
            }

            this.directive_op_map
                .borrow_mut()
                .insert(DirectiveOwner::HostElement(node), directive_op_map);
        }

        this.host_element_op_map.borrow_mut().insert(node_key(node), op_index);
        this.push_op(TcbUnclaimedInputsOp::new(
            Rc::clone(tcb),
            Rc::clone(this),
            &node.bindings,
            LocalSymbol::HostElement(node),
            None,
        ));
        this.push_op(TcbDomSchemaCheckerOp::new(
            Rc::clone(tcb),
            DirectiveOwner::HostElement(node),
            false,
            None,
        ));
        Scope::append_outputs_of_element_like_node(
            this,
            DirectiveOwner::HostElement(node),
            None,
            &node.listeners,
        );
    }

    // scope.ts:1047 validateReferenceBasedDeferredTrigger
    fn validate_reference_based_deferred_trigger(
        &self,
        block: &'a R3DeferredBlock<'a>,
        trigger: ReferenceDeferredTrigger<'a>,
    ) {
        let tcb = &self.tcb;
        if trigger.reference().is_none() {
            let Some(placeholder) = &block.placeholder else {
                tcb.oob_recorder.defer_implicit_trigger_missing_placeholder(&tcb.id, trigger);
                return;
            };

            let mut root_node: Option<&'a R3Node<'a>> = None;

            for child in &placeholder.children {
                // Skip over empty text nodes if the host doesn't preserve whitespaces.
                if !tcb.host_preserve_whitespaces
                    && let R3Node::Text(text) = child
                    && js_trim(&text.value).is_empty()
                {
                    continue;
                }

                // Capture the first root node.
                if root_node.is_none() {
                    root_node = Some(child);
                } else {
                    // More than one root node is invalid. Reset it and break
                    // the loop so the assertion below can flag it.
                    root_node = None;
                    break;
                }
            }

            if !matches!(root_node, Some(R3Node::Element(_))) {
                tcb.oob_recorder.defer_implicit_trigger_invalid_placeholder(&tcb.id, trigger);
            }
            return;
        }

        if tcb.bound_target.get_deferred_trigger_target(block, trigger).is_none() {
            tcb.oob_recorder.inaccessible_deferred_trigger_element(&tcb.id, trigger);
        }
    }

    // scope.ts:1092 checkConflictingLet
    /// Reports a diagnostic if there are any `@let` declarations that conflict with a node.
    /// `node`: a `Variable` or `Reference`
    fn check_conflicting_let(scope: &Scope<'a, 'b>, node: TemplateEntity<'a>) {
        let name = match node {
            TemplateEntity::Variable(node) => node.name.as_str(),
            TemplateEntity::Reference(node) => node.name.as_str(),
            TemplateEntity::LetDeclaration(node) => node.name.as_str(),
        };
        let let_decl = scope.let_decl_op_map.borrow().get(name).copied();
        if let Some(let_decl) = let_decl {
            scope.tcb.oob_recorder.conflicting_declaration(&scope.tcb.id, let_decl.node);
        }
    }

    // scope.ts:1101 reportConflictingBindings
    fn report_conflicting_bindings(this: &ScopeRef<'a, 'b>, node: DirectiveHost<'a>) {
        let tcb = &this.tcb;
        let conflicting_bindings =
            tcb.bound_target.get_conflicting_host_directive_bindings(host_as_owner(node));

        if let Some(conflicting_bindings) = conflicting_bindings {
            for binding in conflicting_bindings {
                tcb.oob_recorder.conflicting_host_directive_binding(
                    &tcb.id,
                    node,
                    &binding.directive.name,
                    binding.kind,
                    &binding.class_property_name,
                    &binding.conflicting_aliases.iter().cloned().collect::<Vec<_>>(),
                );
            }
        }
    }

    // scope.ts:1123 requiresUnknownElementCheck
    /// Checks the element against the DOM schema unless it is matched by an Angular component
    /// or by a directive that explicitly targets the element's tag name (such as `<router-outlet>`).
    /// Directives that only match attributes (like `[formControl]`) do not claim the element name.
    fn requires_unknown_element_check(
        &self,
        node: &'a R3Element<'a>,
        directives: Option<&[DirectiveRef]>,
    ) -> bool {
        let Some(directives) = directives.filter(|directives| !directives.is_empty()) else {
            return true;
        };

        if !self.tcb.env.config().check_unknown_elements {
            return false;
        }

        let has_component = directives.iter().any(|dir| dir.is_component);
        let has_directive_matching_tag =
            directives.iter().any(|dir| matches_tag_name(dir.selector.as_deref(), &node.name));
        !has_component && !has_directive_matching_tag
    }
}

// scope.ts:1143 matchesTagName
fn matches_tag_name(selector: Option<&str>, tag_name: &str) -> bool {
    let Some(selector) = selector else {
        return false;
    };
    // `splitNsName(tagName, false)[1]` (the fork's `split_ns_name` does not throw)
    let lower_tag = split_ns_name(tag_name).1.to_lowercase();
    // `try { CssSelector.parse(selector) ... } catch { return false; }`
    match CssSelector::try_parse(selector) {
        Ok(selectors) => selectors.iter().any(|sel| {
            sel.element.as_ref().is_some_and(|element| element.to_lowercase() == lower_tag)
        }),
        Err(_) => false,
    }
}

/// `node` (`Element | Template | Component | Directive`) as a binder node (`DirectiveOwner`)
fn host_as_owner(node: DirectiveHost<'_>) -> DirectiveOwner<'_> {
    match node {
        DirectiveHost::Element(n) => DirectiveOwner::Element(n),
        DirectiveHost::Template(n) => DirectiveOwner::Template(n),
        DirectiveHost::Component(n) => DirectiveOwner::Component(n),
        DirectiveHost::Directive(n) => DirectiveOwner::Directive(n),
    }
}

/// A binder node that is not a `HostElement` as an `Element | Template | Component | Directive`
fn owner_as_host(node: DirectiveOwner<'_>) -> DirectiveHost<'_> {
    match node {
        DirectiveOwner::Element(n) => DirectiveHost::Element(n),
        DirectiveOwner::Template(n) => DirectiveHost::Template(n),
        DirectiveOwner::Component(n) => DirectiveHost::Component(n),
        DirectiveOwner::Directive(n) => DirectiveHost::Directive(n),
        DirectiveOwner::HostElement(_) => unreachable!("not a HostElement"),
    }
}

/// JavaScript's `String.prototype.trim`: white space and line terminators, which include U+FEFF (Rust's `trim` does
/// not).
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}
