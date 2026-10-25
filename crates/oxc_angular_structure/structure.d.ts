// Generated from crates/oxc_angular_structure/src/schema.rs by its tests/declarations.rs: do not edit.
// Schema version 1.

export type NodeId = number;

export type ProjectStructure = { version: number, components: Array<ComponentStructure>, };

export type ComponentStructure = { 
/**
 * The component's TypeScript file
 */
file: string, className: string, 
/**
 * The class, in its file
 */
classPosition: Position, selector?: string, 
/**
 * The selector, parsed: one entry per comma-separated part
 */
selectors: Array<CssSelector>, standalone: boolean, 
/**
 * The file the template's positions index into: its `templateUrl`, or the component's file for an inline
 * template
 */
templateFile: string, inlineTemplate: boolean, 
/**
 * The names of what the component imports
 */
imports: Array<string>, inputs: Array<ClassBinding>, outputs: Array<ClassBinding>, host: Array<HostBinding>, members: Array<ClassMember>, template: TemplateStructure, };

export type CssSelector = { element?: string, 
/**
 * `[name]`, `[name=value]`: name and value (empty for none)
 */
attrs: Array<[string, string]>, classes: Array<string>, 
/**
 * `:not(…)`
 */
not: Array<CssSelector>, };

export type ClassBinding = { property: string, bindingName: string, required: boolean, };

export type HostBinding = { kind: HostBindingKind, 
/**
 * `attr.data-hint`, `class.active`, `click`, …
 */
name: string, 
/**
 * The expression or handler, as written
 */
source: string, };

export type HostBindingKind = "property" | "listener" | "attribute";

export type ClassMember = { name: string, kind: MemberKind, static: boolean, 
/**
 * The JSDoc written right before it: its text, without the comment's delimiters and each line's leading asterisk
 */
doc?: string, 
/**
 * For a field initialized by a call, the callee: `input`, `input.required`, `computed`, `signal`, …
 */
initializer?: string, 
/**
 * The names of its decorators: `Input`, `HostBinding`, …
 */
decorators: Array<string>, };

export type MemberKind = "field" | "getter" | "setter" | "method" | "constructor";

export type TemplateStructure = { version: number, nodes: Array<Node>, errors: Array<TemplateError>, };

export type TemplateError = { message: string, position: Position, };

export type Position = { 
/**
 * UTF-16 offset of the first character
 */
start: number, 
/**
 * UTF-16 offset past the last character
 */
end: number, 
/**
 * 1-based line of `start`
 */
line: number, 
/**
 * 1-based column of `start`, in UTF-16 units
 */
column: number, };

export type Node = { "type": "element" } & Element | { "type": "template" } & Template | { "type": "if" } & IfBlock | { "type": "for" } & ForBlock | { "type": "switch" } & SwitchBlock | { "type": "defer" } & DeferBlock | { "type": "let" } & LetDeclaration | { "type": "text" } & Text | { "type": "boundText" } & BoundText | { "type": "content" } & Content | { "type": "icu" } & Leaf | { "type": "unknownBlock" } & UnknownBlock | { "type": "selectorless" } & Leaf;

export type Element = { id: number, position: Position, name: string, attrs: Array<Attr>, inputs: Array<Input>, outputs: Array<Output>, refs: Array<Reference>, selfClosing: boolean, 
/**
 * The directives and components matched on the element; absent where nothing bound the template
 */
matches?: Array<DirectiveMatch>, children: Array<Node>, };

export type Template = { id: number, position: Position, 
/**
 * `ng-template`, or the tag of the element a structural directive sits on
 */
tag?: string, attrs: Array<Attr>, inputs: Array<Input>, outputs: Array<Output>, refs: Array<Reference>, 
/**
 * The structural directive's microsyntax (`*ngIf="x; else other"`: `ngIf`, `ngIfElse`)
 */
templateAttrs: Array<TemplateAttr>, 
/**
 * `let-x="y"`, and the microsyntax's `let` and `as`
 */
vars: Array<Variable>, matches?: Array<DirectiveMatch>, children: Array<Node>, };

export type Attr = { name: string, value: string, position: Position, 
/**
 * The matched directive the attribute sets an input of
 */
consumer?: Consumer, };

export type Input = { name: string, binding: BindingKind, expr: Expr, position: Position, consumer?: Consumer, };

export type BindingKind = "property" | "attribute" | "class" | "style" | "twoWay" | "animation";

export type Output = { name: string, handler: Expr, position: Position, consumer?: Consumer, };

export type Reference = { name: string, value: string, target?: ReferenceTarget, };

export type ReferenceTarget = { "kind": "element", node: number, } | { "kind": "template", node: number, } | { "kind": "directive", node: number, directive: string, };

export type TemplateAttr = { name: string, value?: string, expr?: Expr, consumer?: Consumer, };

export type Variable = { name: string, value: string, };

