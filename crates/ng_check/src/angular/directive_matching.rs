//! `packages/compiler/src/directive_matching.ts`

use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::rc::Rc;

use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

// directive_matching.ts:9 _SELECTOR_REGEXP
//
// The TypeScript tokenizes selectors with this global regular expression:
//
// ```text
// (\:not\()|                                       1: ":not("
// (([\.\#]?)[-\w]+)|                               2: "tag"; 3: "."/"#";
// (?:\[([-.\w*\\$]+)(?:=(["']?)([^\]"']*)\5)?\])|  4: attribute; 5: attribute_string; 6: attribute_value
// (\))|                                            7: ")"
// (\s*,\s*)                                        8: ","
// ```
//
// The `regex` crate has no backreferences (`\5`), so `selector_regexp_exec` below is that expression written out by
// hand with JavaScript's semantics: `exec` from `lastIndex` finds the leftmost position where an alternative matches,
// trying the alternatives in order; `\w` is ASCII `[A-Za-z0-9_]` and `\s` is JavaScript's whitespace. Every
// quantifier in it is greedy and none can give back characters usefully (the classes that follow each one are
// disjoint from it), so the greedy match is the match the backtracking engine finds.

// directive_matching.ts:25 SelectorRegexp
/// These offsets should match the match-groups in `_SELECTOR_REGEXP` offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectorRegexp {
    /// The whole match
    All = 0,
    Not = 1,
    Tag = 2,
    Prefix = 3,
    Attribute = 4,
    AttributeString = 5,
    AttributeValue = 6,
    NotEnd = 7,
    Separator = 8,
}

/// One `exec` result of `_SELECTOR_REGEXP`: the groups (`None` for a group that did not participate) and the new
/// `lastIndex`.
struct SelectorRegexpMatch<'s> {
    groups: [Option<&'s str>; 9],
    last_index: usize,
}

impl<'s> SelectorRegexpMatch<'s> {
    fn get(&self, group: SelectorRegexp) -> Option<&'s str> {
        self.groups[group as usize]
    }
}

/// `_SELECTOR_REGEXP.exec(selector)` with `lastIndex` = `last_index` (a byte offset on a char boundary).
fn selector_regexp_exec(selector: &str, last_index: usize) -> Option<SelectorRegexpMatch<'_>> {
    let mut position = last_index;
    while position < selector.len() {
        if let Some(found) = selector_regexp_match_at(selector, position) {
            return Some(found);
        }
        position += selector[position..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// `\w` in a JavaScript regular expression without the `u` flag.
fn is_regexp_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `\s` in a JavaScript regular expression.
fn is_regexp_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The end of the longest run of chars satisfying `pred` from `start`.
fn scan_while(s: &str, start: usize, pred: impl Fn(char) -> bool) -> usize {
    s[start..].char_indices().find(|&(_, c)| !pred(c)).map_or(s.len(), |(i, _)| start + i)
}

/// The alternatives of `_SELECTOR_REGEXP`, in order, anchored at `p`.
fn selector_regexp_match_at(s: &str, p: usize) -> Option<SelectorRegexpMatch<'_>> {
    let bytes = s.as_bytes();
    let mut groups: [Option<&str>; 9] = [None; 9];

    // 1: `(\:not\()`
    if s[p..].starts_with(":not(") {
        let end = p + ":not(".len();
        groups[SelectorRegexp::All as usize] = Some(&s[p..end]);
        groups[SelectorRegexp::Not as usize] = Some(&s[p..end]);
        return Some(SelectorRegexpMatch { groups, last_index: end });
    }

    // 2, 3: `(([\.\#]?)[-\w]+)`
    let prefix_end = if matches!(bytes[p], b'.' | b'#') { p + 1 } else { p };
    let tag_end = scan_while(s, prefix_end, |c| c == '-' || is_regexp_word(c));
    if tag_end > prefix_end {
        groups[SelectorRegexp::All as usize] = Some(&s[p..tag_end]);
        groups[SelectorRegexp::Tag as usize] = Some(&s[p..tag_end]);
        groups[SelectorRegexp::Prefix as usize] = Some(&s[p..prefix_end]);
        return Some(SelectorRegexpMatch { groups, last_index: tag_end });
    }
    // (With the prefix given back, `[-\w]+` would have to start at `.` or `#`, which it cannot.)

    // 4, 5, 6: `(?:\[([-.\w*\\$]+)(?:=(["']?)([^\]"']*)\5)?\])`
    if bytes[p] == b'[' {
        let name_start = p + 1;
        let name_end = scan_while(s, name_start, |c| {
            matches!(c, '-' | '.' | '*' | '\\' | '$') || is_regexp_word(c)
        });
        if name_end > name_start {
            let is_value_char = |c: char| !matches!(c, ']' | '"' | '\'');
            let mut attr: Option<(usize, &str, &str)> = None;
            if bytes.get(name_end) == Some(&b'=') {
                let quote_start = name_end + 1;
                // `["']?` takes the quote when there is one...
                if matches!(bytes.get(quote_start), Some(b'"' | b'\'')) {
                    let value_start = quote_start + 1;
                    let value_end = scan_while(s, value_start, is_value_char);
                    if bytes.get(value_end) == Some(&bytes[quote_start])
                        && bytes.get(value_end + 1) == Some(&b']')
                    {
                        attr = Some((
                            value_end + 2,
                            &s[quote_start..value_start],
                            &s[value_start..value_end],
                        ));
                    }
                }
                // ...and otherwise matches nothing (`\5` is then empty).
                if attr.is_none() {
                    let value_end = scan_while(s, quote_start, is_value_char);
                    if bytes.get(value_end) == Some(&b']') {
                        attr = Some((value_end + 1, "", &s[quote_start..value_end]));
                    }
                }
            }
            if let Some((end, quote, value)) = attr {
                groups[SelectorRegexp::All as usize] = Some(&s[p..end]);
                groups[SelectorRegexp::Attribute as usize] = Some(&s[name_start..name_end]);
                groups[SelectorRegexp::AttributeString as usize] = Some(quote);
                groups[SelectorRegexp::AttributeValue as usize] = Some(value);
                return Some(SelectorRegexpMatch { groups, last_index: end });
            }
            // The optional `=value` group did not match: `\]` right after the name.
            if bytes.get(name_end) == Some(&b']') {
                groups[SelectorRegexp::All as usize] = Some(&s[p..=name_end]);
                groups[SelectorRegexp::Attribute as usize] = Some(&s[name_start..name_end]);
                return Some(SelectorRegexpMatch { groups, last_index: name_end + 1 });
            }
        }
    }

    // 7: `(\))`
    if bytes[p] == b')' {
        groups[SelectorRegexp::All as usize] = Some(&s[p..=p]);
        groups[SelectorRegexp::NotEnd as usize] = Some(&s[p..=p]);
        return Some(SelectorRegexpMatch { groups, last_index: p + 1 });
    }

    // 8: `(\s*,\s*)`
    let comma = scan_while(s, p, is_regexp_whitespace);
    if bytes.get(comma) == Some(&b',') {
        let end = scan_while(s, comma + 1, is_regexp_whitespace);
        groups[SelectorRegexp::All as usize] = Some(&s[p..end]);
        groups[SelectorRegexp::Separator as usize] = Some(&s[p..end]);
        return Some(SelectorRegexpMatch { groups, last_index: end });
    }

    None
}

// directive_matching.ts:41 CssSelector
/// A css selector contains an element name,
/// css classes and attribute/value pairs with the purpose
/// of selecting subsets out of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssSelector {
    pub element: Option<String>,
    pub class_names: Vec<String>,
    /// The selectors are encoded in pairs where:
    /// - even locations are attribute names
    /// - odd locations are attribute values.
    ///
    /// Example:
    /// Selector: `[key1=value1][key2]` would parse to:
    /// ```text
    /// ['key1', 'value1', 'key2', '']
    /// ```
    pub attrs: Vec<String>,
    pub not_selectors: Vec<CssSelector>,
}

