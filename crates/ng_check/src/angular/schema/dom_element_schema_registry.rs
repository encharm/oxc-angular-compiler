//! `packages/compiler/src/schema/dom_element_schema_registry.ts`

use std::sync::{Arc, LazyLock, OnceLock};

use indexmap::{IndexMap, IndexSet};
use rustc_hash::FxBuildHasher;

use crate::angular::core::{CUSTOM_ELEMENTS_SCHEMA, NO_ERRORS_SCHEMA, SchemaMetadata};
use crate::angular::schema::dom_security_schema::{
    MATH_ML_NAMESPACE, SVG_NAMESPACE, SecurityContext, check_security_context,
};
use crate::angular::schema::element_schema_registry::{
    ElementSchemaRegistry, NormalizedStyleValue, StyleValue, ValidationResult,
};

// dom_element_schema_registry.ts:16 BOOLEAN
const BOOLEAN: &str = "boolean";
// dom_element_schema_registry.ts:17 NUMBER
const NUMBER: &str = "number";
// dom_element_schema_registry.ts:18 STRING
const STRING: &str = "string";
// dom_element_schema_registry.ts:19 OBJECT
const OBJECT: &str = "object";

// dom_element_schema_registry.ts:21 normalizeTagName
fn normalize_tag_name(tag_name: &str) -> String {
    let tag_name_lower = tag_name.to_lowercase();
    let (ns, name) = split_ns_name(&tag_name_lower, false);

    // (`SVG_NAMESPACE`/`MATH_ML_NAMESPACE` are imported from `template/pipeline/src/namespaces.ts` in the
    // TypeScript; `dom_security_schema.ts` exports the same constants.)
    if ns == Some(SVG_NAMESPACE) || ns == Some(MATH_ML_NAMESPACE) {
        format!(":{}:{name}", ns.unwrap_or_default())
    } else {
        name.to_string()
    }
}

// ml_parser/tags.ts:27 splitNsName
//
// Not in this file's TypeScript: `ml_parser/tags.ts` is not ported (the fork's `split_ns_name` has no `fatal`
// parameter), so the three helpers this file imports from it are ported here.
fn split_ns_name(element_name: &str, fatal: bool) -> (Option<&str>, &str) {
    if !element_name.starts_with(':') {
        return (None, element_name);
    }

    let Some(colon_index) = element_name[1..].find(':').map(|i| i + 1) else {
        assert!(!fatal, "Unsupported format \"{element_name}\" expecting \":namespace:name\"");
        return (None, element_name);
    };

    (Some(&element_name[1..colon_index]), &element_name[colon_index + 1..])
}

// ml_parser/tags.ts:46 isNgContainer
/// `<ng-container>` tags work the same regardless the namespace
fn is_ng_container(tag_name: &str) -> bool {
    split_ns_name(tag_name, true).1 == "ng-container"
}

// ml_parser/tags.ts:51 isNgContent
/// `<ng-content>` tags work the same regardless the namespace
fn is_ng_content(tag_name: &str) -> bool {
    split_ns_name(tag_name, true).1 == "ng-content"
}

// util.ts:13 dashCaseToCamelCase
//
// Not in this file's TypeScript (imported from `util.ts`): `input.replace(/-+([a-z0-9])/g, (...m) =>
// m[1].toUpperCase())`.
fn dash_case_to_camel_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' {
            let mut j = i;
            while j < chars.len() && chars[j] == '-' {
                j += 1;
            }
            if j < chars.len() && (chars[j].is_ascii_lowercase() || chars[j].is_ascii_digit()) {
                result.push(chars[j].to_ascii_uppercase());
                i = j + 1;
                continue;
            }
            // No match at this `-`: the regular expression moves on one character.
            result.push('-');
            i += 1;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

