//! The JSON a template's structure is written as, and its TypeScript declarations (`structure.d.ts`, generated from
//! these types by `tests/declarations.rs`).
//!
//! Positions are JavaScript string (UTF-16) offsets into the source the template was parsed from (its file, or the
//! component's file for an inline template), with 1-based lines and columns. A node that other nodes refer to (a
//! reference's owner, a variable's scope, an outlet's template) carries an `id`, unique within its template.

use serde::Serialize;
use ts_rs::TS;

/// Raised on any change a reader could notice: a field removed, renamed, or given another meaning.
pub const SCHEMA_VERSION: u32 = 1;

/// Where a node or an expression sits in its source.
#[derive(Debug, Clone, Copy, Serialize, TS)]
pub struct Position {
    /// UTF-16 offset of the first character
    pub start: u32,
    /// UTF-16 offset past the last character
    pub end: u32,
    /// 1-based line of `start`
    pub line: u32,
    /// 1-based column of `start`, in UTF-16 units
    pub column: u32,
}

/// The id of a node within its template.
pub type NodeId = u32;

/// One template: its nodes, or the errors that kept it from parsing (then `nodes` is empty).
#[derive(Debug, Serialize, TS)]
pub struct TemplateStructure {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub errors: Vec<TemplateError>,
}

#[derive(Debug, Serialize, TS)]
pub struct TemplateError {
    pub message: String,
    pub position: Position,
}

#[derive(Debug, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Node {
    Element(Element),
    /// An `<ng-template>`, or the template a structural directive (`*ngIf`) makes of the element it sits on
    Template(Template),
    If(IfBlock),
    For(ForBlock),
    Switch(SwitchBlock),
    Defer(DeferBlock),
    Let(LetDeclaration),
    Text(Text),
    BoundText(BoundText),
    Content(Content),
    Icu(Leaf),
    UnknownBlock(UnknownBlock),
    /// A selectorless component or directive reference (`<MyComp>`), which this schema does not describe further
    Selectorless(Leaf),
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub id: NodeId,
    pub position: Position,
    pub name: String,
    pub attrs: Vec<Attr>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub refs: Vec<Reference>,
    pub self_closing: bool,
    /// The directives and components matched on the element; absent where nothing bound the template
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matches: Option<Vec<DirectiveMatch>>,
    pub children: Vec<Node>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: NodeId,
    pub position: Position,
    /// `ng-template`, or the tag of the element a structural directive sits on
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tag: Option<String>,
    pub attrs: Vec<Attr>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub refs: Vec<Reference>,
    /// The structural directive's microsyntax (`*ngIf="x; else other"`: `ngIf`, `ngIfElse`)
    pub template_attrs: Vec<TemplateAttr>,
    /// `let-x="y"`, and the microsyntax's `let` and `as`
    pub vars: Vec<Variable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matches: Option<Vec<DirectiveMatch>>,
    pub children: Vec<Node>,
}

/// A static attribute.
#[derive(Debug, Serialize, TS)]
pub struct Attr {
    pub name: String,
    pub value: String,
    pub position: Position,
    /// The matched directive the attribute sets an input of
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub consumer: Option<Consumer>,
}

/// A bound input: `[name]`, `[attr.name]`, `[class.name]`, `[style.name]`, `[(name)]`.
#[derive(Debug, Serialize, TS)]
pub struct Input {
    pub name: String,
    pub binding: BindingKind,
    pub expr: Expr,
    pub position: Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub consumer: Option<Consumer>,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum BindingKind {
    Property,
    Attribute,
    Class,
    Style,
    TwoWay,
    Animation,
}

/// A bound event: `(name)`.
#[derive(Debug, Serialize, TS)]
pub struct Output {
    pub name: String,
    pub handler: Expr,
    pub position: Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub consumer: Option<Consumer>,
}

/// A template reference: `#name`, `#name="exportAs"`.
#[derive(Debug, Serialize, TS)]
pub struct Reference {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target: Option<ReferenceTarget>,
}

/// What a reference names.
#[derive(Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReferenceTarget {
    Element {
        node: NodeId,
    },
    Template {
        node: NodeId,
    },
    /// A directive matched on `node`, by its class name
    Directive {
        node: NodeId,
        directive: String,
    },
}

/// A microsyntax entry of a structural directive: a static `value` or a bound `expr`.
#[derive(Debug, Serialize, TS)]
pub struct TemplateAttr {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expr: Option<Expr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub consumer: Option<Consumer>,
}

