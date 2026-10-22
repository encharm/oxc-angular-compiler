//! `packages/compiler/src/typecheck/expression.ts`: the translation of a template expression into the text of a
//! TypeScript expression.
//!
//! Differences from the fork's expression AST, and how they are handled:
//! - `ASTWithSource` is not an `AngularExpression` variant: callers pass its `ast`.
//! - `Unary` has only the prefix `+` and `-` operators (no `++`/`--` update expressions), so `isPrefix` is always
//!   true.
//! - A literal map's property key has no source span: it is recovered from the template text
//!   (`literal_map_key_span`).

use oxc_angular_compiler::ast::expression::{
    AngularExpression, ArrowFunction, Binary, BinaryOperator, Call, Chain, Conditional, EmptyExpr, Interpolation,
    KeyedRead, LiteralArray, LiteralMap, LiteralMapKey, LiteralPrimitive, LiteralValue, NonNullAssert,
    ParenthesizedExpression, PrefixNot, PropertyRead, RegularExpressionLiteral, SafeCall, SafeKeyedRead,
    SafePropertyRead, SpreadElement, TaggedTemplateLiteral, TemplateLiteral, TypeofExpression, Unary, UnaryOperator,
    VoidExpression,
};
use oxc_syntax::number::ToJsString;

use crate::typecheck::api::TypeCheckingConfig;
use crate::typecheck::ops::codegen::{SpanInfo, TcbExpr};

/// The `maybeResolve` callback of `astToTcbExpr`
pub type MaybeResolve<'r, 'a> = dyn Fn(&'a AngularExpression<'a>) -> Option<TcbExpr> + 'r;

// expression.ts:49 astToTcbExpr
/// Convert an `AST` to a `TcbExpr` directly, without going through an intermediate `Expression` AST.
pub fn ast_to_tcb_expr<'a>(
    ast: &'a AngularExpression<'a>,
    maybe_resolve: &MaybeResolve<'_, 'a>,
    config: &TypeCheckingConfig,
) -> TcbExpr {
    let translator = TcbExprTranslator::new(maybe_resolve, config);
    translator.translate(ast)
}

// expression.ts:58 TcbExprTranslator
struct TcbExprTranslator<'r, 'a> {
    maybe_resolve: &'r MaybeResolve<'r, 'a>,
    config: &'r TypeCheckingConfig,
}

impl<'r, 'a> TcbExprTranslator<'r, 'a> {
    // expression.ts:59 constructor
    fn new(maybe_resolve: &'r MaybeResolve<'r, 'a>, config: &'r TypeCheckingConfig) -> Self {
        Self { maybe_resolve, config }
    }

    // expression.ts:64 translate
    fn translate(&self, ast: &'a AngularExpression<'a>) -> TcbExpr {
        // (`ASTWithSource` is not an expression in the fork's AST: callers pass its `ast`.)

        // First attempt to let any custom resolution logic provide a translation for the given node.
        if let Some(resolved) = (self.maybe_resolve)(ast) {
            return resolved;
        }

        self.visit(ast)
    }