// dom_element_schema_registry.ts:84 SCHEMA
/// This array represents the DOM schema. It encodes inheritance, properties, and events.
///
/// ## Overview
///
/// Each line represents one kind of element. The `element_inheritance` and properties are joined
/// using `element_inheritance|properties` syntax.
///
/// ## Element Inheritance
///
/// The `element_inheritance` can be further subdivided as `element1,element2,...^parentElement`.
/// Here the individual elements are separated by `,` (commas). Every element in the list
/// has identical properties.
///
/// An `element` may inherit additional properties from `parentElement` If no `^parentElement` is
/// specified then `""` (blank) element is assumed.
///
/// NOTE: The blank element inherits from root `[Element]` element, the super element of all
/// elements.
///
/// NOTE an element prefix such as `:svg:` has no special meaning to the schema.
///
/// ## Properties
///
/// Each element has a set of properties separated by `,` (commas). Each property can be prefixed
/// by a special character designating its type:
///
/// - (no prefix): property is a string.
/// - `*`: property represents an event.
/// - `!`: property is a boolean.
/// - `#`: property is a number.
/// - `%`: property is an object.
///
/// ## Query
///
/// The class creates an internal squas representation which allows to easily answer the query of
/// if a given property exist on a given element.
///
/// NOTE: We don't yet support querying for types or events.
/// NOTE: This schema is auto extracted from `schema_extractor.ts` located in the test folder,
///       see `dom_element_schema_registry_spec.ts`
//
// =================================================================================================
// =================================================================================================
// =========== S T O P   -  S T O P   -  S T O P   -  S T O P   -  S T O P   -  S T O P  ===========
// =================================================================================================
// =================================================================================================
//
//                       DO NOT EDIT THIS DOM SCHEMA WITHOUT A SECURITY REVIEW!
//
// Newly added properties must be security reviewed and assigned an appropriate SecurityContext in
// dom_security_schema.ts. Reach out to mprobst & rjamet for details.
//
// =================================================================================================
pub const SCHEMA: &[&str] = &[
    concat!(
        "[Element]|textContent,%ariaActiveDescendantElement,%ariaAtomic,%ariaAutoComplete,%ariaBusy,%ariaChecked,%ariaColCount,%ariaColIndex,%ariaColIndexText,%ariaColSpan,%ariaControlsElements,%ariaCurrent,%ariaDescribedByElements,%ariaDescription,%ariaDetailsElements,%ariaDisabled,%ariaErrorMessageElements,%ariaExpanded,%ariaFlowToElements,%ariaHasPopup,%ariaHidden,%ariaInvalid,%ariaKeyShortcuts,%ariaLabel,%ariaLabelledByElements,%ariaLevel,%ariaLive,%ariaModal,%ariaMultiLine,%ariaMultiSelectable,%ariaOrientation,%ariaOwnsElements,%ariaPlaceholder,%ariaPosInSet,%ariaPressed,%ariaReadOnly,%ariaRelevant,%ariaRequired,%ariaRoleDescription,%ariaRowCount,%ariaRowIndex,%ariaRowIndexText,%ariaRowSpan,%ariaSelected,%ariaSetSize,%ariaSort,%ariaValueMax,%ariaValueMin,%ariaValueNow,%ariaValueText,%classList,className,elementTiming,id,innerHTML,*beforecopy,*beforecut,*beforepaste,*fullscreenchange,*fullscreenerror,*search,*webkitfullscreenchange,*webkitfullscreenerror,outerHTML,%part,#scrollLeft,#scrollTop,slot",
        /* added manually to avoid breaking changes */
        ",*message,*mozfullscreenchange,*mozfullscreenerror,*mozpointerlockchange,*mozpointerlockerror,*webglcontextcreationerror,*webglcontextlost,*webglcontextrestored",
    ),
    "[HTMLElement]^[Element]|accessKey,autocapitalize,!autofocus,contentEditable,dir,!draggable,enterKeyHint,!hidden,!inert,innerText,inputMode,lang,nonce,*abort,*animationend,*animationiteration,*animationstart,*auxclick,*beforexrselect,*blur,*cancel,*canplay,*canplaythrough,*change,*click,*close,*contextmenu,*copy,*cuechange,*cut,*dblclick,*drag,*dragend,*dragenter,*dragleave,*dragover,*dragstart,*drop,*durationchange,*emptied,*ended,*error,*focus,*formdata,*gotpointercapture,*input,*invalid,*keydown,*keypress,*keyup,*load,*loadeddata,*loadedmetadata,*loadstart,*lostpointercapture,*mousedown,*mouseenter,*mouseleave,*mousemove,*mouseout,*mouseover,*mouseup,*mousewheel,*paste,*pause,*play,*playing,*pointercancel,*pointerdown,*pointerenter,*pointerleave,*pointermove,*pointerout,*pointerover,*pointerrawupdate,*pointerup,*progress,*ratechange,*reset,*resize,*scroll,*securitypolicyviolation,*seeked,*seeking,*select,*selectionchange,*selectstart,*slotchange,*stalled,*submit,*suspend,*timeupdate,*toggle,*transitioncancel,*transitionend,*transitionrun,*transitionstart,*volumechange,*waiting,*webkitanimationend,*webkitanimationiteration,*webkitanimationstart,*webkittransitionend,*wheel,outerText,popover,!spellcheck,%style,#tabIndex,title,!translate,virtualKeyboardPolicy",
    "abbr,address,article,aside,b,bdi,bdo,cite,content,code,dd,dfn,dt,em,figcaption,figure,footer,header,hgroup,i,kbd,main,mark,nav,noscript,rb,rp,rt,rtc,ruby,s,samp,search,section,small,strong,sub,sup,u,var,wbr^[HTMLElement]|accessKey,autocapitalize,!autofocus,contentEditable,dir,!draggable,enterKeyHint,!hidden,innerText,inputMode,lang,nonce,*abort,*animationend,*animationiteration,*animationstart,*auxclick,*beforexrselect,*blur,*cancel,*canplay,*canplaythrough,*change,*click,*close,*contextmenu,*copy,*cuechange,*cut,*dblclick,*drag,*dragend,*dragenter,*dragleave,*dragover,*dragstart,*drop,*durationchange,*emptied,*ended,*error,*focus,*formdata,*gotpointercapture,*input,*invalid,*keydown,*keypress,*keyup,*load,*loadeddata,*loadedmetadata,*loadstart,*lostpointercapture,*mousedown,*mouseenter,*mouseleave,*mousemove,*mouseout,*mouseover,*mouseup,*mousewheel,*paste,*pause,*play,*playing,*pointercancel,*pointerdown,*pointerenter,*pointerleave,*pointermove,*pointerout,*pointerover,*pointerrawupdate,*pointerup,*progress,*ratechange,*reset,*resize,*scroll,*securitypolicyviolation,*seeked,*seeking,*select,*selectionchange,*selectstart,*slotchange,*stalled,*submit,*suspend,*timeupdate,*toggle,*transitioncancel,*transitionend,*transitionrun,*transitionstart,*volumechange,*waiting,*webkitanimationend,*webkitanimationiteration,*webkitanimationstart,*webkittransitionend,*wheel,outerText,!spellcheck,%style,#tabIndex,title,!translate,virtualKeyboardPolicy",
    "media^[HTMLElement]|!autoplay,!controls,%controlsList,%crossOrigin,#currentTime,!defaultMuted,#defaultPlaybackRate,!disableRemotePlayback,!loop,!muted,*encrypted,*waitingforkey,#playbackRate,preload,!preservesPitch,src,%srcObject,#volume",
    ":svg:^[HTMLElement]|!autofocus,nonce,*abort,*animationend,*animationiteration,*animationstart,*auxclick,*beforexrselect,*blur,*cancel,*canplay,*canplaythrough,*change,*click,*close,*contextmenu,*copy,*cuechange,*cut,*dblclick,*drag,*dragend,*dragenter,*dragleave,*dragover,*dragstart,*drop,*durationchange,*emptied,*ended,*error,*focus,*formdata,*gotpointercapture,*input,*invalid,*keydown,*keypress,*keyup,*load,*loadeddata,*loadedmetadata,*loadstart,*lostpointercapture,*mousedown,*mouseenter,*mouseleave,*mousemove,*mouseout,*mouseover,*mouseup,*mousewheel,*paste,*pause,*play,*playing,*pointercancel,*pointerdown,*pointerenter,*pointerleave,*pointermove,*pointerout,*pointerover,*pointerrawupdate,*pointerup,*progress,*ratechange,*reset,*resize,*scroll,*securitypolicyviolation,*seeked,*seeking,*select,*selectionchange,*selectstart,*slotchange,*stalled,*submit,*suspend,*timeupdate,*toggle,*transitioncancel,*transitionend,*transitionrun,*transitionstart,*volumechange,*waiting,*webkitanimationend,*webkitanimationiteration,*webkitanimationstart,*webkittransitionend,*wheel,%style,#tabIndex",
    ":svg:graphics^:svg:|",
    ":svg:animation^:svg:|*begin,*end,*repeat",
    ":svg:geometry^:svg:|",
    ":svg:componentTransferFunction^:svg:|",
    ":svg:gradient^:svg:|",
    ":svg:textContent^:svg:graphics|",
    ":svg:textPositioning^:svg:textContent|",
    "a^[HTMLElement]|charset,coords,download,hash,host,hostname,href,hreflang,name,password,pathname,ping,port,protocol,referrerPolicy,rel,%relList,rev,search,shape,target,text,type,username",
    "area^[HTMLElement]|alt,coords,download,hash,host,hostname,href,!noHref,password,pathname,ping,port,protocol,referrerPolicy,rel,%relList,search,shape,target,username",
    "audio^media|",
    "br^[HTMLElement]|clear",
    "base^[HTMLElement]|href,target",
    "body^[HTMLElement]|aLink,background,bgColor,link,*afterprint,*beforeprint,*beforeunload,*blur,*error,*focus,*hashchange,*languagechange,*load,*message,*messageerror,*offline,*online,*pagehide,*pageshow,*popstate,*rejectionhandled,*resize,*scroll,*storage,*unhandledrejection,*unload,text,vLink",
    "button^[HTMLElement]|!disabled,formAction,formEnctype,formMethod,!formNoValidate,formTarget,name,popoverTargetAction,%popoverTargetElement,type,value",
    "canvas^[HTMLElement]|#height,#width",
    "content^[HTMLElement]|select",
    "dl^[HTMLElement]|!compact",
    "data^[HTMLElement]|value",
    "datalist^[HTMLElement]|",
    "details^[HTMLElement]|!open",
    "dialog^[HTMLElement]|!open,returnValue",
    "dir^[HTMLElement]|!compact",
    "div^[HTMLElement]|align",
    "embed^[HTMLElement]|align,height,name,src,type,width",
    "fieldset^[HTMLElement]|!disabled,name",
    "font^[HTMLElement]|color,face,size",
    "form^[HTMLElement]|acceptCharset,action,autocomplete,encoding,enctype,method,name,!noValidate,target",
    "frame^[HTMLElement]|frameBorder,longDesc,marginHeight,marginWidth,name,!noResize,scrolling,src",
    "frameset^[HTMLElement]|cols,*afterprint,*beforeprint,*beforeunload,*blur,*error,*focus,*hashchange,*languagechange,*load,*message,*messageerror,*offline,*online,*pagehide,*pageshow,*popstate,*rejectionhandled,*resize,*scroll,*storage,*unhandledrejection,*unload,rows",
    "geolocation^[HTMLElement]|accuracymode,!autolocate,*location,*promptaction,*promptdismiss,*validationstatuschange,!watch",
    "hr^[HTMLElement]|align,color,!noShade,size,width",
    "head^[HTMLElement]|",
    "h1,h2,h3,h4,h5,h6^[HTMLElement]|align",
    "html^[HTMLElement]|version",
    "iframe^[HTMLElement]|align,allow,!allowFullscreen,!allowPaymentRequest,csp,!credentialless,frameBorder,height,loading,longDesc,marginHeight,marginWidth,name,referrerPolicy,%sandbox,scrolling,src,srcdoc,width",
    "img^[HTMLElement]|align,alt,border,%crossOrigin,decoding,#height,#hspace,!isMap,loading,longDesc,lowsrc,name,referrerPolicy,sizes,src,srcset,useMap,#vspace,#width",
    "input^[HTMLElement]|accept,align,alt,autocomplete,!checked,!defaultChecked,defaultValue,dirName,!disabled,%files,formAction,formEnctype,formMethod,!formNoValidate,formTarget,#height,!incremental,!indeterminate,max,#maxLength,min,#minLength,!multiple,name,pattern,placeholder,popoverTargetAction,%popoverTargetElement,!readOnly,!required,selectionDirection,#selectionEnd,#selectionStart,#size,src,step,type,useMap,value,%valueAsDate,#valueAsNumber,#width",
    "li^[HTMLElement]|type,#value",
    "label^[HTMLElement]|htmlFor",
    "legend^[HTMLElement]|align",
    "link^[HTMLElement]|as,charset,%crossOrigin,!disabled,href,hreflang,imageSizes,imageSrcset,integrity,media,referrerPolicy,rel,%relList,rev,%sizes,target,type",
    "map^[HTMLElement]|name",
    "marquee^[HTMLElement]|behavior,bgColor,direction,height,#hspace,#loop,#scrollAmount,#scrollDelay,!trueSpeed,#vspace,width",
    "menu^[HTMLElement]|!compact",
    "meta^[HTMLElement]|content,httpEquiv,media,name,scheme",
    "meter^[HTMLElement]|#high,#low,#max,#min,#optimum,#value",
    "ins,del^[HTMLElement]|cite,dateTime",
    "ol^[HTMLElement]|!compact,!reversed,#start,type",
    "object^[HTMLElement]|align,archive,border,code,codeBase,codeType,data,!declare,height,#hspace,name,standby,type,useMap,#vspace,width",
    "optgroup^[HTMLElement]|!disabled,label",
    "option^[HTMLElement]|!defaultSelected,!disabled,label,!selected,text,value",
    "output^[HTMLElement]|defaultValue,%htmlFor,name,value",
    "p^[HTMLElement]|align",
    "param^[HTMLElement]|name,type,value,valueType",
    "picture^[HTMLElement]|",
    "pre^[HTMLElement]|#width",
    "progress^[HTMLElement]|#max,#value",
    "q,blockquote,cite^[HTMLElement]|",
    "script^[HTMLElement]|!async,charset,%crossOrigin,!defer,event,htmlFor,integrity,!noModule,%referrerPolicy,src,text,type",
    "select^[HTMLElement]|autocomplete,!disabled,#length,!multiple,name,!required,#selectedIndex,#size,value",
    "selectedcontent^[HTMLElement]|",
    "slot^[HTMLElement]|name",
    "source^[HTMLElement]|#height,media,sizes,src,srcset,type,#width",
    "span^[HTMLElement]|",
    "style^[HTMLElement]|!disabled,media,type",
    "search^[HTMLELement]|",
    "caption^[HTMLElement]|align",
    "th,td^[HTMLElement]|abbr,align,axis,bgColor,ch,chOff,#colSpan,headers,height,!noWrap,#rowSpan,scope,vAlign,width",
    "col,colgroup^[HTMLElement]|align,ch,chOff,#span,vAlign,width",
    "table^[HTMLElement]|align,bgColor,border,%caption,cellPadding,cellSpacing,frame,rules,summary,%tFoot,%tHead,width",
    "tr^[HTMLElement]|align,bgColor,ch,chOff,vAlign",
    "tfoot,thead,tbody^[HTMLElement]|align,ch,chOff,vAlign",
    "template^[HTMLElement]|",
    "textarea^[HTMLElement]|autocomplete,#cols,defaultValue,dirName,!disabled,#maxLength,#minLength,name,placeholder,!readOnly,!required,#rows,selectionDirection,#selectionEnd,#selectionStart,value,wrap",
    "time^[HTMLElement]|dateTime",
    "title^[HTMLElement]|text",
    "track^[HTMLElement]|!default,kind,label,src,srclang",
    "ul^[HTMLElement]|!compact,type",
    "unknown^[HTMLElement]|",
    "video^media|!disablePictureInPicture,#height,*enterpictureinpicture,*leavepictureinpicture,!playsInline,poster,#width",
    ":svg:a^:svg:graphics|",
    ":svg:animate^:svg:animation|",
    ":svg:animateMotion^:svg:animation|",
    ":svg:animateTransform^:svg:animation|",
    ":svg:circle^:svg:geometry|",
    ":svg:clipPath^:svg:graphics|",
    ":svg:defs^:svg:graphics|",
    ":svg:desc^:svg:|",
    ":svg:discard^:svg:|",
    ":svg:ellipse^:svg:geometry|",
    ":svg:feBlend^:svg:|",
    ":svg:feColorMatrix^:svg:|",
    ":svg:feComponentTransfer^:svg:|",
    ":svg:feComposite^:svg:|",
    ":svg:feConvolveMatrix^:svg:|",
    ":svg:feDiffuseLighting^:svg:|",
    ":svg:feDisplacementMap^:svg:|",
    ":svg:feDistantLight^:svg:|",
    ":svg:feDropShadow^:svg:|",
    ":svg:feFlood^:svg:|",
    ":svg:feFuncA^:svg:componentTransferFunction|",
    ":svg:feFuncB^:svg:componentTransferFunction|",
    ":svg:feFuncG^:svg:componentTransferFunction|",
    ":svg:feFuncR^:svg:componentTransferFunction|",
    ":svg:feGaussianBlur^:svg:|",
    ":svg:feImage^:svg:|",
    ":svg:feMerge^:svg:|",
    ":svg:feMergeNode^:svg:|",
    ":svg:feMorphology^:svg:|",
    ":svg:feOffset^:svg:|",
    ":svg:fePointLight^:svg:|",
    ":svg:feSpecularLighting^:svg:|",
    ":svg:feSpotLight^:svg:|",
    ":svg:feTile^:svg:|",
    ":svg:feTurbulence^:svg:|",
    ":svg:filter^:svg:|",
    ":svg:foreignObject^:svg:graphics|",
    ":svg:g^:svg:graphics|",
    ":svg:image^:svg:graphics|decoding",
    ":svg:line^:svg:geometry|",
    ":svg:linearGradient^:svg:gradient|",
    ":svg:mpath^:svg:|",
    ":svg:marker^:svg:|",
    ":svg:mask^:svg:|",
    ":svg:metadata^:svg:|",
    ":svg:path^:svg:geometry|",
    ":svg:pattern^:svg:|",
    ":svg:polygon^:svg:geometry|",
    ":svg:polyline^:svg:geometry|",
    ":svg:radialGradient^:svg:gradient|",
    ":svg:rect^:svg:geometry|",
    ":svg:svg^:svg:graphics|#currentScale,#zoomAndPan",
    ":svg:script^:svg:|type",
    ":svg:set^:svg:animation|",
    ":svg:stop^:svg:|",
    ":svg:style^:svg:|!disabled,media,title,type",
    ":svg:switch^:svg:graphics|",
    ":svg:symbol^:svg:|",
    ":svg:tspan^:svg:textPositioning|",
    ":svg:text^:svg:textPositioning|",
    ":svg:textPath^:svg:textContent|",
    ":svg:title^:svg:|",
    ":svg:use^:svg:graphics|",
    ":svg:view^:svg:|#zoomAndPan",
    "data^[HTMLElement]|value",
    "keygen^[HTMLElement]|!autofocus,challenge,!disabled,form,keytype,name",
    "menuitem^[HTMLElement]|type,label,icon,!disabled,!checked,radiogroup,!default",
    "summary^[HTMLElement]|",
    "time^[HTMLElement]|dateTime",
    ":svg:cursor^:svg:|",
    ":math:^[HTMLElement]|!autofocus,nonce,*abort,*animationend,*animationiteration,*animationstart,*auxclick,*beforeinput,*beforematch,*beforetoggle,*beforexrselect,*blur,*cancel,*canplay,*canplaythrough,*change,*click,*close,*contentvisibilityautostatechange,*contextlost,*contextmenu,*contextrestored,*copy,*cuechange,*cut,*dblclick,*drag,*dragend,*dragenter,*dragleave,*dragover,*dragstart,*drop,*durationchange,*emptied,*ended,*error,*focus,*formdata,*gotpointercapture,*input,*invalid,*keydown,*keypress,*keyup,*load,*loadeddata,*loadedmetadata,*loadstart,*lostpointercapture,*mousedown,*mouseenter,*mouseleave,*mousemove,*mouseout,*mouseover,*mouseup,*mousewheel,*paste,*pause,*play,*playing,*pointercancel,*pointerdown,*pointerenter,*pointerleave,*pointermove,*pointerout,*pointerover,*pointerrawupdate,*pointerup,*progress,*ratechange,*reset,*resize,*scroll,*scrollend,*securitypolicyviolation,*seeked,*seeking,*select,*selectionchange,*selectstart,*slotchange,*stalled,*submit,*suspend,*timeupdate,*toggle,*transitioncancel,*transitionend,*transitionrun,*transitionstart,*volumechange,*waiting,*webkitanimationend,*webkitanimationiteration,*webkitanimationstart,*webkittransitionend,*wheel,%style,#tabIndex",
    ":math:math^:math:|",
    ":math:maction^:math:|",
    ":math:menclose^:math:|",
    ":math:merror^:math:|",
    ":math:mfenced^:math:|",
    ":math:mfrac^:math:|",
    ":math:mi^:math:|",
    ":math:mmultiscripts^:math:|",
    ":math:mn^:math:|",
    ":math:mo^:math:|",
    ":math:mover^:math:|",
    ":math:mpadded^:math:|",
    ":math:mphantom^:math:|",
    ":math:mroot^:math:|",
    ":math:mrow^:math:|",
    ":math:ms^:math:|",
    ":math:mspace^:math:|",
    ":math:msqrt^:math:|",
    ":math:mstyle^:math:|",
    ":math:msub^:math:|",
    ":math:msubsup^:math:|",
    ":math:msup^:math:|",
    ":math:mtable^:math:|",
    ":math:mtd^:math:|",
    ":math:mtext^:math:|",
    ":math:mtr^:math:|",
    ":math:munder^:math:|",
    ":math:munderover^:math:|",
    ":math:semantics^:math:|",
];

