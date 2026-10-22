//! `packages/compiler/src/schema/dom_security_schema.ts`
//!
//! The fork's `oxc_angular_compiler::schema` holds a flattened, version-profiled variant of this schema
//! (`get_security_context_for`), not this file's three-level lookup with its SVG fallback, so the file is ported
//! here as is.

use std::sync::LazyLock;

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

// dom_security_schema.ts:18 SecurityContext
//
// The fork's AST already has Angular's `SecurityContext` (with one extra variant, `UrlOrResourceUrl`, for the
// `[URL, RESOURCE_URL]` pair); this file uses it so that the type checker's values compare with the AST's.
pub use oxc_angular_compiler::ast::r3::SecurityContext;

// dom_security_schema.ts:47 SecuritySchema
/// A security schema mapping property or attribute names to namespaces,
/// tag names, and finally their corresponding `SecurityContext`.
///
/// - The first level keys are lowercase property or attribute names (e.g., `'href'`, `'srcdoc'`).
/// - The second level keys are lowercase namespaces (e.g., `''` for HTML/default, `'svg'`, `'math'`).
/// - The third level keys are lowercase tag names (e.g., `'a'`, `'iframe'`, or `'*'` for all elements)
///   mapping to the appropriate `SecurityContext` value.
pub type SecuritySchema = IndexMap<
    String, // Property/Attribute Name
    IndexMap<
        String, // Namespace
        IndexMap<
            String, // Tag Name
            SecurityContext,
            FxBuildHasher,
        >,
        FxBuildHasher,
    >,
    FxBuildHasher,
>;

// dom_security_schema.ts:59 SVG_NAMESPACE
pub const SVG_NAMESPACE: &str = "svg";
// dom_security_schema.ts:60 MATH_ML_NAMESPACE
pub const MATH_ML_NAMESPACE: &str = "math";
// dom_security_schema.ts:61 NO_NAMESPACE
const NO_NAMESPACE: &str = "";
// dom_security_schema.ts:62 MATCH_ALL_ELEMENTS
const MATCH_ALL_ELEMENTS: &str = "*";

// dom_security_schema.ts:58 _SECURITY_SCHEMA
static SECURITY_SCHEMA_: LazyLock<SecuritySchema> = LazyLock::new(|| {
    let mut schema = SecuritySchema::default();

    // Case is insignificant below, all element and attribute names are lower-cased for lookup.

    register_context(
        &mut schema,
        SecurityContext::Html,
        /* Namespace */ None,
        &[("iframe", &["srcdoc"]), ("*", &["innerHTML", "outerHTML"])],
    );
    register_context(
        &mut schema,
        SecurityContext::Style,
        /* Namespace */ None,
        &[("*", &["style"])],
    );
    // NB: no SCRIPT contexts here, they are never allowed due to the parser stripping them.
    register_context(
        &mut schema,
        SecurityContext::Url,
        /* Namespace */ None,
        &[
            ("*", &["formAction"]),
            ("area", &["href"]),
            ("a", &["href", "xlink:href"]),
            ("form", &["action"]),
        ],
    );

    register_context(
        &mut schema,
        SecurityContext::Url,
        Some(MATH_ML_NAMESPACE),
        &[
            // MathML namespace
            // https://crsrc.org/c/third_party/blink/renderer/core/sanitizer/sanitizer.cc;l=753-768;drc=b3eb16372dcd3317d65e9e0265015e322494edcd;bpv=1;bpt=1
            ("*", &["href", "xlink:href"]),
        ],
    );

    register_context(
        &mut schema,
        SecurityContext::ResourceUrl,
        /* Namespace */ None,
        &[
            ("base", &["href"]),
            ("embed", &["src"]),
            ("frame", &["src"]),
            ("iframe", &["src"]),
            ("link", &["href"]),
            ("object", &["codebase", "data"]),
        ],
    );

    register_context(
        &mut schema,
        SecurityContext::Url,
        Some(SVG_NAMESPACE),
        &[("a", &["href", "xlink:href"])],
    );

    // Keep this in sync with SECURITY_SENSITIVE_ELEMENTS in packages/core/src/sanitization/sanitization.ts
    // The `unknown` elements refer to cases when we need to validate the input/binding in a directive (host bindings)
    // and the directive can be applied to multiple different elements (with different tag names). In this case we generate
    // a special instruction that an attribute might potentially be security-sensitive and defer the actual security check
    // to runtime, when we apply that directive to a concrete elements, thus we can check the combination of tag+attribute
    // against the set that requires sanitization.
    // These are unsafe as `attributeName` can be `href` or `xlink:href`
    // See: http://b/463880509#comment7
    register_context(
        &mut schema,
        SecurityContext::AttributeNoBinding,
        Some(SVG_NAMESPACE),
        &[
            ("animate", &["attributeName", "values", "to", "from"]),
            ("set", &["to", "attributeName"]),
            ("animateMotion", &["attributeName"]),
            ("animateTransform", &["attributeName"]),
        ],
    );

    register_context(
        &mut schema,
        SecurityContext::AttributeNoBinding,
        /* Namespace */ None,
        &[
            (
                "unknown",
                &[
                    "attributeName",
                    "values",
                    "to",
                    "from",
                    "sandbox",
                    "allow",
                    "allowFullscreen",
                    "referrerPolicy",
                    "csp",
                    "fetchPriority",
                    "credentialless",
                ],
            ),
            (
                "iframe",
                &[
                    "sandbox",
                    "allow",
                    "allowFullscreen",
                    "referrerPolicy",
                    "csp",
                    "fetchPriority",
                    "credentialless",
                ],
            ),
        ],
    );

    schema
});