impl CssSelector {
    pub fn new() -> Self {
        Self::default()
    }

    // directive_matching.ts:58 parse
    /// # Panics
    ///
    /// As the TypeScript throws: on a nested `:not`, a selector list inside `:not`, or an unescaped `$` in an
    /// attribute name.
    pub fn parse(selector: &str) -> Vec<CssSelector> {
        Self::try_parse(selector).unwrap_or_else(|message| panic!("{message}"))
    }

    /// `parse`, with what the TypeScript throws as an error (for its callers that catch it: the release build
    /// aborts on a panic)
    pub fn try_parse(selector: &str) -> Result<Vec<CssSelector>, String> {
        let mut results: Vec<CssSelector> = Vec::new();
        let add_result = |res: &mut Vec<CssSelector>, mut css_sel: CssSelector| {
            if !css_sel.not_selectors.is_empty()
                && css_sel.element.as_deref().is_none_or(str::is_empty)
                && css_sel.class_names.is_empty()
                && css_sel.attrs.is_empty()
            {
                css_sel.element = Some("*".to_string());
            }
            res.push(css_sel);
        };
        let mut css_selector = CssSelector::new();
        // `current` is `cssSelector` or the last of its `notSelectors`.
        let mut current_is_not = false;
        let mut in_not = false;
        let mut last_index = 0;
        while let Some(match_) = selector_regexp_exec(selector, last_index) {
            last_index = match_.last_index;
            if match_.get(SelectorRegexp::Not).is_some_and(|s| !s.is_empty()) {
                if in_not {
                    return Err("Nesting :not in a selector is not allowed".to_string());
                }
                in_not = true;
                css_selector.not_selectors.push(CssSelector::new());
                current_is_not = true;
            }
            let current = if current_is_not {
                css_selector.not_selectors.last_mut().unwrap()
            } else {
                &mut css_selector
            };
            if let Some(tag) = match_.get(SelectorRegexp::Tag).filter(|s| !s.is_empty()) {
                let prefix = match_.get(SelectorRegexp::Prefix);
                if prefix == Some("#") {
                    // #hash
                    current.add_attribute("id", &tag[1..]);
                } else if prefix == Some(".") {
                    // Class
                    current.add_class_name(&tag[1..]);
                } else {
                    // Element
                    current.set_element(Some(tag));
                }
            }
            let attribute = match_.get(SelectorRegexp::Attribute);

            if let Some(attribute) = attribute.filter(|s| !s.is_empty()) {
                let name = current.try_unescape_attribute(attribute)?;
                current
                    .add_attribute(&name, match_.get(SelectorRegexp::AttributeValue).unwrap_or(""));
            }
            if match_.get(SelectorRegexp::NotEnd).is_some_and(|s| !s.is_empty()) {
                in_not = false;
                current_is_not = false;
            }
            if match_.get(SelectorRegexp::Separator).is_some_and(|s| !s.is_empty()) {
                if in_not {
                    return Err("Multiple selectors in :not are not supported".to_string());
                }
                add_result(&mut results, std::mem::take(&mut css_selector));
                current_is_not = false;
            }
        }
        add_result(&mut results, css_selector);
        Ok(results)
    }

    // directive_matching.ts:133 unescapeAttribute
    /// Unescape `\$` sequences from the CSS attribute selector.
    ///
    /// This is needed because `$` can have a special meaning in CSS selectors,
    /// but we might want to match an attribute that contains `$`.
    /// [MDN web link for more
    /// info](https://developer.mozilla.org/en-US/docs/Web/CSS/Attribute_selectors).
    /// @param attr the attribute to unescape.
    /// @returns the unescaped string.
    ///
    /// # Panics
    ///
    /// On an unescaped `$`, as the TypeScript throws.
    pub fn unescape_attribute(&self, attr: &str) -> String {
        self.try_unescape_attribute(attr).unwrap_or_else(|message| panic!("{message}"))
    }