// dom_element_schema_registry.ts:274 _ATTR_TO_PROP
pub static ATTR_TO_PROP: LazyLock<IndexMap<&'static str, &'static str, FxBuildHasher>> =
    LazyLock::new(|| {
        [
            ("class", "className"),
            ("for", "htmlFor"),
            ("formaction", "formAction"),
            ("innerHtml", "innerHTML"),
            ("readonly", "readOnly"),
            ("tabindex", "tabIndex"),
            // https://www.w3.org/TR/wai-aria-1.3/#accessibilityroleandproperties-correspondence
            ("aria-activedescendant", "ariaActiveDescendantElement"),
            ("aria-atomic", "ariaAtomic"),
            ("aria-autocomplete", "ariaAutoComplete"),
            ("aria-busy", "ariaBusy"),
            ("aria-checked", "ariaChecked"),
            ("aria-colcount", "ariaColCount"),
            ("aria-colindex", "ariaColIndex"),
            ("aria-colindextext", "ariaColIndexText"),
            ("aria-colspan", "ariaColSpan"),
            ("aria-controls", "ariaControlsElements"),
            ("aria-current", "ariaCurrent"),
            ("aria-describedby", "ariaDescribedByElements"),
            ("aria-description", "ariaDescription"),
            ("aria-details", "ariaDetailsElements"),
            ("aria-disabled", "ariaDisabled"),
            ("aria-errormessage", "ariaErrorMessageElements"),
            ("aria-expanded", "ariaExpanded"),
            ("aria-flowto", "ariaFlowToElements"),
            ("aria-haspopup", "ariaHasPopup"),
            ("aria-hidden", "ariaHidden"),
            ("aria-invalid", "ariaInvalid"),
            ("aria-keyshortcuts", "ariaKeyShortcuts"),
            ("aria-label", "ariaLabel"),
            ("aria-labelledby", "ariaLabelledByElements"),
            ("aria-level", "ariaLevel"),
            ("aria-live", "ariaLive"),
            ("aria-modal", "ariaModal"),
            ("aria-multiline", "ariaMultiLine"),
            ("aria-multiselectable", "ariaMultiSelectable"),
            ("aria-orientation", "ariaOrientation"),
            ("aria-owns", "ariaOwnsElements"),
            ("aria-placeholder", "ariaPlaceholder"),
            ("aria-posinset", "ariaPosInSet"),
            ("aria-pressed", "ariaPressed"),
            ("aria-readonly", "ariaReadOnly"),
            ("aria-required", "ariaRequired"),
            ("aria-roledescription", "ariaRoleDescription"),
            ("aria-rowcount", "ariaRowCount"),
            ("aria-rowindex", "ariaRowIndex"),
            ("aria-rowindextext", "ariaRowIndexText"),
            ("aria-rowspan", "ariaRowSpan"),
            ("aria-selected", "ariaSelected"),
            ("aria-setsize", "ariaSetSize"),
            ("aria-sort", "ariaSort"),
            ("aria-valuemax", "ariaValueMax"),
            ("aria-valuemin", "ariaValueMin"),
            ("aria-valuenow", "ariaValueNow"),
            ("aria-valuetext", "ariaValueText"),
        ]
        .into_iter()
        .collect()
    });