// dom_security_schema.ts:69 SECURITY_SCHEMA
/// @remarks Keep is a copy of DOM Security Schema.
/// @see [SECURITY_SCHEMA](../../../compiler/src/schema/dom_security_schema.ts)
pub fn security_schema() -> &'static SecuritySchema {
    &SECURITY_SCHEMA_
}

// dom_security_schema.ts:157 registerContext
fn register_context(
    schema: &mut SecuritySchema,
    ctx: SecurityContext,
    namespace: Option<&str>,
    specs: &[(&str, &[&str])],
) {
    let ns_key = namespace.unwrap_or(NO_NAMESPACE);
    for (element, attribute_names) in specs {
        let tag_name = element.to_lowercase();

        for attr in *attribute_names {
            let attr_lower = attr.to_lowercase();
            let attr_schema = schema.entry(attr_lower).or_default();
            let ns_schema = attr_schema.entry(ns_key.to_string()).or_default();
            ns_schema.insert(tag_name.clone(), ctx);
        }
    }
}

// dom_security_schema.ts:181 checkSecurityContext
/// Checks the SecurityContext for a given tag and property.
///
/// @param tagName The tag name (e.g. 'div', or 'a')
/// @param propName The property or attribute name
/// @param namespace The namespace of the element, if any (e.g. 'svg' or 'math')
pub fn check_security_context(
    tag_name: &str,
    prop_name: &str,
    namespace: Option<&str>,
) -> SecurityContext {
    let security_schema = security_schema();
    let Some(attr_schema) = security_schema.get(&prop_name.to_lowercase()) else {
        return SecurityContext::None;
    };

    let tag_lower = tag_name.to_lowercase();
    let mut context: Option<SecurityContext> = None;

    if let Some(namespace) = namespace.filter(|ns| !ns.is_empty())
        && let Some(ns_schema) = attr_schema.get(namespace)
    {
        context = ns_schema.get(&tag_lower).or_else(|| ns_schema.get(MATCH_ALL_ELEMENTS)).copied();
    }

    if context.is_none()
        && let Some(default_schema) = attr_schema.get(NO_NAMESPACE)
    {
        context = default_schema
            .get(&tag_lower)
            .or_else(|| default_schema.get(MATCH_ALL_ELEMENTS))
            .copied();
    }

    // An SVG animation element declared outside of an `<svg>` has no explicit namespace at compile
    // time, but can still animate once it is projected into an SVG subtree.
    if context.is_none()
        && namespace.is_none_or(|ns| ns.is_empty() || ns == NO_NAMESPACE)
        && let Some(svg_schema) = attr_schema.get(SVG_NAMESPACE)
    {
        context = svg_schema.get(&tag_lower).copied();
    }

    context.unwrap_or(SecurityContext::None)
}
