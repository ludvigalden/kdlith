//! Reading one construct node: its arguments and properties are taken
//! explicitly, and whatever was not taken fails closed on finish, with
//! the binding codes every processor of the document reports.

use kdlith_model::Value;

use crate::compile::Cx;
use crate::decode::found;
use crate::diag::{DUPLICATE_PROPERTY, Site, UNBOUND_ENTRY};
use crate::syntax::{Scalar, SyntaxNode, parse_u64};

/// A construct node being read.
pub(crate) struct Construct<'n> {
    node: &'n SyntaxNode,
    doc: usize,
    args: Vec<bool>,
    props: Vec<bool>,
}

impl<'n> Construct<'n> {
    pub(crate) fn new(cx: &mut Cx<'_>, doc: usize, node: &'n SyntaxNode) -> Self {
        let construct = Self {
            node,
            doc,
            args: vec![false; node.args.len()],
            props: vec![false; node.props.len()],
        };
        let unbound = |cx: &mut Cx<'_>, span, what: &str| {
            let site = Site::new(doc, span);
            cx.diags.error(UNBOUND_ENTRY, site, &[what, &node.name]);
        };
        if node.annotated {
            unbound(cx, node.span, "the type annotation");
        }
        for (at, (name, scalar)) in node.props.iter().enumerate() {
            let site = Site::new(doc, scalar.span);
            if node.props[..at].iter().any(|(earlier, _)| earlier == name) {
                cx.diags.error(DUPLICATE_PROPERTY, site, &[name]);
            }
            if scalar.annotated {
                let what = format!("the type annotation of property {name}");
                unbound(cx, scalar.span, &what);
            }
        }
        for (at, scalar) in node.args.iter().enumerate() {
            if scalar.annotated {
                let what = format!("the type annotation of argument {at}");
                unbound(cx, scalar.span, &what);
            }
        }
        construct
    }

    pub(crate) fn site(&self) -> Site {
        Site::new(self.doc, self.node.span)
    }

    /// The node name, with its first argument when that is a string.
    pub(crate) fn label(&self) -> String {
        match self.node.args.first().map(|scalar| &scalar.value) {
            Some(Value::String(text)) => format!("{} {text}", self.node.name),
            _ => self.node.name.clone(),
        }
    }

    fn arg(&mut self, at: usize) -> Option<&'n Scalar> {
        let scalar = self.node.args.get(at)?;
        self.args[at] = true;
        Some(scalar)
    }

    /// A required argument is absent.
    fn missing(&self, cx: &mut Cx<'_>, what: &str) {
        cx.missing(self.site(), &self.node.name, what);
    }

    /// A required string argument.
    pub(crate) fn string_arg(&mut self, cx: &mut Cx<'_>, at: usize, what: &str) -> Option<String> {
        match self.arg(at) {
            Some(scalar) => self.string(cx, scalar, what),
            None => {
                self.missing(cx, what);
                None
            }
        }
    }

    /// An optional string argument.
    pub(crate) fn opt_string_arg(
        &mut self,
        cx: &mut Cx<'_>,
        at: usize,
        what: &str,
    ) -> Option<String> {
        let scalar = self.arg(at)?;
        self.string(cx, scalar, what)
    }

    /// A required non-negative integer argument.
    pub(crate) fn u64_arg(&mut self, cx: &mut Cx<'_>, at: usize, what: &str) -> Option<u64> {
        match self.arg(at) {
            Some(scalar) => self.unsigned(cx, scalar, what),
            None => {
                self.missing(cx, what);
                None
            }
        }
    }

    /// A required single value of any scalar kind.
    pub(crate) fn value_arg(&mut self, cx: &mut Cx<'_>, at: usize, what: &str) -> Option<Value> {
        match self.arg(at) {
            Some(scalar) => Some(scalar.value.clone()),
            None => {
                self.missing(cx, what);
                None
            }
        }
    }

    /// An optional single value of any scalar kind; absent stays absent
    /// without a diagnostic.
    pub(crate) fn opt_value_arg(&mut self, at: usize) -> Option<Value> {
        let scalar = self.arg(at)?;
        Some(scalar.value.clone())
    }

    /// Every string argument from `from` on.
    pub(crate) fn rest_strings(&mut self, cx: &mut Cx<'_>, from: usize, what: &str) -> Vec<String> {
        let mut out = Vec::new();
        for at in from..self.node.args.len() {
            if let Some(scalar) = self.arg(at)
                && let Some(text) = self.string(cx, scalar, what)
            {
                out.push(text);
            }
        }
        out
    }

    fn prop(&mut self, key: &str) -> Option<&'n Scalar> {
        let mut found = None;
        for (at, (name, scalar)) in self.node.props.iter().enumerate() {
            if name == key {
                self.props[at] = true;
                found = Some(scalar);
            }
        }
        found
    }

    pub(crate) fn string_prop(&mut self, cx: &mut Cx<'_>, key: &str) -> Option<String> {
        let scalar = self.prop(key)?;
        self.string(cx, scalar, key)
    }

    pub(crate) fn u64_prop(&mut self, cx: &mut Cx<'_>, key: &str) -> Option<u64> {
        let scalar = self.prop(key)?;
        self.unsigned(cx, scalar, key)
    }

    pub(crate) fn value_prop(&mut self, key: &str) -> Option<Value> {
        self.prop(key).map(|scalar| scalar.value.clone())
    }

    fn string(&self, cx: &mut Cx<'_>, scalar: &Scalar, what: &str) -> Option<String> {
        if let Value::String(text) = &scalar.value {
            return Some(text.clone());
        }
        let site = Site::new(self.doc, scalar.span);
        let what = format!("{what} of {}", self.node.name);
        cx.mistyped(site, &what, "String", &found(&scalar.value));
        None
    }

    fn unsigned(&self, cx: &mut Cx<'_>, scalar: &Scalar, what: &str) -> Option<u64> {
        if let Value::Number(text) = &scalar.value
            && let Some(number) = parse_u64(text)
        {
            return Some(number);
        }
        let site = Site::new(self.doc, scalar.span);
        let what = format!("{what} of {}", self.node.name);
        cx.mistyped(site, &what, "U64", &found(&scalar.value));
        None
    }

    /// Reject every argument and property that was not read.
    pub(crate) fn finish(self, cx: &mut Cx<'_>) {
        let node = self.node.name.as_str();
        for (at, scalar) in self.node.args.iter().enumerate() {
            if !self.args[at] {
                let site = Site::new(self.doc, scalar.span);
                let what = format!("argument {at}");
                cx.diags.error(UNBOUND_ENTRY, site, &[&what, node]);
            }
        }
        for (at, (name, scalar)) in self.node.props.iter().enumerate() {
            if self.props[at]
                || self.node.props[..at]
                    .iter()
                    .any(|(earlier, _)| earlier == name)
            {
                continue;
            }
            let site = Site::new(self.doc, scalar.span);
            let what = format!("property {name}");
            cx.diags.error(UNBOUND_ENTRY, site, &[&what, node]);
        }
    }

    /// Finish a construct that takes no children.
    pub(crate) fn finish_leaf(self, cx: &mut Cx<'_>) {
        for child in &self.node.children {
            cx.unknown(self.doc, child, &self.node.name);
        }
        self.finish(cx);
    }
}