// dom_element_schema_registry.ts:336 _PROP_TO_ATTR
/// Invert `_ATTR_TO_PROP`.
///
/// As in the TypeScript, the reducer's `[propertyName, attributeName]` destructures `_ATTR_TO_PROP`'s
/// `[attribute, property]` entries, so this map has the same entries as `_ATTR_TO_PROP` (attribute to property).
static PROP_TO_ATTR: LazyLock<IndexMap<&'static str, &'static str, FxBuildHasher>> =
    LazyLock::new(|| {
        ATTR_TO_PROP.iter().fold(
            IndexMap::default(),
            |mut inverted, (&property_name, &attribute_name)| {
                inverted.insert(property_name, attribute_name);
                inverted
            },
        )
    });

type ElementProperties = IndexMap<String, &'static str, FxBuildHasher>;
type ElementEvents = IndexSet<String, FxBuildHasher>;

// dom_element_schema_registry.ts:344 DomElementSchemaRegistry
//
// The TypeScript shares one `Map`/`Set` between the tags of a schema line; here they share an `Arc`.
#[derive(Debug)]
pub struct DomElementSchemaRegistry {
    schema: IndexMap<String, Arc<ElementProperties>, FxBuildHasher>,
    /// We don't allow binding to events for security reasons. Allowing event bindings would almost
    /// certainly introduce bad XSS vulnerabilities. Instead, we store events in a separate schema.
    event_schema: IndexMap<String, Arc<ElementEvents>, FxBuildHasher>,
    /// Union of all events in `_eventSchema`, built lazily by `isKnownEventOfAnyElement`. Since
    /// events bubble, any known event can be observed on any element, regardless of its tag.
    all_known_events: OnceLock<ElementEvents>,
}