/// A template variable: its name and what it is bound to (`$implicit`, `index`, …).
#[derive(Debug, Serialize, TS)]
pub struct Variable {
    pub name: String,
    pub value: String,
}

/// The directive a binding sets, and the class property it sets.
#[derive(Debug, Serialize, TS)]
pub struct Consumer {
    /// The directive's class name
    pub directive: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub property: Option<String>,
}

/// A directive or component matched on an element or a template.
#[derive(Debug, Serialize, TS)]
pub struct DirectiveMatch {
    /// The class name
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<String>,
    pub component: bool,
    /// The class's reference key: where it is declared
    pub reference: String,
}

#[derive(Debug, Serialize, TS)]
pub struct IfBlock {
    pub id: NodeId,
    pub position: Position,
    /// In order; a branch holds when its own condition does and no earlier branch's does
    pub branches: Vec<IfBranch>,
}

#[derive(Debug, Serialize, TS)]
pub struct IfBranch {
    pub id: NodeId,
    pub position: Position,
    /// None for `@else`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub condition: Option<Expr>,
    /// `@if (x; as y)`: `y`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alias: Option<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ForBlock {
    pub id: NodeId,
    pub position: Position,
    pub expr: Expr,
    pub item: String,
    pub track: Expr,
    /// `$index`, `$first`, … and their aliases
    pub context_vars: Vec<Variable>,
    pub children: Vec<Node>,
    /// `@empty`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub empty: Option<Vec<Node>>,
}

#[derive(Debug, Serialize, TS)]
pub struct SwitchBlock {
    pub id: NodeId,
    pub position: Position,
    pub expr: Expr,
    pub groups: Vec<SwitchGroup>,
}

/// Cases that share a body.
#[derive(Debug, Serialize, TS)]
pub struct SwitchGroup {
    /// Each `@case`'s value; null for `@default`
    pub cases: Vec<Option<Expr>>,
    pub children: Vec<Node>,
}

#[derive(Debug, Serialize, TS)]
pub struct DeferBlock {
    pub id: NodeId,
    pub position: Position,
    pub children: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub loading: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<Vec<Node>>,
}

#[derive(Debug, Serialize, TS)]
pub struct LetDeclaration {
    pub id: NodeId,
    pub position: Position,
    pub name: String,
    pub expr: Expr,
}

/// Static text; its whitespace as written.
#[derive(Debug, Serialize, TS)]
pub struct Text {
    pub position: Position,
    pub value: String,
}

/// Text with interpolations: `expr` is the interpolation.
#[derive(Debug, Serialize, TS)]
pub struct BoundText {
    pub position: Position,
    pub expr: Expr,
}

/// `<ng-content select="…">`.
#[derive(Debug, Serialize, TS)]
pub struct Content {
    pub position: Position,
    pub selector: String,
}

#[derive(Debug, Serialize, TS)]
pub struct UnknownBlock {
    pub position: Position,
    pub name: String,
}

#[derive(Debug, Serialize, TS)]
pub struct Leaf {
    pub position: Position,
}

/// A template expression: its source text, where it is, and its tree.
#[derive(Debug, Serialize, TS)]
pub struct Expr {
    pub source: String,
    pub position: Position,
    #[serde(flatten)]
    #[ts(flatten)]
    pub kind: ExprKind,
}