    /// `unescapeAttribute`, with what the TypeScript throws as an error
    pub fn try_unescape_attribute(&self, attr: &str) -> Result<String, String> {
        let mut result = String::new();
        let mut escaping = false;
        for char in attr.chars() {
            if char == '\\' {
                escaping = true;
                continue;
            }
            if char == '$' && !escaping {
                return Err(format!(
                    "Error in attribute selector \"{attr}\". Unescaped \"$\" is not supported. Please escape with \"\\$\"."
                ));
            }
            escaping = false;
            result.push(char);
        }
        Ok(result)
    }

    // directive_matching.ts:164 escapeAttribute
    /// Escape `$` sequences from the CSS attribute selector.
    ///
    /// This is needed because `$` can have a special meaning in CSS selectors,
    /// with this method we are escaping `$` with `\$'.
    /// [MDN web link for more
    /// info](https://developer.mozilla.org/en-US/docs/Web/CSS/Attribute_selectors).
    /// @param attr the attribute to escape.
    /// @returns the escaped string.
    pub fn escape_attribute(&self, attr: &str) -> String {
        attr.replace('\\', "\\\\").replace('$', "\\$")
    }

    // directive_matching.ts:168 isElementSelector
    pub fn is_element_selector(&self) -> bool {
        self.has_element_selector()
            && self.class_names.is_empty()
            && self.attrs.is_empty()
            && self.not_selectors.is_empty()
    }

    // directive_matching.ts:177 hasElementSelector
    pub fn has_element_selector(&self) -> bool {
        self.element.as_deref().is_some_and(|e| !e.is_empty())
    }

    // directive_matching.ts:181 setElement
    pub fn set_element(&mut self, element: Option<&str>) {
        self.element = element.map(str::to_string);
    }

    // directive_matching.ts:185 getAttrs
    pub fn get_attrs(&self) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        if !self.class_names.is_empty() {
            result.push("class".to_string());
            result.push(self.class_names.join(" "));
        }
        result.extend(self.attrs.iter().cloned());
        result
    }

    // directive_matching.ts:193 addAttribute
    pub fn add_attribute(&mut self, name: &str, value: &str) {
        self.attrs.push(name.to_string());
        self.attrs.push(value.to_lowercase());
    }

    // directive_matching.ts:197 addClassName
    pub fn add_class_name(&mut self, name: &str) {
        self.class_names.push(name.to_lowercase());
    }
}

// directive_matching.ts:201 toString
impl fmt::Display for CssSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = self.element.clone().unwrap_or_default();
        for klass in &self.class_names {
            let _ = write!(res, ".{klass}");
        }
        for pair in self.attrs.chunks(2) {
            let name = self.escape_attribute(&pair[0]);
            let value = pair.get(1).map_or("", String::as_str);
            let value = if value.is_empty() { String::new() } else { format!("={value}") };
            let _ = write!(res, "[{name}{value}]");
        }
        for not_selector in &self.not_selectors {
            let _ = write!(res, ":not({not_selector})");
        }
        f.write_str(&res)
    }
}

/// `matchedCallback` of `SelectorMatcher.match`.
pub type MatchedCallback<'c, T> = Option<&'c mut dyn FnMut(&CssSelector, &T)>;

type SelectorContextMap<T> = IndexMap<String, Vec<Rc<SelectorContext<T>>>, FxBuildHasher>;
type SelectorMatcherMap<T> = IndexMap<String, SelectorMatcher<T>, FxBuildHasher>;

// directive_matching.ts:222 SelectorMatcher
/// Reads a list of CssSelectors and allows to calculate which ones
/// are contained in a given CssSelector.
#[derive(Debug)]
pub struct SelectorMatcher<T> {
    element_map: SelectorContextMap<T>,
    element_partial_map: SelectorMatcherMap<T>,
    class_map: SelectorContextMap<T>,
    class_partial_map: SelectorMatcherMap<T>,
    attr_value_map: IndexMap<String, SelectorContextMap<T>, FxBuildHasher>,
    attr_value_partial_map: IndexMap<String, SelectorMatcherMap<T>, FxBuildHasher>,
    list_contexts: Vec<Rc<SelectorListContext>>,
}

impl<T> Default for SelectorMatcher<T> {
    fn default() -> Self {
        Self {
            element_map: IndexMap::default(),
            element_partial_map: IndexMap::default(),
            class_map: IndexMap::default(),
            class_partial_map: IndexMap::default(),
            attr_value_map: IndexMap::default(),
            attr_value_partial_map: IndexMap::default(),
            list_contexts: Vec::new(),
        }
    }
}

impl SelectorMatcher<()> {
    // directive_matching.ts:223 createNotMatcher
    pub fn create_not_matcher(not_selectors: &[CssSelector]) -> SelectorMatcher<()> {
        let mut not_matcher = SelectorMatcher::<()>::new();
        not_matcher.add_selectables(not_selectors, ());
        not_matcher
    }
}

impl<T> SelectorMatcher<T> {
    pub fn new() -> Self {
        Self::default()
    }

    // directive_matching.ts:237 addSelectables
    pub fn add_selectables(&mut self, css_selectors: &[CssSelector], callback_ctxt: T)
    where
        T: Clone,
    {
        let mut list_context: Option<Rc<SelectorListContext>> = None;
        if css_selectors.len() > 1 {
            let context = Rc::new(SelectorListContext::new(css_selectors.to_vec()));
            self.list_contexts.push(Rc::clone(&context));
            list_context = Some(context);
        }
        for css_selector in css_selectors {
            self.add_selectable(css_selector, callback_ctxt.clone(), list_context.as_ref());
        }
    }