impl Default for DomElementSchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DomElementSchemaRegistry {
    // dom_element_schema_registry.ts:353 constructor
    pub fn new() -> Self {
        let mut schema: IndexMap<String, Arc<ElementProperties>, FxBuildHasher> =
            IndexMap::default();
        let mut event_schema: IndexMap<String, Arc<ElementEvents>, FxBuildHasher> =
            IndexMap::default();
        for encoded_type in SCHEMA {
            let mut type_ = ElementProperties::default();
            let mut events = ElementEvents::default();
            let mut parts = encoded_type.split('|');
            let str_type = parts.next().unwrap_or_default();
            let str_properties = parts.next().unwrap_or_default();
            let properties = str_properties.split(',');
            let mut type_parts = str_type.split('^');
            let type_names = type_parts.next().unwrap_or_default();
            let super_name = type_parts.next();
            // The TypeScript registers the (still empty) shared map and set under each tag first, then fills them.
            let tags: Vec<String> = type_names.split(',').map(str::to_lowercase).collect();
            for tag in &tags {
                schema.insert(tag.clone(), Arc::default());
                event_schema.insert(tag.clone(), Arc::default());
            }
            let super_name = super_name.filter(|s| !s.is_empty()).map(str::to_lowercase);
            let super_type = super_name.as_ref().and_then(|super_name| schema.get(super_name));
            if let Some(super_type) = super_type {
                for (prop, value) in super_type.iter() {
                    type_.insert(prop.clone(), value);
                }
                for super_event in event_schema[super_name.as_deref().unwrap_or_default()].iter() {
                    events.insert(super_event.clone());
                }
            }
            for property in properties {
                if !property.is_empty() {
                    match property.as_bytes()[0] {
                        b'*' => {
                            events.insert(property[1..].to_string());
                        }
                        b'!' => {
                            type_.insert(property[1..].to_string(), BOOLEAN);
                        }
                        b'#' => {
                            type_.insert(property[1..].to_string(), NUMBER);
                        }
                        b'%' => {
                            type_.insert(property[1..].to_string(), OBJECT);
                        }
                        _ => {
                            type_.insert(property.to_string(), STRING);
                        }
                    }
                }
            }
            let type_ = Arc::new(type_);
            let events = Arc::new(events);
            for tag in tags {
                schema.insert(tag.clone(), Arc::clone(&type_));
                event_schema.insert(tag, Arc::clone(&events));
            }
        }
        Self { schema, event_schema, all_known_events: OnceLock::new() }
    }
}

impl ElementSchemaRegistry for DomElementSchemaRegistry {
    // dom_element_schema_registry.ts:397 hasProperty
    fn has_property(
        &self,
        tag_name: &str,
        prop_name: &str,
        schema_metas: &[SchemaMetadata],
    ) -> bool {
        if schema_metas.iter().any(|schema| schema.name == NO_ERRORS_SCHEMA) {
            return true;
        }

        let normalized_tag = normalize_tag_name(tag_name);
        if normalized_tag.contains('-') {
            if is_ng_container(&normalized_tag) || is_ng_content(&normalized_tag) {
                return false;
            }

            if schema_metas.iter().any(|schema| schema.name == CUSTOM_ELEMENTS_SCHEMA) {
                // Can't tell now as we don't know which properties a custom element will get
                // once it is instantiated
                return true;
            }
        }

        let element_properties =
            self.schema.get(&normalized_tag).unwrap_or_else(|| &self.schema["unknown"]);
        element_properties.contains_key(prop_name)
    }

    // dom_element_schema_registry.ts:419 hasElement
    fn has_element(&self, tag_name: &str, schema_metas: &[SchemaMetadata]) -> bool {
        if schema_metas.iter().any(|schema| schema.name == NO_ERRORS_SCHEMA) {
            return true;
        }

        let normalized_tag = normalize_tag_name(tag_name);
        if normalized_tag.contains('-') {
            if is_ng_container(&normalized_tag) || is_ng_content(&normalized_tag) {
                return true;
            }

            if schema_metas.iter().any(|schema| schema.name == CUSTOM_ELEMENTS_SCHEMA) {
                // Allow any custom elements
                return true;
            }
        }

        self.schema.contains_key(&normalized_tag)
    }

    // dom_element_schema_registry.ts:449 securityContext
    /// securityContext returns the security context for the given property on the given DOM tag.
    ///
    /// Tag and property name are statically known and cannot change at runtime, i.e. it is not
    /// possible to bind a value into a changing attribute or tag name.
    ///
    /// The filtering is based on a list of allowed tags|attributes. All attributes in the schema
    /// above are assumed to have the 'NONE' security context, i.e. that they are safe inert
    /// string values. Only specific well known attack vectors are assigned their appropriate context.
    fn security_context(
        &self,
        tag_name: &str,
        prop_name: &str,
        is_attribute: bool,
    ) -> SecurityContext {
        let mut prop_name = prop_name.to_string();
        if is_attribute {
            // NB: For security purposes, use the mapped property name, not the attribute name.
            prop_name = self.get_mapped_prop_name(&prop_name);
        }

        let (ns, name) = split_ns_name(tag_name, false);
        check_security_context(name, &prop_name, ns)
    }

    // dom_element_schema_registry.ts:463 getMappedPropName
    fn get_mapped_prop_name(&self, prop_name: &str) -> String {
        ATTR_TO_PROP.get(prop_name).copied().unwrap_or(prop_name).to_string()
    }

    // dom_element_schema_registry.ts:467 getDefaultComponentElementName
    fn get_default_component_element_name(&self) -> String {
        "ng-component".to_string()
    }