export type Consumer = { 
/**
 * The directive's class name
 */
directive: string, property?: string, };

export type DirectiveMatch = { 
/**
 * The class name
 */
name: string, selector?: string, component: boolean, 
/**
 * The class's reference key: where it is declared
 */
reference: string, };

export type IfBlock = { id: number, position: Position, 
/**
 * In order; a branch holds when its own condition does and no earlier branch's does
 */
branches: Array<IfBranch>, };

export type IfBranch = { id: number, position: Position, 
/**
 * None for `@else`
 */
condition?: Expr, 
/**
 * `@if (x; as y)`: `y`
 */
alias?: string, children: Array<Node>, };

export type ForBlock = { id: number, position: Position, expr: Expr, item: string, track: Expr, 
/**
 * `$index`, `$first`, … and their aliases
 */
contextVars: Array<Variable>, children: Array<Node>, 
/**
 * `@empty`
 */
empty?: Array<Node>, };

export type SwitchBlock = { id: number, position: Position, expr: Expr, groups: Array<SwitchGroup>, };

export type SwitchGroup = { 
/**
 * Each `@case`'s value; null for `@default`
 */
cases: Array<Expr | null>, children: Array<Node>, };

export type DeferBlock = { id: number, position: Position, children: Array<Node>, placeholder?: Array<Node>, loading?: Array<Node>, error?: Array<Node>, };

export type LetDeclaration = { id: number, position: Position, name: string, expr: Expr, };

export type Text = { position: Position, value: string, };

export type BoundText = { position: Position, expr: Expr, };

export type Content = { position: Position, selector: string, };

export type UnknownBlock = { position: Position, name: string, };

export type Leaf = { position: Position, };

export type Expr = { source: string, position: Position, } & ({ "kind": "read", name: string, receiver?: Expr, safe: boolean, target?: ExprTarget, } | { "kind": "keyed", receiver: Expr, key: Expr, safe: boolean, } | { "kind": "call", callee: Expr, args: Array<Expr>, safe: boolean, } | { "kind": "not", operand: Expr, } | { "kind": "unary", operator: string, operand: Expr, } | { "kind": "binary", operator: string, left: Expr, right: Expr, } | { "kind": "conditional", condition: Expr, then: Expr, otherwise: Expr, } | { "kind": "pipe", name: string, input: Expr, args: Array<Expr>, } | { "kind": "literal", value: JsonValue, undefined?: boolean, } | { "kind": "array", items: Array<Expr>, } | { "kind": "object", entries: Array<ObjectEntry>, } | { "kind": "interpolation", strings: Array<string>, exprs: Array<Expr>, } | { "kind": "templateLiteral", strings: Array<string>, exprs: Array<Expr>, tag?: Expr, } | { "kind": "paren", operand: Expr, } | { "kind": "typeof", operand: Expr, } | { "kind": "void", operand: Expr, } | { "kind": "nonNull", operand: Expr, } | { "kind": "spread", operand: Expr, } | { "kind": "arrow", params: Array<string>, body: Expr, } | { "kind": "chain", exprs: Array<Expr>, } | { "kind": "regex", body: string, flags?: string, } | { "kind": "this" } | { "kind": "empty" });

export type ExprKind = { "kind": "read", name: string, receiver?: Expr, safe: boolean, target?: ExprTarget, } | { "kind": "keyed", receiver: Expr, key: Expr, safe: boolean, } | { "kind": "call", callee: Expr, args: Array<Expr>, safe: boolean, } | { "kind": "not", operand: Expr, } | { "kind": "unary", operator: string, operand: Expr, } | { "kind": "binary", operator: string, left: Expr, right: Expr, } | { "kind": "conditional", condition: Expr, then: Expr, otherwise: Expr, } | { "kind": "pipe", name: string, input: Expr, args: Array<Expr>, } | { "kind": "literal", value: JsonValue, undefined?: boolean, } | { "kind": "array", items: Array<Expr>, } | { "kind": "object", entries: Array<ObjectEntry>, } | { "kind": "interpolation", strings: Array<string>, exprs: Array<Expr>, } | { "kind": "templateLiteral", strings: Array<string>, exprs: Array<Expr>, tag?: Expr, } | { "kind": "paren", operand: Expr, } | { "kind": "typeof", operand: Expr, } | { "kind": "void", operand: Expr, } | { "kind": "nonNull", operand: Expr, } | { "kind": "spread", operand: Expr, } | { "kind": "arrow", params: Array<string>, body: Expr, } | { "kind": "chain", exprs: Array<Expr>, } | { "kind": "regex", body: string, flags?: string, } | { "kind": "this" } | { "kind": "empty" };

export type ObjectEntry = { key?: string, value: Expr, };

export type ExprTarget = { "kind": "reference", node: number, } | { "kind": "variable", node: number, } | { "kind": "let", node: number, };

export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;