    // directive_matching.ts:253 _addSelectable
    /// Add an object that can be found later on by calling `match`.
    /// @param cssSelector A css selector
    /// @param callbackCtxt An opaque object that will be given to the callback of the `match` function
    fn add_selectable(
        &mut self,
        css_selector: &CssSelector,
        callback_ctxt: T,
        list_context: Option<&Rc<SelectorListContext>>,
    ) {
        let mut matcher: &mut SelectorMatcher<T> = self;
        let element = css_selector.element.as_deref();
        let class_names = &css_selector.class_names;
        let attrs = &css_selector.attrs;
        let selectable = Rc::new(SelectorContext::new(
            css_selector.clone(),
            callback_ctxt,
            list_context.map(Rc::clone),
        ));

        if let Some(element) = element.filter(|e| !e.is_empty()) {
            let is_terminal = attrs.is_empty() && class_names.is_empty();
            if is_terminal {
                Self::add_terminal(&mut matcher.element_map, element, Rc::clone(&selectable));
            } else {
                matcher = Self::add_partial(&mut matcher.element_partial_map, element);
            }
        }

        for (i, class_name) in class_names.iter().enumerate() {
            let is_terminal = attrs.is_empty() && i == class_names.len() - 1;
            if is_terminal {
                Self::add_terminal(&mut matcher.class_map, class_name, Rc::clone(&selectable));
            } else {
                matcher = Self::add_partial(&mut matcher.class_partial_map, class_name);
            }
        }

        let mut i = 0;
        while i < attrs.len() {
            let is_terminal = i == attrs.len() - 2;
            let name = &attrs[i];
            let value = &attrs[i + 1];
            if is_terminal {
                let terminal_map = &mut matcher.attr_value_map;
                let terminal_values_map = terminal_map.entry(name.clone()).or_default();
                Self::add_terminal(terminal_values_map, value, Rc::clone(&selectable));
            } else {
                let partial_map = &mut matcher.attr_value_partial_map;
                let partial_values_map = partial_map.entry(name.clone()).or_default();
                matcher = Self::add_partial(partial_values_map, value);
            }
            i += 2;
        }
    }

    // directive_matching.ts:311 _addTerminal
    fn add_terminal(
        map: &mut SelectorContextMap<T>,
        name: &str,
        selectable: Rc<SelectorContext<T>>,
    ) {
        let terminal_list = map.entry(name.to_string()).or_default();
        terminal_list.push(selectable);
    }

    // directive_matching.ts:324 _addPartial
    fn add_partial<'m>(
        map: &'m mut SelectorMatcherMap<T>,
        name: &str,
    ) -> &'m mut SelectorMatcher<T> {
        map.entry(name.to_string()).or_default()
    }

    // directive_matching.ts:340 match
    /// Find the objects that have been added via `addSelectable`
    /// whose css selector is contained in the given css selector.
    /// @param cssSelector A css selector
    /// @param matchedCallback This callback will be called with the object handed into `addSelectable`
    /// @return boolean true if a match was found
    pub fn match_(
        &self,
        css_selector: &CssSelector,
        mut matched_callback: MatchedCallback<'_, T>,
    ) -> bool {
        self.match_with(css_selector, &mut matched_callback)
    }

    /// `match`, with the callback borrowed so that the recursion can pass it on.
    fn match_with(
        &self,
        css_selector: &CssSelector,
        matched_callback: &mut MatchedCallback<'_, T>,
    ) -> bool {
        let mut result = false;
        let element = css_selector.element.as_deref();
        let class_names = &css_selector.class_names;
        let attrs = &css_selector.attrs;

        for list_context in &self.list_contexts {
            list_context.already_matched.set(false);
        }

        result =
            self.match_terminal(Some(&self.element_map), element, css_selector, matched_callback)
                || result;
        result = self.match_partial(
            Some(&self.element_partial_map),
            element,
            css_selector,
            matched_callback,
        ) || result;

        for class_name in class_names {
            result = self.match_terminal(
                Some(&self.class_map),
                Some(class_name),
                css_selector,
                matched_callback,
            ) || result;
            result = self.match_partial(
                Some(&self.class_partial_map),
                Some(class_name),
                css_selector,
                matched_callback,
            ) || result;
        }

        let mut i = 0;
        while i < attrs.len() {
            let name = &attrs[i];
            let value = &attrs[i + 1];

            let terminal_values_map = self.attr_value_map.get(name);
            if !value.is_empty() {
                result = self.match_terminal(
                    terminal_values_map,
                    Some(""),
                    css_selector,
                    matched_callback,
                ) || result;
            }
            result = self.match_terminal(
                terminal_values_map,
                Some(value),
                css_selector,
                matched_callback,
            ) || result;

            let partial_values_map = self.attr_value_partial_map.get(name);
            if !value.is_empty() {
                result = self.match_partial(
                    partial_values_map,
                    Some(""),
                    css_selector,
                    matched_callback,
                ) || result;
            }
            result =
                self.match_partial(partial_values_map, Some(value), css_selector, matched_callback)
                    || result;
            i += 2;
        }
        result
    }

    // directive_matching.ts:393 _matchTerminal
    #[expect(clippy::unused_self, reason = "the TypeScript method")]
    fn match_terminal(
        &self,
        map: Option<&SelectorContextMap<T>>,
        name: Option<&str>,
        css_selector: &CssSelector,
        matched_callback: &mut MatchedCallback<'_, T>,
    ) -> bool {
        let (Some(map), Some(name)) = (map, name) else {
            return false;
        };

        let selectables = map.get(name).map_or(&[][..], Vec::as_slice);
        let star_selectables = map.get("*").map_or(&[][..], Vec::as_slice);
        if selectables.is_empty() && star_selectables.is_empty() {
            return false;
        }
        let mut result = false;
        for selectable in selectables.iter().chain(star_selectables) {
            result = selectable.finalize(css_selector, matched_callback) || result;
        }
        result
    }

    // directive_matching.ts:421 _matchPartial
    #[expect(clippy::unused_self, reason = "the TypeScript method")]
    fn match_partial(
        &self,
        map: Option<&SelectorMatcherMap<T>>,
        name: Option<&str>,
        css_selector: &CssSelector,
        matched_callback: &mut MatchedCallback<'_, T>,
    ) -> bool {
        let (Some(map), Some(name)) = (map, name) else {
            return false;
        };

        let Some(nested_selector) = map.get(name) else {
            return false;
        };
        // TODO(perf): get rid of recursion and measure again
        // TODO(perf): don't pass the whole selector into the recursion,
        // but only the not processed parts
        nested_selector.match_with(css_selector, matched_callback)
    }
}