    /// `ast.visit(this)`
    fn visit(&self, ast: &'a AngularExpression<'a>) -> TcbExpr {
        use AngularExpression as E;
        match ast {
            E::Unary(ast) => self.visit_unary(ast),
            E::Binary(ast) => self.visit_binary(ast),
            E::Chain(ast) => self.visit_chain(ast),
            E::Conditional(ast) => self.visit_conditional(ast),
            E::ImplicitReceiver(_) => self.visit_implicit_receiver(),
            E::ThisReceiver(_) => self.visit_this_receiver(),
            E::RegularExpressionLiteral(ast) => self.visit_regular_expression_literal(ast),
            E::Interpolation(ast) => self.visit_interpolation(ast),
            E::KeyedRead(ast) => self.visit_keyed_read(ast),
            E::LiteralArray(ast) => self.visit_literal_array(ast),
            E::LiteralMap(ast) => self.visit_literal_map(ast),
            E::LiteralPrimitive(ast) => self.visit_literal_primitive(ast),
            E::NonNullAssert(ast) => self.visit_non_null_assert(ast),
            E::BindingPipe(_) => self.visit_pipe(),
            E::PrefixNot(ast) => self.visit_prefix_not(ast),
            E::TypeofExpression(ast) => self.visit_typeof_expression(ast),
            E::VoidExpression(ast) => self.visit_void_expression(ast),
            E::PropertyRead(ast) => self.visit_property_read(ast),
            E::SafePropertyRead(ast) => self.visit_safe_property_read(ast),
            E::SafeKeyedRead(ast) => self.visit_safe_keyed_read(ast),
            E::Call(ast) => self.visit_call(ast),
            E::SafeCall(ast) => self.visit_safe_call(ast),
            E::TemplateLiteral(ast) => self.visit_template_literal(ast),
            E::TaggedTemplateLiteral(ast) => self.visit_tagged_template_literal(ast),
            E::ParenthesizedExpression(ast) => self.visit_parenthesized_expression(ast),
            E::SpreadElement(ast) => self.visit_spread_element(ast),
            E::Empty(ast) => self.visit_empty_expr(ast),
            E::ArrowFunction(ast) => self.visit_arrow_function(ast),
        }
    }

    // expression.ts:78 visitUnary
    fn visit_unary(&self, ast: &'a Unary<'a>) -> TcbExpr {
        // Note that we don't need to guard against sequences like `+ +a` collapsing into `++a`,
        // because `wrapForTypeChecker` always parenthesizes nested unary expressions.
        let expr = self.translate(&ast.expr).p();
        // (The fork's `Unary` is always a prefix operator.)
        let node = TcbExpr::new(format!("{}{expr}", unary_operator(ast.operator)));
        node.wrap_for_type_checker().add_parse_span_info(ast.source_span)
    }