    // dom_element_schema_registry.ts:471 validateProperty
    fn validate_property(&self, name: &str) -> ValidationResult {
        if name.to_lowercase().starts_with("on") {
            let msg = format!(
                "Binding to event property '{name}' is disallowed for security reasons, please use ({})=...\nIf '{name}' is a directive input, make sure the directive is imported by the current module.",
                js_slice_from(name, 2)
            );
            ValidationResult { error: true, msg: Some(msg) }
        } else {
            ValidationResult { error: false, msg: None }
        }
    }

    // dom_element_schema_registry.ts:484 validateAttribute
    fn validate_attribute(&self, name: &str) -> ValidationResult {
        if name.to_lowercase().starts_with("on") {
            let msg = format!(
                "Binding to event attribute '{name}' is disallowed for security reasons, please use ({})=...",
                js_slice_from(name, 2)
            );
            ValidationResult { error: true, msg: Some(msg) }
        } else {
            ValidationResult { error: false, msg: None }
        }
    }

    // dom_element_schema_registry.ts:495 allKnownElementNames
    fn all_known_element_names(&self) -> Vec<String> {
        self.schema.keys().cloned().collect()
    }

    // dom_element_schema_registry.ts:522 normalizeAnimationStyleProperty
    fn normalize_animation_style_property(&self, prop_name: &str) -> String {
        dash_case_to_camel_case(prop_name)
    }

    // dom_element_schema_registry.ts:526 normalizeAnimationStyleValue
    fn normalize_animation_style_value(
        &self,
        camel_case_prop: &str,
        user_provided_prop: &str,
        val: StyleValue<'_>,
    ) -> NormalizedStyleValue {
        let mut unit = "";
        let str_val = match val {
            StyleValue::String(s) => js_trim(s).to_string(),
            StyleValue::Number(n) => js_number_to_string(n),
        };
        let mut error_msg: Option<String> = None;

        let is_zero = match val {
            StyleValue::Number(n) => n == 0.0,
            StyleValue::String(s) => s == "0",
        };
        if is_pixel_dimension_style(camel_case_prop) && !is_zero {
            match val {
                StyleValue::Number(_) => {
                    unit = "px";
                }
                StyleValue::String(val) => {
                    // `val.match(/^[+-]?[\d\.]+([a-z]*)$/)`
                    let unsigned = val.strip_prefix(['+', '-']).unwrap_or(val);
                    let number_end = unsigned
                        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                        .unwrap_or(unsigned.len());
                    let suffix = &unsigned[number_end..];
                    let val_and_suffix_match =
                        number_end > 0 && suffix.bytes().all(|b| b.is_ascii_lowercase());
                    if val_and_suffix_match && suffix.is_empty() {
                        error_msg = Some(format!(
                            "Please provide a CSS unit value for {user_provided_prop}:{val}"
                        ));
                    }
                }
            }
        }
        NormalizedStyleValue { error: error_msg, value: str_val + unit }
    }
}

// The non-`override` methods of `DomElementSchemaRegistry` (in the TypeScript, between `allKnownElementNames` and
// `normalizeAnimationStyleProperty`).
impl DomElementSchemaRegistry {
    // dom_element_schema_registry.ts:499 allKnownAttributesOfElement
    pub fn all_known_attributes_of_element(&self, tag_name: &str) -> Vec<String> {
        let normalized_tag = normalize_tag_name(tag_name);
        let element_properties =
            self.schema.get(&normalized_tag).unwrap_or_else(|| &self.schema["unknown"]);
        // Convert properties to attributes.
        element_properties
            .keys()
            .map(|prop| {
                PROP_TO_ATTR
                    .get(prop.as_str())
                    .map_or_else(|| prop.clone(), |attr| (*attr).to_string())
            })
            .collect()
    }

    // dom_element_schema_registry.ts:506 allKnownEventsOfElement
    pub fn all_known_events_of_element(&self, tag_name: &str) -> Vec<String> {
        let normalized_tag = normalize_tag_name(tag_name);
        self.event_schema
            .get(&normalized_tag)
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }

    // dom_element_schema_registry.ts:511 isKnownEventOfAnyElement
    pub fn is_known_event_of_any_element(&self, event_name: &str) -> bool {
        // Note: `flat()` would not work here since it only flattens nested arrays,
        // while `_eventSchema` contains `Set`s.
        let all_known_events = self.all_known_events.get_or_init(|| {
            self.event_schema.values().flat_map(|events| events.iter().cloned()).collect()
        });
        // The schema stores event names in lowercase, but some events have camelCase names at
        // runtime (e.g. the vendor-prefixed `webkitAnimationEnd`), so compare ignoring case.
        all_known_events.contains(&event_name.to_lowercase())
    }
}

// dom_element_schema_registry.ts:549 _isPixelDimensionStyle
fn is_pixel_dimension_style(prop: &str) -> bool {
    matches!(
        prop,
        "width"
            | "height"
            | "minWidth"
            | "minHeight"
            | "maxWidth"
            | "maxHeight"
            | "left"
            | "top"
            | "bottom"
            | "right"
            | "fontSize"
            | "outlineWidth"
            | "outlineOffset"
            | "paddingTop"
            | "paddingLeft"
            | "paddingBottom"
            | "paddingRight"
            | "marginTop"
            | "marginLeft"
            | "marginBottom"
            | "marginRight"
            | "borderRadius"
            | "borderWidth"
            | "borderTopWidth"
            | "borderLeftWidth"
            | "borderRightWidth"
            | "borderBottomWidth"
            | "textIndent"
    )
}

/// `s.slice(start)` with JavaScript's UTF-16 indices.
fn js_slice_from(s: &str, start: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16_lossy(units.get(start..).unwrap_or_default())
}

/// `String.prototype.trim`: JavaScript's whitespace and line terminators.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n'
                | '\u{000B}'
                | '\u{000C}'
                | '\r'
                | ' '
                | '\u{00A0}'
                | '\u{1680}'
                | '\u{2000}'
                ..='\u{200A}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202F}'
                    | '\u{205F}'
                    | '\u{3000}'
                    | '\u{FEFF}'
        )
    })
}