#[derive(Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExprKind {
    /// `name`, `receiver.name`, `receiver?.name` (`safe`). With no receiver, `target` says what template entity the
    /// name is (a reference, a variable, a `@let`) where a binder has resolved it; with no target it is the
    /// component's member.
    Read {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        receiver: Option<Box<Expr>>,
        safe: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        target: Option<ExprTarget>,
    },
    /// `receiver[key]`, `receiver?.[key]`
    Keyed {
        receiver: Box<Expr>,
        key: Box<Expr>,
        safe: bool,
    },
    /// `callee(args)`, `callee?.(args)`
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        safe: bool,
    },
    /// `!operand`
    Not {
        operand: Box<Expr>,
    },
    /// `+operand`, `-operand`
    Unary {
        operator: String,
        operand: Box<Expr>,
    },
    /// `left operator right`: `&&`, `||`, `??`, comparisons, arithmetic, `in`, assignments
    Binary {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// `condition ? then : otherwise`
    Conditional {
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// `input | name:args`
    Pipe {
        name: String,
        input: Box<Expr>,
        args: Vec<Expr>,
    },
    /// A string, number, boolean or null; `undefined` is `null` with `undefined: true`
    Literal {
        value: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        undefined: Option<bool>,
    },
    Array {
        items: Vec<Expr>,
    },
    /// `{ key: value }`; a spread entry has no key
    Object {
        entries: Vec<ObjectEntry>,
    },
    /// Text with `{{ }}`: `strings` around `exprs`
    Interpolation {
        strings: Vec<String>,
        exprs: Vec<Expr>,
    },
    /// `` `a${b}` ``, `` tag`a${b}` ``
    TemplateLiteral {
        strings: Vec<String>,
        exprs: Vec<Expr>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        tag: Option<Box<Expr>>,
    },
    Paren {
        operand: Box<Expr>,
    },
    Typeof {
        operand: Box<Expr>,
    },
    Void {
        operand: Box<Expr>,
    },
    /// `operand!`
    NonNull {
        operand: Box<Expr>,
    },
    Spread {
        operand: Box<Expr>,
    },
    /// `(params) => body`
    Arrow {
        params: Vec<String>,
        body: Box<Expr>,
    },
    /// `a; b`
    Chain {
        exprs: Vec<Expr>,
    },
    Regex {
        body: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        flags: Option<String>,
    },
    This,
    Empty,
}

#[derive(Debug, Serialize, TS)]
pub struct ObjectEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub key: Option<String>,
    pub value: Expr,
}

/// The template entity a name resolves to, by the node that declares it.
#[derive(Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExprTarget {
    /// `#name` on the element or template `node`
    Reference { node: NodeId },
    /// A variable of the template, `@for` block or `@if` branch `node`
    Variable { node: NodeId },
    /// The `@let` declaration `node`
    Let { node: NodeId },
}

/// A project's components, as `ngcheck --structure` writes them.
#[derive(Debug, Serialize, TS)]
pub struct ProjectStructure {
    pub version: u32,
    pub components: Vec<ComponentStructure>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStructure {
    /// The component's TypeScript file
    pub file: String,
    pub class_name: String,
    /// The class, in its file
    pub class_position: Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<String>,
    /// The selector, parsed: one entry per comma-separated part
    pub selectors: Vec<CssSelector>,
    pub standalone: bool,
    /// The file the template's positions index into: its `templateUrl`, or the component's file for an inline
    /// template
    pub template_file: String,
    pub inline_template: bool,
    /// The names of what the component imports
    pub imports: Vec<String>,
    pub inputs: Vec<ClassBinding>,
    pub outputs: Vec<ClassBinding>,
    pub host: Vec<HostBinding>,
    pub members: Vec<ClassMember>,
    pub template: TemplateStructure,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CssSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub element: Option<String>,
    /// `[name]`, `[name=value]`: name and value (empty for none)
    pub attrs: Vec<(String, String)>,
    pub classes: Vec<String>,
    /// `:not(…)`
    pub not: Vec<CssSelector>,
}

/// An input or an output: the class property and the name a template binds it by.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClassBinding {
    pub property: String,
    pub binding_name: String,
    pub required: bool,
}

/// A host binding or listener, from the `host` metadata or a `@HostBinding` / `@HostListener` member.
#[derive(Debug, Serialize, TS)]
pub struct HostBinding {
    pub kind: HostBindingKind,
    /// `attr.data-hint`, `class.active`, `click`, …
    pub name: String,
    /// The expression or handler, as written
    pub source: String,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum HostBindingKind {
    /// `'[name]': 'expr'`
    Property,
    /// `'(name)': 'handler'`
    Listener,
    /// `'name': 'value'`: a static attribute
    Attribute,
}

/// A member of the component class.
#[derive(Debug, Serialize, TS)]
pub struct ClassMember {
    pub name: String,
    pub kind: MemberKind,
    #[serde(rename = "static")]
    pub is_static: bool,
    /// The JSDoc written right before it, without its `/** */` and leading `*`s
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub doc: Option<String>,
    /// For a field initialized by a call, the callee: `input`, `input.required`, `computed`, `signal`, …
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub initializer: Option<String>,
    /// The names of its decorators: `Input`, `HostBinding`, …
    pub decorators: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MemberKind {
    Field,
    Getter,
    Setter,
    Method,
    Constructor,
}