// directive_matching.ts:442 SelectorListContext
#[derive(Debug)]
pub struct SelectorListContext {
    pub already_matched: Cell<bool>,
    pub selectors: Vec<CssSelector>,
}

impl SelectorListContext {
    pub fn new(selectors: Vec<CssSelector>) -> Self {
        Self { already_matched: Cell::new(false), selectors }
    }
}

// directive_matching.ts:449 SelectorContext
/// Store context to pass back selector and context when a selector is matched
#[derive(Debug)]
pub struct SelectorContext<T> {
    pub not_selectors: Vec<CssSelector>,
    pub selector: CssSelector,
    pub cb_context: T,
    pub list_context: Option<Rc<SelectorListContext>>,
}

impl<T> SelectorContext<T> {
    // directive_matching.ts:452 constructor
    pub fn new(
        selector: CssSelector,
        cb_context: T,
        list_context: Option<Rc<SelectorListContext>>,
    ) -> Self {
        let not_selectors = selector.not_selectors.clone();
        Self { not_selectors, selector, cb_context, list_context }
    }

    // directive_matching.ts:460 finalize
    pub fn finalize(
        &self,
        css_selector: &CssSelector,
        callback: &mut MatchedCallback<'_, T>,
    ) -> bool {
        let mut result = true;
        let not_already_matched =
            self.list_context.as_ref().is_none_or(|c| !c.already_matched.get());
        if !self.not_selectors.is_empty() && not_already_matched {
            let not_matcher = SelectorMatcher::create_not_matcher(&self.not_selectors);
            result = !not_matcher.match_(css_selector, None);
        }
        if result
            && self.list_context.as_ref().is_none_or(|c| !c.already_matched.get())
            && let Some(callback) = callback.as_mut()
        {
            if let Some(list_context) = &self.list_context {
                list_context.already_matched.set(true);
            }
            callback(&self.selector, &self.cb_context);
        }
        result
    }
}

// directive_matching.ts:476 SelectorlessMatcher
#[derive(Debug)]
pub struct SelectorlessMatcher<T> {
    registry: IndexMap<String, Vec<T>, FxBuildHasher>,
}

impl<T: Clone> SelectorlessMatcher<T> {
    pub fn new(registry: IndexMap<String, Vec<T>, FxBuildHasher>) -> Self {
        Self { registry }
    }

    // directive_matching.ts:479 match
    pub fn match_(&self, name: &str) -> Vec<T> {
        self.registry.get(name).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    //! `packages/compiler/test/selector/selector_spec.ts`

    use super::*;

    /// `matched` of the spec: the selectors and contexts the collector saw, in order.
    #[derive(Debug, PartialEq)]
    enum Item {
        Sel(CssSelector),
        Ctx(i32),
    }

    fn pairs(items: &[(&CssSelector, i32)]) -> Vec<Item> {
        items.iter().flat_map(|(s, c)| [Item::Sel((*s).clone()), Item::Ctx(*c)]).collect()
    }

    fn run(
        matcher: &SelectorMatcher<i32>,
        selector: &CssSelector,
        matched: &mut Vec<Item>,
    ) -> bool {
        let mut collector = |selector: &CssSelector, context: &i32| {
            matched.push(Item::Sel(selector.clone()));
            matched.push(Item::Ctx(*context));
        };
        matcher.match_(selector, Some(&mut collector))
    }

    #[derive(Default)]
    struct SelectorFor<'a> {
        tag: &'a str,
        attrs: &'a [(&'a str, &'a str)],
        classes: &'a str,
    }

    fn get_selector_for(spec: &SelectorFor<'_>) -> CssSelector {
        let mut selector = CssSelector::new();
        selector.set_element(Some(spec.tag));

        for (name, value) in spec.attrs {
            selector.add_attribute(name, value);
        }

        // `classes.trim().split(/\s+/g)`: an empty string gives `['']`.
        let trimmed = spec.classes.trim();
        let class_names: Vec<&str> =
            if trimmed.is_empty() { vec![""] } else { trimmed.split_whitespace().collect() };
        for c_name in class_names {
            selector.add_class_name(c_name);
        }

        selector
    }

    fn tag(tag: &str) -> CssSelector {
        get_selector_for(&SelectorFor { tag, ..Default::default() })
    }

    fn classes(classes: &str) -> CssSelector {
        get_selector_for(&SelectorFor { classes, ..Default::default() })
    }

    fn attrs(attrs: &[(&str, &str)]) -> CssSelector {
        get_selector_for(&SelectorFor { attrs, ..Default::default() })
    }

    fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
        let err = std::panic::catch_unwind(f).expect_err("expected a panic");
        err.downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap()
    }

    #[test]
    fn should_select_by_element_name_case_sensitive() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("someTag");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &tag("SOMEOTHERTAG"), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &tag("SOMETAG"), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &tag("someTag"), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_select_by_class_name_case_insensitive() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse(".someClass");
        matcher.add_selectables(&s1, 1);
        let s2 = CssSelector::parse(".someClass.class2");
        matcher.add_selectables(&s2, 2);

        assert!(!run(&matcher, &classes("SOMEOTHERCLASS"), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &classes("SOMECLASS"), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));

        matched.clear();
        assert!(run(&matcher, &classes("someClass class2"), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));
    }

    #[test]
    fn should_not_throw_for_class_name_constructor() {
        let matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        assert!(!run(&matcher, &classes("constructor"), &mut matched));
        assert_eq!(matched, vec![]);
    }

    #[test]
    fn should_select_by_attr_name_case_sensitive_independent_of_the_value() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[someAttr]");
        matcher.add_selectables(&s1, 1);
        let s2 = CssSelector::parse("[someAttr][someAttr2]");
        matcher.add_selectables(&s2, 2);

        assert!(!run(&matcher, &attrs(&[("SOMEOTHERATTR", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &attrs(&[("SOMEATTR", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &attrs(&[("SOMEATTR", "someValue")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &attrs(&[("someAttr", ""), ("someAttr2", "")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));

        matched.clear();
        assert!(run(
            &matcher,
            &attrs(&[("someAttr", "someValue"), ("someAttr2", "")]),
            &mut matched
        ));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));

        matched.clear();
        assert!(run(
            &matcher,
            &attrs(&[("someAttr2", ""), ("someAttr", "someValue")]),
            &mut matched
        ));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));

        matched.clear();
        assert!(run(
            &matcher,
            &attrs(&[("someAttr2", "someValue"), ("someAttr", "")]),
            &mut matched
        ));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));
    }

    #[test]
    fn should_support_dot_in_attribute_names() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[foo.bar]");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &attrs(&[("barfoo", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        matched.clear();
        assert!(run(&matcher, &attrs(&[("foo.bar", "")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_support_dollar_in_attribute_names() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[someAttr\\$]");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &attrs(&[("someAttr", "")]), &mut matched));
        assert_eq!(matched, vec![]);
        matched.clear();

        assert!(run(&matcher, &attrs(&[("someAttr$", "")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
        matched.clear();

        let s1 = CssSelector::parse("[some\\$attr]");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &attrs(&[("someattr", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &attrs(&[("some$attr", "")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
        matched.clear();

        let s1 = CssSelector::parse("[\\$someAttr]");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &attrs(&[("someAttr", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &attrs(&[("$someAttr", "")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
        matched.clear();

        let s1 = CssSelector::parse("[some-\\$Attr]");
        matcher.add_selectables(&s1, 1);
        let s2 = CssSelector::parse("[some-\\$Attr][some-\\$-attr]");
        matcher.add_selectables(&s2, 2);

        assert!(!run(&matcher, &attrs(&[("some\\$Attr", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(
            &matcher,
            &attrs(&[("some-$-attr", "someValue"), ("some-$Attr", "")]),
            &mut matched
        ));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));
        matched.clear();

        assert!(!run(&matcher, &attrs(&[("someattr$", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &attrs(&[("some-simple-attr", "")]), &mut matched));
        assert_eq!(matched, vec![]);
        matched.clear();
    }

    #[test]
    fn should_select_by_attr_name_only_once_if_the_value_is_from_the_dom() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[some-decor]");
        matcher.add_selectables(&s1, 1);

        let mut element_selector = CssSelector::new();
        // `el('<div attr></div>').getAttribute('attr')`
        let empty = "";
        element_selector.add_attribute("some-decor", empty);
        run(&matcher, &element_selector, &mut matched);
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_select_by_attr_name_case_sensitive_and_value_case_insensitive() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[someAttr=someValue]");
        matcher.add_selectables(&s1, 1);

        assert!(!run(&matcher, &attrs(&[("SOMEATTR", "SOMEOTHERATTR")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &attrs(&[("SOMEATTR", "SOMEVALUE")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(&matcher, &attrs(&[("someAttr", "SOMEVALUE")]), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_select_by_element_name_class_name_and_attribute_name_with_value() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("someTag.someClass[someAttr=someValue]");
        matcher.add_selectables(&s1, 1);

        let spec = |tag, classes, attrs| get_selector_for(&SelectorFor { tag, attrs, classes });

        assert!(!run(
            &matcher,
            &spec("someOtherTag", "someOtherClass", &[("someOtherAttr", "")]),
            &mut matched
        ));
        assert_eq!(matched, vec![]);

        assert!(!run(
            &matcher,
            &spec("someTag", "someOtherClass", &[("someOtherAttr", "")]),
            &mut matched
        ));
        assert_eq!(matched, vec![]);

        assert!(!run(
            &matcher,
            &spec("someTag", "someClass", &[("someOtherAttr", "")]),
            &mut matched
        ));
        assert_eq!(matched, vec![]);

        assert!(!run(&matcher, &spec("someTag", "someClass", &[("someAttr", "")]), &mut matched));
        assert_eq!(matched, vec![]);

        assert!(run(
            &matcher,
            &spec("someTag", "someClass", &[("someAttr", "someValue")]),
            &mut matched
        ));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_select_by_many_attributes_and_independent_of_the_value() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("input[type=text][control]");
        matcher.add_selectables(&s1, 1);

        let mut css_selector = CssSelector::new();
        css_selector.set_element(Some("input"));
        css_selector.add_attribute("type", "text");
        css_selector.add_attribute("control", "one");

        assert!(run(&matcher, &css_selector, &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_select_independent_of_the_order_in_the_css_selector() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("[someAttr].someClass");
        matcher.add_selectables(&s1, 1);
        let s2 = CssSelector::parse(".someClass[someAttr]");
        matcher.add_selectables(&s2, 2);
        let s3 = CssSelector::parse(".class1.class2");
        matcher.add_selectables(&s3, 3);
        let s4 = CssSelector::parse(".class2.class1");
        matcher.add_selectables(&s4, 4);

        assert!(run(&matcher, &CssSelector::parse("[someAttr].someClass")[0], &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));

        matched.clear();
        assert!(run(&matcher, &CssSelector::parse(".someClass[someAttr]")[0], &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2)]));

        matched.clear();
        assert!(run(&matcher, &CssSelector::parse(".class1.class2")[0], &mut matched));
        assert_eq!(matched, pairs(&[(&s3[0], 3), (&s4[0], 4)]));

        matched.clear();
        assert!(run(&matcher, &CssSelector::parse(".class2.class1")[0], &mut matched));
        assert_eq!(matched, pairs(&[(&s4[0], 4), (&s3[0], 3)]));
    }

    #[test]
    fn should_not_select_with_a_matching_not_selector() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        matcher.add_selectables(&CssSelector::parse("p:not(.someClass)"), 1);
        matcher.add_selectables(&CssSelector::parse("p:not([someAttr])"), 2);
        matcher.add_selectables(&CssSelector::parse(":not(.someClass)"), 3);
        matcher.add_selectables(&CssSelector::parse(":not(p)"), 4);
        matcher.add_selectables(&CssSelector::parse(":not(p[someAttr])"), 5);

        let selector = get_selector_for(&SelectorFor {
            tag: "p",
            classes: "someClass",
            attrs: &[("someAttr", "")],
        });
        assert!(!run(&matcher, &selector, &mut matched));
        assert_eq!(matched, vec![]);
    }

    #[test]
    fn should_select_with_a_non_matching_not_selector() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("p:not(.someClass)");
        matcher.add_selectables(&s1, 1);
        let s2 = CssSelector::parse("p:not(.someOtherClass[someAttr])");
        matcher.add_selectables(&s2, 2);
        let s3 = CssSelector::parse(":not(.someClass)");
        matcher.add_selectables(&s3, 3);
        let s4 = CssSelector::parse(":not(.someOtherClass[someAttr])");
        matcher.add_selectables(&s4, 4);

        let selector = get_selector_for(&SelectorFor {
            tag: "p",
            attrs: &[("someOtherAttr", "")],
            classes: "someOtherClass",
        });
        assert!(run(&matcher, &selector, &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1), (&s2[0], 2), (&s3[0], 3), (&s4[0], 4)]));
    }

    #[test]
    fn should_match_star_with_not_selector() {
        let mut matcher = SelectorMatcher::new();
        matcher.add_selectables(&CssSelector::parse(":not([a])"), 1);
        assert!(matcher.match_(&tag("div"), Some(&mut |_: &CssSelector, _: &i32| {})));
    }

    #[test]
    fn should_match_with_multiple_not_selectors() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("div:not([a]):not([b])");
        matcher.add_selectables(&s1, 1);
        let spec = |attrs| get_selector_for(&SelectorFor { tag: "div", attrs, classes: "" });
        assert!(!run(&matcher, &spec(&[("a", "")]), &mut matched));
        assert!(!run(&matcher, &spec(&[("b", "")]), &mut matched));
        assert!(run(&matcher, &spec(&[("c", "")]), &mut matched));
    }

    #[test]
    fn should_select_with_one_match_in_a_list() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("input[type=text], textbox");
        matcher.add_selectables(&s1, 1);

        assert!(run(&matcher, &tag("textbox"), &mut matched));
        assert_eq!(matched, pairs(&[(&s1[1], 1)]));

        matched.clear();
        let selector = get_selector_for(&SelectorFor {
            tag: "input",
            attrs: &[("type", "text")],
            classes: "",
        });
        assert!(run(&matcher, &selector, &mut matched));
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    #[test]
    fn should_not_select_twice_with_two_matches_in_a_list() {
        let mut matcher = SelectorMatcher::new();
        let mut matched = Vec::new();
        let s1 = CssSelector::parse("input, .someClass");
        matcher.add_selectables(&s1, 1);

        let selector =
            get_selector_for(&SelectorFor { tag: "input", classes: "someclass", attrs: &[] });
        assert!(run(&matcher, &selector, &mut matched));
        assert_eq!(matched.len(), 2);
        assert_eq!(matched, pairs(&[(&s1[0], 1)]));
    }

    // describe('CssSelector.parse')

    #[test]
    fn should_detect_element_names() {
        let css_selector = &CssSelector::parse("sometag")[0];
        assert_eq!(css_selector.element.as_deref(), Some("sometag"));
        assert_eq!(css_selector.to_string(), "sometag");
    }

    #[test]
    fn should_detect_attr_names_with_escaped_dollar() {
        let css_selector = &CssSelector::parse("[attrname\\$]")[0];
        assert_eq!(css_selector.attrs, ["attrname$", ""]);
        assert_eq!(css_selector.to_string(), "[attrname\\$]");

        let css_selector = &CssSelector::parse("[\\$attrname]")[0];
        assert_eq!(css_selector.attrs, ["$attrname", ""]);
        assert_eq!(css_selector.to_string(), "[\\$attrname]");

        let css_selector = &CssSelector::parse("[foo\\$bar]")[0];
        assert_eq!(css_selector.attrs, ["foo$bar", ""]);
        assert_eq!(css_selector.to_string(), "[foo\\$bar]");
    }

    #[test]
    fn should_error_on_attr_names_with_unescaped_dollar() {
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("[attrname$]");
            }),
            "Error in attribute selector \"attrname$\". Unescaped \"$\" is not supported. Please escape with \"\\$\"."
        );
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("[$attrname]");
            }),
            "Error in attribute selector \"$attrname\". Unescaped \"$\" is not supported. Please escape with \"\\$\"."
        );
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("[foo$bar]");
            }),
            "Error in attribute selector \"foo$bar\". Unescaped \"$\" is not supported. Please escape with \"\\$\"."
        );
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("[foo\\$bar$]");
            }),
            "Error in attribute selector \"foo\\$bar$\". Unescaped \"$\" is not supported. Please escape with \"\\$\"."
        );
    }

    #[test]
    fn should_detect_class_names() {
        let css_selector = &CssSelector::parse(".someClass")[0];
        assert_eq!(css_selector.class_names, ["someclass"]);

        assert_eq!(css_selector.to_string(), ".someclass");
    }

    #[test]
    fn should_detect_attr_names() {
        let css_selector = &CssSelector::parse("[attrname]")[0];
        assert_eq!(css_selector.attrs, ["attrname", ""]);

        assert_eq!(css_selector.to_string(), "[attrname]");
    }

    #[test]
    fn should_detect_attr_values() {
        let css_selector = &CssSelector::parse("[attrname=attrvalue]")[0];
        assert_eq!(css_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(css_selector.to_string(), "[attrname=attrvalue]");
    }

    #[test]
    fn should_detect_attr_values_with_double_quotes() {
        let css_selector = &CssSelector::parse("[attrname=\"attrvalue\"]")[0];
        assert_eq!(css_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(css_selector.to_string(), "[attrname=attrvalue]");
    }

    #[test]
    fn should_detect_hash_some_value_syntax_and_treat_as_attribute() {
        let css_selector = &CssSelector::parse("#some-value")[0];
        assert_eq!(css_selector.attrs, ["id", "some-value"]);
        assert_eq!(css_selector.to_string(), "[id=some-value]");
    }

    #[test]
    fn should_detect_attr_values_with_single_quotes() {
        let css_selector = &CssSelector::parse("[attrname='attrvalue']")[0];
        assert_eq!(css_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(css_selector.to_string(), "[attrname=attrvalue]");
    }

    #[test]
    fn should_detect_multiple_parts() {
        let css_selector = &CssSelector::parse("sometag[attrname=attrvalue].someclass")[0];
        assert_eq!(css_selector.element.as_deref(), Some("sometag"));
        assert_eq!(css_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(css_selector.class_names, ["someclass"]);

        assert_eq!(css_selector.to_string(), "sometag.someclass[attrname=attrvalue]");
    }

    #[test]
    fn should_detect_multiple_attributes() {
        let css_selector = &CssSelector::parse("input[type=text][control]")[0];
        assert_eq!(css_selector.element.as_deref(), Some("input"));
        assert_eq!(css_selector.attrs, ["type", "text", "control", ""]);

        assert_eq!(css_selector.to_string(), "input[type=text][control]");
    }

    #[test]
    fn should_detect_not() {
        let css_selector = &CssSelector::parse("sometag:not([attrname=attrvalue].someclass)")[0];
        assert_eq!(css_selector.element.as_deref(), Some("sometag"));
        assert_eq!(css_selector.attrs.len(), 0);
        assert_eq!(css_selector.class_names.len(), 0);

        let not_selector = &css_selector.not_selectors[0];
        assert_eq!(not_selector.element, None);
        assert_eq!(not_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(not_selector.class_names, ["someclass"]);

        assert_eq!(css_selector.to_string(), "sometag:not(.someclass[attrname=attrvalue])");
    }

    #[test]
    fn should_detect_not_without_truthy() {
        let css_selector = &CssSelector::parse(":not([attrname=attrvalue].someclass)")[0];
        assert_eq!(css_selector.element.as_deref(), Some("*"));

        let not_selector = &css_selector.not_selectors[0];
        assert_eq!(not_selector.attrs, ["attrname", "attrvalue"]);
        assert_eq!(not_selector.class_names, ["someclass"]);

        assert_eq!(css_selector.to_string(), "*:not(.someclass[attrname=attrvalue])");
    }

    #[test]
    fn should_throw_when_nested_not() {
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("sometag:not(:not([attrname=attrvalue].someclass))");
            }),
            "Nesting :not in a selector is not allowed"
        );
    }

    #[test]
    fn should_throw_when_multiple_selectors_in_not() {
        assert_eq!(
            panic_message(|| {
                CssSelector::parse("sometag:not(a,b)");
            }),
            "Multiple selectors in :not are not supported"
        );
    }

    #[test]
    fn should_detect_lists_of_selectors() {
        let css_selectors = CssSelector::parse(".someclass,[attrname=attrvalue], sometag");
        assert_eq!(css_selectors.len(), 3);

        assert_eq!(css_selectors[0].class_names, ["someclass"]);
        assert_eq!(css_selectors[1].attrs, ["attrname", "attrvalue"]);
        assert_eq!(css_selectors[2].element.as_deref(), Some("sometag"));
    }

    #[test]
    fn should_detect_lists_of_selectors_with_not() {
        let css_selectors =
            CssSelector::parse("input[type=text], :not(textarea), textbox:not(.special)");
        assert_eq!(css_selectors.len(), 3);

        assert_eq!(css_selectors[0].element.as_deref(), Some("input"));
        assert_eq!(css_selectors[0].attrs, ["type", "text"]);

        assert_eq!(css_selectors[1].element.as_deref(), Some("*"));
        assert_eq!(css_selectors[1].not_selectors[0].element.as_deref(), Some("textarea"));

        assert_eq!(css_selectors[2].element.as_deref(), Some("textbox"));
        assert_eq!(css_selectors[2].not_selectors[0].class_names, ["special"]);
    }

    // Not in the spec: `SelectorlessMatcher`, and the regular expression's edge cases.

    #[test]
    fn selectorless_matcher_returns_the_registered_values() {
        let mut registry: IndexMap<String, Vec<i32>, FxBuildHasher> = IndexMap::default();
        registry.insert("Foo".to_string(), vec![1, 2]);
        let matcher = SelectorlessMatcher::new(registry);
        assert_eq!(matcher.match_("Foo"), vec![1, 2]);
        assert_eq!(matcher.match_("Bar"), Vec::<i32>::new());
    }

    #[test]
    fn selector_regexp_edge_cases() {
        // A quote that is not closed by the same quote falls back to the unquoted form, which then fails, so the
        // `[` is skipped and the rest is tokenized as tags.
        let css_selector = &CssSelector::parse("[a=\"b']")[0];
        assert_eq!(css_selector.attrs, Vec::<String>::new());
        assert_eq!(css_selector.element.as_deref(), Some("b"));
        // Whitespace without a comma is skipped; a lone `.` is skipped.
        let css_selectors = CssSelector::parse("div . span");
        assert_eq!(css_selectors.len(), 1);
        assert_eq!(css_selectors[0].element.as_deref(), Some("span"));
        // `[a=]` has an empty value.
        assert_eq!(CssSelector::parse("[a=]")[0].attrs, ["a", ""]);
        assert_eq!(CssSelector::parse("[a='']")[0].attrs, ["a", ""]);
    }
}