/// `Number.prototype.toString()` (ECMA-262 `Number::toString` with radix 10).
fn js_number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if value < 0.0 {
        return format!("-{}", js_number_to_string(-value));
    }
    // Rust's `{:e}` prints the shortest digits that round-trip, as JavaScript does.
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    let n = exponent.parse::<i64>().unwrap_or(0) + 1;
    let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
    if k <= n && n <= 21 {
        format!("{digits}{}", zeros(n - k))
    } else if 0 < n && n <= 21 {
        let split = usize::try_from(n).unwrap_or(0);
        format!("{}.{}", &digits[..split], &digits[split..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", zeros(-n))
    } else {
        let e = n - 1;
        let sign = if e >= 0 { "+" } else { "-" };
        if k == 1 {
            format!("{digits}e{sign}{}", e.abs())
        } else {
            format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
        }
    }
}

#[cfg(test)]
mod tests {
    //! `packages/compiler/test/schema/dom_element_schema_registry_spec.ts`

    use super::*;

    fn schema(name: &str) -> SchemaMetadata {
        SchemaMetadata { name: name.to_string() }
    }

    #[test]
    fn should_detect_elements() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_element("div", &[]));
        assert!(registry.has_element("b", &[]));
        assert!(registry.has_element("ng-container", &[]));
        assert!(registry.has_element("ng-content", &[]));

        assert!(!registry.has_element("my-cmp", &[]));
        assert!(!registry.has_element("abc", &[]));
    }

    // https://github.com/angular/angular/issues/11219
    #[test]
    fn should_detect_elements_missing_from_chrome() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_element("data", &[]));
        assert!(registry.has_element("menuitem", &[]));
        assert!(registry.has_element("summary", &[]));
        assert!(registry.has_element("time", &[]));
    }

    #[test]
    fn should_detect_properties_on_regular_elements() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_property("div", "id", &[]));
        assert!(registry.has_property("div", "title", &[]));
        assert!(registry.has_property("div", "inert", &[]));
        assert!(registry.has_property("div", "popover", &[]));
        assert!(registry.has_property("button", "popoverTargetAction", &[]));
        assert!(registry.has_property("button", "popoverTargetElement", &[]));
        assert!(registry.has_property("input", "popoverTargetAction", &[]));
        assert!(registry.has_property("input", "popoverTargetElement", &[]));
        assert!(registry.has_property("h1", "align", &[]));
        assert!(registry.has_property("h2", "align", &[]));
        assert!(registry.has_property("h3", "align", &[]));
        assert!(registry.has_property("h4", "align", &[]));
        assert!(registry.has_property("h5", "align", &[]));
        assert!(registry.has_property("h6", "align", &[]));
        assert!(!registry.has_property("h7", "align", &[]));
        assert!(registry.has_property("textarea", "disabled", &[]));
        assert!(registry.has_property("input", "disabled", &[]));
        assert!(!registry.has_property("div", "unknown", &[]));
    }

    // https://github.com/angular/angular/issues/11219
    #[test]
    fn should_detect_properties_on_elements_missing_from_chrome() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_property("data", "value", &[]));

        assert!(registry.has_property("menuitem", "type", &[]));
        assert!(registry.has_property("menuitem", "default", &[]));

        assert!(registry.has_property("time", "dateTime", &[]));
    }

    #[test]
    fn should_detect_different_kinds_of_types() {
        let registry = DomElementSchemaRegistry::new();
        // inheritance: video => media => [HTMLElement] => [Element]
        assert!(registry.has_property("video", "className", &[])); // from [Element]
        assert!(registry.has_property("video", "id", &[])); // string
        assert!(registry.has_property("video", "scrollLeft", &[])); // number
        assert!(registry.has_property("video", "height", &[])); // number
        assert!(registry.has_property("video", "autoplay", &[])); // boolean
        assert!(registry.has_property("video", "classList", &[])); // object
        // from *; but events are not properties
        assert!(!registry.has_property("video", "click", &[]));
    }

    #[test]
    fn should_treat_custom_elements_as_an_unknown_element_by_default() {
        let registry = DomElementSchemaRegistry::new();
        assert!(!registry.has_property("custom-like", "unknown", &[]));
        assert!(registry.has_property("custom-like", "className", &[]));
        assert!(registry.has_property("custom-like", "style", &[]));
        assert!(registry.has_property("custom-like", "id", &[]));
    }

    #[test]
    fn should_return_true_for_custom_like_elements_if_the_custom_elements_schema_was_used() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_property("custom-like", "unknown", &[schema(CUSTOM_ELEMENTS_SCHEMA)]));

        assert!(registry.has_element("custom-like", &[schema(CUSTOM_ELEMENTS_SCHEMA)]));
    }

    #[test]
    fn should_return_true_for_all_elements_if_the_no_errors_schema_was_used() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_property("custom-like", "unknown", &[schema(NO_ERRORS_SCHEMA)]));
        assert!(registry.has_property("a", "unknown", &[schema(NO_ERRORS_SCHEMA)]));

        assert!(registry.has_element("custom-like", &[schema(NO_ERRORS_SCHEMA)]));
        assert!(registry.has_element("unknown", &[schema(NO_ERRORS_SCHEMA)]));
    }

    #[test]
    fn should_re_map_property_names_that_are_specified_in_dom_facade() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.get_mapped_prop_name("readonly"), "readOnly");
    }

    #[test]
    fn should_not_re_map_property_names_that_are_not_specified_in_dom_facade() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.get_mapped_prop_name("title"), "title");
        assert_eq!(registry.get_mapped_prop_name("exotic-unknown"), "exotic-unknown");
    }

    #[test]
    fn should_return_an_error_message_when_asserting_event_properties() {
        let registry = DomElementSchemaRegistry::new();
        let report = registry.validate_property("onClick");
        assert!(report.error);
        assert_eq!(
            report.msg.as_deref(),
            Some(
                "Binding to event property 'onClick' is disallowed for security reasons, please use (Click)=...
If 'onClick' is a directive input, make sure the directive is imported by the current module."
            )
        );

        let report = registry.validate_property("onAnything");
        assert!(report.error);
        assert_eq!(
            report.msg.as_deref(),
            Some(
                "Binding to event property 'onAnything' is disallowed for security reasons, please use (Anything)=...
If 'onAnything' is a directive input, make sure the directive is imported by the current module."
            )
        );
    }

    #[test]
    fn should_return_an_error_message_when_asserting_event_attributes() {
        let registry = DomElementSchemaRegistry::new();
        let report = registry.validate_attribute("onClick");
        assert!(report.error);
        assert_eq!(
            report.msg.as_deref(),
            Some(
                "Binding to event attribute 'onClick' is disallowed for security reasons, please use (Click)=..."
            )
        );

        let report = registry.validate_attribute("onAnything");
        assert!(report.error);
        assert_eq!(
            report.msg.as_deref(),
            Some(
                "Binding to event attribute 'onAnything' is disallowed for security reasons, please use (Anything)=..."
            )
        );
    }

    #[test]
    fn should_not_return_an_error_message_when_asserting_non_event_properties_or_attributes() {
        let registry = DomElementSchemaRegistry::new();
        let report = registry.validate_property("title");
        assert!(!report.error);
        assert_eq!(report.msg, None);

        let report = registry.validate_property("exotic-unknown");
        assert!(!report.error);
        assert_eq!(report.msg, None);
    }

    #[test]
    fn should_return_security_contexts_for_elements() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.security_context("iframe", "srcdoc", false), SecurityContext::Html);
        assert_eq!(registry.security_context("p", "innerHTML", false), SecurityContext::Html);
        assert_eq!(registry.security_context("a", "href", false), SecurityContext::Url);
        assert_eq!(registry.security_context("a", "style", false), SecurityContext::Style);
        assert_eq!(registry.security_context("base", "href", false), SecurityContext::ResourceUrl);
        assert_eq!(
            registry.security_context("iframe", "credentialless", false),
            SecurityContext::AttributeNoBinding
        );

        // SVG animate and set attributes
        assert_eq!(
            registry.security_context(":svg:animate", "to", false),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context(":svg:animate", "from", false),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context(":svg:animate", "values", false),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context(":svg:set", "to", false),
            SecurityContext::AttributeNoBinding
        );

        // SVG animation elements are sensitive when their namespace is omitted at compile time.
        assert_eq!(
            registry.security_context("set", "to", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("set", "attributeName", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animate", "to", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animate", "from", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animate", "values", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animate", "attributeName", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animateMotion", "attributeName", true),
            SecurityContext::AttributeNoBinding
        );
        assert_eq!(
            registry.security_context("animateTransform", "attributeName", true),
            SecurityContext::AttributeNoBinding
        );

        // The defensive SVG lookup must not apply to elements in an explicit foreign namespace.
        assert_eq!(registry.security_context(":math:set", "to", true), SecurityContext::None);

        // SVG link attributes
        assert_eq!(registry.security_context(":svg:a", "href", false), SecurityContext::Url);
        assert_eq!(registry.security_context(":svg:a", "xlink:href", false), SecurityContext::Url);
        assert_eq!(registry.security_context(":svg:a", "href", true), SecurityContext::Url);
        assert_eq!(registry.security_context(":svg:a", "xlink:href", true), SecurityContext::Url);

        // MathML link attributes
        assert_eq!(registry.security_context(":math:math", "href", false), SecurityContext::Url);
        assert_eq!(
            registry.security_context(":math:math", "xlink:href", false),
            SecurityContext::Url
        );
        assert_eq!(registry.security_context(":math:math", "href", true), SecurityContext::Url);
        assert_eq!(
            registry.security_context(":math:math", "xlink:href", true),
            SecurityContext::Url
        );
        // Making sure we're have a wild card match for MathML elements with the correct security context
        assert_eq!(registry.security_context(":math:foobar", "href", true), SecurityContext::Url);
        assert_eq!(
            registry.security_context(":math:foobar", "xlink:href", true),
            SecurityContext::Url
        );
        assert_eq!(registry.security_context(":math:foobar", "href", false), SecurityContext::Url);
        assert_eq!(
            registry.security_context(":math:foobar", "xlink:href", false),
            SecurityContext::Url
        );
        // Ensure MathML wildcard does not apply outside of the MathML namespace.
        assert_eq!(registry.security_context(":svg:foobar", "href", false), SecurityContext::None);
        assert_eq!(
            registry.security_context(":svg:foobar", "xlink:href", false),
            SecurityContext::None
        );

        assert_eq!(registry.security_context("p", "href", false), SecurityContext::None);
    }

    #[test]
    fn should_detect_properties_on_namespaced_elements() {
        let registry = DomElementSchemaRegistry::new();
        // `new HtmlParser().parse('<svg:style>', 'TestComp').rootNodes[0].name`
        let node_name = ":svg:style";
        assert!(registry.has_property(node_name, "type", &[]));
    }

    #[test]
    fn should_check_security_contexts_case_insensitive() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.security_context("p", "iNnErHtMl", false), SecurityContext::Html);
        assert_eq!(registry.security_context("p", "formaction", false), SecurityContext::Url);
        assert_eq!(registry.security_context("p", "formAction", false), SecurityContext::Url);
    }

    #[test]
    fn should_check_security_contexts_for_attributes() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.security_context("p", "innerHtml", true), SecurityContext::Html);
        assert_eq!(registry.security_context("p", "formaction", true), SecurityContext::Url);
    }

    // describe('Angular custom elements')

    #[test]
    fn should_support_ng_container() {
        let registry = DomElementSchemaRegistry::new();
        assert!(!registry.has_property("ng-container", "id", &[]));
    }

    #[test]
    fn should_support_ng_content() {
        let registry = DomElementSchemaRegistry::new();
        assert!(!registry.has_property("ng-content", "id", &[]));
        assert!(!registry.has_property("ng-content", "select", &[]));
    }

    // describe('Custom XML / XHTML namespaces')

    #[test]
    fn should_support_elements_with_custom_namespaces() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_element(":xhtml:a", &[]));
        assert!(registry.has_element(":foo:div", &[]));
    }

    #[test]
    fn should_support_properties_on_custom_namespaced_elements() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.has_property(":xhtml:a", "href", &[]));
        assert!(registry.has_property(":foo:div", "id", &[]));
    }

    #[test]
    fn should_return_correct_security_contexts_for_custom_namespaced_elements() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.security_context(":xhtml:a", "href", false), SecurityContext::Url);
        assert_eq!(
            registry.security_context(":foo:div", "innerHTML", false),
            SecurityContext::Html
        );
    }

    // describe('normalizeAnimationStyleProperty')

    #[test]
    fn should_normalize_the_given_css_property_to_camel_case() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(registry.normalize_animation_style_property("border-radius"), "borderRadius");
        assert_eq!(registry.normalize_animation_style_property("zIndex"), "zIndex");
        assert_eq!(
            registry.normalize_animation_style_property("-webkit-animation"),
            "WebkitAnimation"
        );
    }

    // describe('normalizeAnimationStyleValue')

    #[test]
    fn should_normalize_the_given_dimensional_css_style_value_to_contain_a_px_value_when_numeric() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(
            registry
                .normalize_animation_style_value(
                    "borderRadius",
                    "border-radius",
                    StyleValue::Number(10.0)
                )
                .value,
            "10px"
        );
    }

    #[test]
    fn should_not_normalize_any_values_that_are_of_zero() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(
            registry
                .normalize_animation_style_value("opacity", "opacity", StyleValue::Number(0.0))
                .value,
            "0"
        );
        assert_eq!(
            registry
                .normalize_animation_style_value("width", "width", StyleValue::Number(0.0))
                .value,
            "0"
        );
    }

    #[test]
    fn should_retain_the_given_dimensional_css_style_values_unit_if_it_already_exists() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(
            registry
                .normalize_animation_style_value(
                    "borderRadius",
                    "border-radius",
                    StyleValue::String("10em")
                )
                .value,
            "10em"
        );
    }

    #[test]
    fn should_trim_the_provided_css_style_value() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(
            registry
                .normalize_animation_style_value("color", "color", StyleValue::String("   red "))
                .value,
            "red"
        );
    }

    #[test]
    fn should_stringify_all_non_dimensional_numeric_style_values() {
        let registry = DomElementSchemaRegistry::new();
        assert_eq!(
            registry
                .normalize_animation_style_value("zIndex", "zIndex", StyleValue::Number(10.0))
                .value,
            "10"
        );
        assert_eq!(
            registry
                .normalize_animation_style_value("opacity", "opacity", StyleValue::Number(0.5))
                .value,
            "0.5"
        );
    }

    #[test]
    fn should_support_aria_property_if_attribute_is_also_supported() {
        let elementschema = SCHEMA[0];

        for prop in ATTR_TO_PROP.values().filter(|prop| prop.starts_with("aria")) {
            assert!(elementschema.contains(prop), "{prop}");
        }
    }

    // Not in the spec.

    #[test]
    fn unitless_dimension_strings_report_an_error() {
        let registry = DomElementSchemaRegistry::new();
        let result =
            registry.normalize_animation_style_value("width", "width", StyleValue::String("10"));
        assert_eq!(result.error.as_deref(), Some("Please provide a CSS unit value for width:10"));
        assert_eq!(result.value, "10");
    }

    #[test]
    fn js_number_to_string_matches_javascript() {
        for (value, expected) in [
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1, "0.1"),
            (123_456.789, "123456.789"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-7, "1.5e-7"),
            (0.000_001, "0.000001"),
        ] {
            assert_eq!(js_number_to_string(value), expected);
        }
    }

    #[test]
    fn known_events() {
        let registry = DomElementSchemaRegistry::new();
        assert!(registry.all_known_events_of_element("div").contains(&"click".to_string()));
        assert!(registry.is_known_event_of_any_element("webkitAnimationEnd"));
        assert!(!registry.is_known_event_of_any_element("not-an-event"));
        assert!(registry.all_known_attributes_of_element("div").contains(&"className".to_string()));
    }
}