    // expression.ts:86 visitBinary
    fn visit_binary(&self, ast: &'a Binary<'a>) -> TcbExpr {
        let lhs = self.translate(&ast.left);
        let rhs = self.translate(&ast.right);
        let lhs = lhs.wrap_for_type_checker();
        let rhs = rhs.wrap_for_type_checker();
        let expression = format!("{} {} {}", lhs.p(), ast.operation.as_str(), rhs.p());
        let node = TcbExpr::new(
            if ast.operation == BinaryOperator::NullishCoalescing || ast.operation == BinaryOperator::Power {
                format!("({expression})")
            } else {
                expression
            },
        );
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:99 visitChain
    fn visit_chain(&self, ast: &'a Chain<'a>) -> TcbExpr {
        let elements: Vec<String> = ast.expressions.iter().map(|expr| self.translate(expr).p()).collect();
        let node = TcbExpr::new(elements.join(", "));
        let node = node.wrap_for_type_checker();
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:107 visitConditional
    fn visit_conditional(&self, ast: &'a Conditional<'a>) -> TcbExpr {
        let cond_expr = self.translate(&ast.condition);
        let true_expr = self.translate(&ast.true_exp);
        // Wrap `falseExpr` in parens so that the trailing parse span info is not attributed to the
        // whole conditional.
        // In the following example, the last source span comment (5,6) could be seen as the
        // trailing comment for _either_ the whole conditional expression _or_ just the `falseExpr` that
        // is immediately before it:
        // `conditional /*1,2*/ ? trueExpr /*3,4*/ : falseExpr /*5,6*/`
        // This should be instead be `conditional /*1,2*/ ? trueExpr /*3,4*/ : (falseExpr /*5,6*/)`
        let false_expr = self.translate(&ast.false_exp).wrap_for_type_checker();
        TcbExpr::new(format!("({} ? {} : {})", cond_expr.p(), true_expr.p(), false_expr.p()))
            .add_parse_span_info(ast.source_span)
    }

    // expression.ts:124 visitImplicitReceiver
    fn visit_implicit_receiver(&self) -> TcbExpr {
        panic!("Method not implemented.");
    }

    // expression.ts:128 visitThisReceiver
    fn visit_this_receiver(&self) -> TcbExpr {
        panic!("Method not implemented.");
    }

    // expression.ts:132 visitRegularExpressionLiteral
    fn visit_regular_expression_literal(&self, ast: &'a RegularExpressionLiteral<'a>) -> TcbExpr {
        let flags = ast.flags.as_ref().map_or("", |flags| flags.as_str());
        let node = TcbExpr::new(format!("/{}/{flags}", ast.body));
        node.wrap_for_type_checker()
    }

    // expression.ts:138 visitInterpolation
    fn visit_interpolation(&self, ast: &'a Interpolation<'a>) -> TcbExpr {
        let exprs: Vec<String> = ast
            .expressions
            .iter()
            .map(|e| {
                let node = self.translate(e);
                let node = node.wrap_for_type_checker();
                node.p()
            })
            .collect();
        TcbExpr::new(format!("\"\" + {}", exprs.join(" + ")))
    }

    // expression.ts:147 visitKeyedRead
    fn visit_keyed_read(&self, ast: &'a KeyedRead<'a>) -> TcbExpr {
        let mut receiver = self.translate(&ast.receiver);
        if !self.is_strict_safe_navigation_chain(&ast.receiver) {
            receiver = receiver.wrap_for_type_checker();
        }
        let key = self.translate(&ast.key);
        TcbExpr::new(format!("{}[{}]", receiver.p(), key.p())).add_parse_span_info(ast.source_span)
    }

    // expression.ts:156 visitLiteralArray
    fn visit_literal_array(&self, ast: &'a LiteralArray<'a>) -> TcbExpr {
        let elements: Vec<TcbExpr> = ast.expressions.iter().map(|expr| self.translate(expr)).collect();
        let mut literal = format!("[{}]", elements.iter().map(TcbExpr::p).collect::<Vec<_>>().join(", "));

        if !self.config.strict_literal_types {
            literal = format!("({literal} as any)");
        }

        TcbExpr::new(literal).add_parse_span_info(ast.source_span)
    }

    // expression.ts:167 visitLiteralMap
    fn visit_literal_map(&self, ast: &'a LiteralMap<'a>) -> TcbExpr {
        let properties: Vec<String> = ast
            .keys
            .iter()
            .enumerate()
            .map(|(idx, key)| {
                let value = self.translate(&ast.values[idx]);

                match key {
                    LiteralMapKey::Property(key) => {
                        let key_node = TcbExpr::new(TcbExpr::quote_and_escape(&key.key));
                        let key_node = key_node.add_parse_span_info(literal_map_key_span(ast, idx));
                        format!("{}: {}", key_node.p(), value.p())
                    }
                    LiteralMapKey::Spread(_) => format!("...{}", value.p()),
                }
            })
            .collect();

        let mut literal = format!("{{ {} }}", properties.join(", "));

        if !self.config.strict_literal_types {
            literal = format!("{literal} as any");
        }

        let expression = TcbExpr::new(literal).add_parse_span_info(ast.source_span);
        expression.wrap_for_type_checker()
    }

    // expression.ts:192 visitLiteralPrimitive
    fn visit_literal_primitive(&self, ast: &'a LiteralPrimitive<'a>) -> TcbExpr {
        let node = match &ast.value {
            LiteralValue::Undefined => TcbExpr::new("undefined"),
            LiteralValue::Null => TcbExpr::new("null"),
            LiteralValue::String(value) => TcbExpr::new(TcbExpr::quote_and_escape(value)),
            LiteralValue::Number(value) => {
                if value.is_nan() {
                    TcbExpr::new("NaN")
                } else if !value.is_finite() {
                    TcbExpr::new(if *value > 0.0 { "Infinity" } else { "-Infinity" })
                } else {
                    TcbExpr::new(value.to_js_string())
                }
            }
            LiteralValue::Boolean(value) => TcbExpr::new(value.to_string()),
        };
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:217 visitNonNullAssert
    fn visit_non_null_assert(&self, ast: &'a NonNullAssert<'a>) -> TcbExpr {
        let expr = self.translate(&ast.expression).wrap_for_type_checker();
        TcbExpr::new(format!("{}!", expr.p())).add_parse_span_info(ast.source_span)
    }

    // expression.ts:222 visitPipe
    fn visit_pipe(&self) -> TcbExpr {
        panic!("Method not implemented.");
    }

    // expression.ts:226 visitPrefixNot
    fn visit_prefix_not(&self, ast: &'a PrefixNot<'a>) -> TcbExpr {
        let expression = self.translate(&ast.expression).wrap_for_type_checker();
        TcbExpr::new(format!("!{}", expression.p())).add_parse_span_info(ast.source_span)
    }

    // expression.ts:231 visitTypeofExpression
    fn visit_typeof_expression(&self, ast: &'a TypeofExpression<'a>) -> TcbExpr {
        let expression = self.translate(&ast.expression).wrap_for_type_checker();
        TcbExpr::new(format!("typeof {}", expression.p())).add_parse_span_info(ast.source_span)
    }

    // expression.ts:236 visitVoidExpression
    fn visit_void_expression(&self, ast: &'a VoidExpression<'a>) -> TcbExpr {
        let expression = self.translate(&ast.expression).wrap_for_type_checker();
        TcbExpr::new(format!("void {}", expression.p())).add_parse_span_info(ast.source_span)
    }

    // expression.ts:241 visitPropertyRead
    fn visit_property_read(&self, ast: &'a PropertyRead<'a>) -> TcbExpr {
        let mut receiver = self.translate(&ast.receiver);
        let is_safe_chain = self.is_strict_safe_navigation_chain(&ast.receiver);
        if !is_safe_chain {
            receiver = receiver.wrap_for_type_checker();
        }

        let node = TcbExpr::new(format!("{}.{}", receiver.p(), ast.name)).add_parse_span_info(ast.name_span);

        let result = if is_safe_chain { TcbExpr::new(node.p()) } else { node.wrap_for_type_checker() };
        result.add_parse_span_info(ast.source_span)
    }

    // expression.ts:259 visitSafePropertyRead
    fn visit_safe_property_read(&self, ast: &'a SafePropertyRead<'a>) -> TcbExpr {
        let receiver = self.translate(&ast.receiver).wrap_for_type_checker();
        let name = TcbExpr::new(ast.name.as_str()).add_parse_span_info(ast.name_span);

        let node = if self.config.strict_safe_navigation_types {
            TcbExpr::new(format!("{}?.{}", receiver.p(), name.p()))
        } else if VeSafeLhsInferenceBugDetector::ve_will_infer_any_for(VeSafeLhsTarget::SafePropertyRead(ast)) {
            TcbExpr::new(format!("({} as any).{}", receiver.p(), name.p()))
        } else {
            TcbExpr::new(format!("({}!.{} as any)", receiver.p(), name.p()))
        };
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:274 visitSafeKeyedRead
    fn visit_safe_keyed_read(&self, ast: &'a SafeKeyedRead<'a>) -> TcbExpr {
        let receiver = self.translate(&ast.receiver).wrap_for_type_checker();
        let key = self.translate(&ast.key);

        let node = if self.config.strict_safe_navigation_types {
            TcbExpr::new(format!("{}?.[{}]", receiver.p(), key.p()))
        } else if VeSafeLhsInferenceBugDetector::ve_will_infer_any_for(VeSafeLhsTarget::SafeKeyedRead(ast)) {
            TcbExpr::new(format!("({} as any)[{}]", receiver.p(), key.p()))
        } else {
            let element_access =
                TcbExpr::new(format!("{}![{}]", receiver.p(), key.p())).add_parse_span_info(ast.source_span);
            TcbExpr::new(format!("({} as any)", element_access.p()))
        };
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:292 visitCall
    fn visit_call(&self, ast: &'a Call<'a>) -> TcbExpr {
        let args: Vec<TcbExpr> = ast.args.iter().map(|expr| self.translate(expr)).collect();
        let receiver = &ast.receiver;

        let expr = if let AngularExpression::PropertyRead(property_read) = receiver {
            if let Some(resolved) = (self.maybe_resolve)(receiver) {
                resolved
            } else {
                let mut property_receiver = self.translate(&property_read.receiver);
                if !self.is_strict_safe_navigation_chain(&property_read.receiver) {
                    property_receiver = property_receiver.wrap_for_type_checker();
                }
                TcbExpr::new(format!("{}.{}", property_receiver.p(), property_read.name))
                    .add_parse_span_info(property_read.name_span)
            }
        } else {
            self.translate(receiver)
        };

        let node = if matches!(
            ast.receiver,
            AngularExpression::SafePropertyRead(_) | AngularExpression::SafeKeyedRead(_)
        ) {
            self.convert_to_safe_call(VeSafeLhsTarget::Call(ast), &expr, &args)
        } else {
            TcbExpr::new(format!("{}({})", expr.p(), args.iter().map(TcbExpr::p).collect::<Vec<_>>().join(", ")))
        };

        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:325 visitSafeCall
    fn visit_safe_call(&self, ast: &'a SafeCall<'a>) -> TcbExpr {
        let args: Vec<TcbExpr> = ast.args.iter().map(|expr| self.translate(expr)).collect();
        let expr = self.translate(&ast.receiver).wrap_for_type_checker();
        self.convert_to_safe_call(VeSafeLhsTarget::SafeCall(ast), &expr, &args).add_parse_span_info(ast.source_span)
    }

    // expression.ts:331 visitTemplateLiteral
    fn visit_template_literal(&self, ast: &'a TemplateLiteral<'a>) -> TcbExpr {
        let length = ast.elements.len();
        let head = &ast.elements[0];

        let result = if length == 1 {
            format!("`{}`", Self::escape_template_literal(&head.text))
        } else {
            let mut parts = vec![format!("`{}", Self::escape_template_literal(&head.text))];
            let tail_index = length - 1;

            for i in 1..tail_index {
                let expr = self.translate(&ast.expressions[i - 1]);
                parts.push(format!("${{{}}}{}", expr.p(), Self::escape_template_literal(&ast.elements[i].text)));
            }
            let resolved_expression = self.translate(&ast.expressions[tail_index - 1]);
            parts.push(format!(
                "${{{}}}{}`",
                resolved_expression.p(),
                Self::escape_template_literal(&ast.elements[tail_index].text)
            ));
            parts.join("")
        };
        TcbExpr::new(result)
    }

    // expression.ts:355 visitTemplateLiteralElement: never visited (the fork's template literal elements are not
    // expressions)

    // expression.ts:359 visitTaggedTemplateLiteral
    fn visit_tagged_template_literal(&self, ast: &'a TaggedTemplateLiteral<'a>) -> TcbExpr {
        let tag = self.translate(&ast.tag);
        let template = self.visit_template_literal(&ast.template);
        TcbExpr::new(format!("{}{}", tag.p(), template.p()))
    }

    // expression.ts:365 visitParenthesizedExpression
    fn visit_parenthesized_expression(&self, ast: &'a ParenthesizedExpression<'a>) -> TcbExpr {
        let expr = self.translate(&ast.expression);
        TcbExpr::new(format!("({})", expr.p()))
    }

    // expression.ts:370 visitSpreadElement
    fn visit_spread_element(&self, ast: &'a SpreadElement<'a>) -> TcbExpr {
        let expression = self.translate(&ast.expression);
        let expression = expression.wrap_for_type_checker();
        let node = TcbExpr::new(format!("...{}", expression.p()));
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:378 visitEmptyExpr
    fn visit_empty_expr(&self, ast: &'a EmptyExpr) -> TcbExpr {
        let node = TcbExpr::new("undefined");
        node.add_parse_span_info(ast.source_span)
    }

    // expression.ts:384 visitArrowFunction
    fn visit_arrow_function(&self, ast: &'a ArrowFunction<'a>) -> TcbExpr {
        let params = ast
            .parameters
            .iter()
            .map(|param| TcbExpr::new(param.name.as_str()).mark_ignore_diagnostics().p())
            .collect::<Vec<_>>()
            .join(", ");
        let body = ast_to_tcb_expr(
            &ast.body,
            &|inner_ast| {
                let AngularExpression::PropertyRead(inner) = inner_ast else {
                    return (self.maybe_resolve)(inner_ast);
                };
                if matches!(inner.receiver, AngularExpression::ThisReceiver(_))
                    || !matches!(inner.receiver, AngularExpression::ImplicitReceiver(_))
                {
                    return (self.maybe_resolve)(inner_ast);
                }

                let corresponding_param = ast.parameters.iter().find(|arg| arg.name == inner.name);

                if corresponding_param.is_some() {
                    let node = TcbExpr::new(inner.name.as_str());
                    return Some(node.add_parse_span_info(inner.source_span));
                }

                (self.maybe_resolve)(inner_ast)
            },
            self.config,
        );

        TcbExpr::new(format!(
            "{} => {}",
            if ast.parameters.len() == 1 { params } else { format!("({params})") },
            body.p()
        ))
    }

    // expression.ts:417 convertToSafeCall
    fn convert_to_safe_call(&self, ast: VeSafeLhsTarget<'a>, expr_node: &TcbExpr, arg_nodes: &[TcbExpr]) -> TcbExpr {
        let expr = expr_node.p();
        let args = arg_nodes.iter().map(TcbExpr::p).collect::<Vec<_>>().join(", ");

        if self.config.strict_safe_navigation_types {
            return TcbExpr::new(format!("{expr}?.({args})"));
        }

        if VeSafeLhsInferenceBugDetector::ve_will_infer_any_for(ast) {
            return TcbExpr::new(format!("({expr} as any)({args})"));
        }

        TcbExpr::new(format!("({expr}!({args}) as any)"))
    }

    // expression.ts:432 escapeTemplateLiteral
    fn escape_template_literal(value: &str) -> String {
        value.replace('\\', "\\\\").replace('`', "\\`").replace("${", "$\\{")
    }

    // expression.ts:436 isStrictSafeNavigationChain
    fn is_strict_safe_navigation_chain(&self, ast: &'a AngularExpression<'a>) -> bool {
        use AngularExpression as E;
        if !self.config.strict_safe_navigation_types {
            return false;
        }
        let mut current = ast;
        loop {
            match current {
                E::SafePropertyRead(_) | E::SafeKeyedRead(_) | E::SafeCall(_) => return true,
                E::PropertyRead(read) => current = &read.receiver,
                E::KeyedRead(read) => current = &read.receiver,
                E::Call(call) => current = &call.receiver,
                E::NonNullAssert(assert) => current = &assert.expression,
                _ => break,
            }
        }
        false
    }
}

/// `Unary.operator` (the fork's operators are the prefix `+` and `-`)
fn unary_operator(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Plus => "+",
        UnaryOperator::Minus => "-",
    }
}

/// The source span of the key of the `idx`th property of a literal map (`key.sourceSpan`), which the fork's AST does
/// not keep. Angular's parser spans the key's token: an identifier, keyword or quoted string. It is recovered from the
/// template text (`crate::span::with_current_text`): a shorthand property's key is its value's span; otherwise the key
/// token is the one before the `:` that precedes the value. Without the text, the value's span stands in.
fn literal_map_key_span(map: &LiteralMap<'_>, idx: usize) -> SpanInfo {
    let value_span = map.values[idx].source_span();
    let LiteralMapKey::Property(key) = &map.keys[idx] else {
        return value_span.into();
    };
    if key.is_shorthand_initialized {
        return value_span.into();
    }
    crate::span::with_current_text(|text| {
        text.and_then(|text| key_token_before_colon(text.as_bytes(), value_span.start as usize))
            .unwrap_or_else(|| value_span.into())
    })
}

/// The span of the token before the `:` that precedes `value_start` (skipping whitespace): a quoted string or an
/// identifier/keyword (Angular's lexer: ASCII letters, digits, `_` and `$`).
fn key_token_before_colon(text: &[u8], value_start: usize) -> Option<SpanInfo> {
    let mut i = value_start.min(text.len());
    while i > 0 && text[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || text[i - 1] != b':' {
        return None;
    }
    i -= 1;
    while i > 0 && text[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let end = i;
    if end == 0 {
        return None;
    }
    let quote = text[end - 1];
    let start = if quote == b'"' || quote == b'\'' {
        // The opening quote: the nearest one before the closing quote that is not escaped (preceded by an even
        // number of backslashes).
        let mut j = end - 1;
        loop {
            if j == 0 {
                return None;
            }
            j -= 1;
            if text[j] == quote {
                let backslashes = text[..j].iter().rev().take_while(|&&c| c == b'\\').count();
                if backslashes % 2 == 0 {
                    break j;
                }
            }
        }
    } else {
        let mut j = end;
        while j > 0 && (text[j - 1].is_ascii_alphanumeric() || text[j - 1] == b'_' || text[j - 1] == b'$') {
            j -= 1;
        }
        if j == end {
            return None;
        }
        j
    };
    Some(SpanInfo { start: start as u32, end: end as u32 })
}

/// The nodes `VeSafeLhsInferenceBugDetector.veWillInferAnyFor` takes: `Call | SafeCall | SafePropertyRead |
/// SafeKeyedRead`
#[derive(Clone, Copy)]
enum VeSafeLhsTarget<'a> {
    Call(&'a Call<'a>),
    SafeCall(&'a SafeCall<'a>),
    SafePropertyRead(&'a SafePropertyRead<'a>),
    SafeKeyedRead(&'a SafeKeyedRead<'a>),
}

// expression.ts:465 VeSafeLhsInferenceBugDetector
struct VeSafeLhsInferenceBugDetector;

impl VeSafeLhsInferenceBugDetector {
    // expression.ts:468 veWillInferAnyFor
    fn ve_will_infer_any_for(ast: VeSafeLhsTarget<'_>) -> bool {
        match ast {
            // `ast.visit(visitor)`: `visitCall`
            VeSafeLhsTarget::Call(_) => true,
            VeSafeLhsTarget::SafeCall(ast) => Self::visit(&ast.receiver),
            VeSafeLhsTarget::SafePropertyRead(ast) => Self::visit(&ast.receiver),
            VeSafeLhsTarget::SafeKeyedRead(ast) => Self::visit(&ast.receiver),
        }
    }

    /// `ast.visit(this)`, with the visitor methods of expression.ts:473-556
    fn visit(ast: &AngularExpression<'_>) -> bool {
        use AngularExpression as E;
        match ast {
            E::Unary(ast) => Self::visit(&ast.expr),
            E::Binary(ast) => Self::visit(&ast.left) || Self::visit(&ast.right),
            E::Chain(_) => false,
            E::Conditional(ast) => {
                Self::visit(&ast.condition) || Self::visit(&ast.true_exp) || Self::visit(&ast.false_exp)
            }
            E::Call(_) => true,
            E::SafeCall(_) => false,
            E::ImplicitReceiver(_) => false,
            E::ThisReceiver(_) => false,
            E::Interpolation(ast) => ast.expressions.iter().any(|exp| Self::visit(exp)),
            E::KeyedRead(_) => false,
            E::LiteralArray(_) => true,
            E::LiteralMap(_) => true,
            E::LiteralPrimitive(_) => false,
            E::BindingPipe(_) => true,
            E::PrefixNot(ast) => Self::visit(&ast.expression),
            E::TypeofExpression(ast) => Self::visit(&ast.expression),
            E::VoidExpression(ast) => Self::visit(&ast.expression),
            E::NonNullAssert(ast) => Self::visit(&ast.expression),
            E::PropertyRead(_) => false,
            E::SafePropertyRead(_) => false,
            E::SafeKeyedRead(_) => false,
            E::TemplateLiteral(_) => false,
            E::TaggedTemplateLiteral(_) => false,
            E::ParenthesizedExpression(ast) => Self::visit(&ast.expression),
            E::RegularExpressionLiteral(_) => false,
            E::SpreadElement(ast) => Self::visit(&ast.expression),
            E::ArrowFunction(_) => false,
            // The detector has no `visitEmptyExpr`: JavaScript throws calling the missing method.
            E::Empty(_) => panic!("visitor.visitEmptyExpr is not a function"),
        }
    }
}
