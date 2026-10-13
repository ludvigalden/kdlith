//! Reading one construct node: its arguments and properties are taken
//! explicitly, and whatever was not taken fails closed on finish.

use kdlith_model::Value;

use crate::compile::Cx;
use crate::deferred::{Spelling, deferred_by};
use crate::diag::{DEFERRED_MECHANISM, DUPLICATE_PROPERTY, Site};
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
        let label = construct.label();
        if node.annotated {
            cx.malformed(
                construct.site(),
                &label,
                "type annotations are not part of the dialect",
            );
        }
        for (at, (name, scalar)) in node.props.iter().enumerate() {
            let site = Site::new(doc, scalar.span);
            if node.props[..at].iter().any(|(earlier, _)| earlier == name) {
                cx.diags.error(DUPLICATE_PROPERTY, site, &[name]);
            }
            if scalar.annotated {
                cx.malformed(site, &label, "type annotations are not part of the dialect");
            }
        }
        for scalar in node.args.iter().filter(|scalar| scalar.annotated) {
            let site = Site::new(doc, scalar.span);
            cx.malformed(site, &label, "type annotations are not part of the dialect");
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

    /// A required string argument.
    pub(crate) fn string_arg(&mut self, cx: &mut Cx<'_>, at: usize, what: &str) -> Option<String> {
        match self.arg(at) {
            Some(scalar) => self.string(cx, scalar, what),
            None => {
                cx.malformed(self.site(), &self.label(), &format!("missing {what}"));
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
                cx.malformed(self.site(), &self.label(), &format!("missing {what}"));
                None
            }
        }
    }

    /// A required single value of any scalar kind.
    pub(crate) fn value_arg(&mut self, cx: &mut Cx<'_>, at: usize, what: &str) -> Option<Value> {
        match self.arg(at) {
            Some(scalar) => Some(scalar.value.clone()),
            None => {
                cx.malformed(self.site(), &self.label(), &format!("missing {what}"));
                None
            }
        }
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
        cx.malformed(site, &self.label(), &format!("{what} must be a string"));
        None
    }

    fn unsigned(&self, cx: &mut Cx<'_>, scalar: &Scalar, what: &str) -> Option<u64> {
        if let Value::Number(text) = &scalar.value
            && let Some(number) = parse_u64(text)
        {
            return Some(number);
        }
        let site = Site::new(self.doc, scalar.span);
        cx.malformed(
            site,
            &self.label(),
            &format!("{what} must be a non-negative integer"),
        );
        None
    }

    /// Reject every argument and property that was not read.
    pub(crate) fn finish(self, cx: &mut Cx<'_>) {
        let label = self.label();
        for (at, scalar) in self.node.args.iter().enumerate() {
            if !self.args[at] {
                let site = Site::new(self.doc, scalar.span);
                cx.malformed(site, &label, &format!("unexpected argument {at}"));
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
            match deferred_by(&cx.deferrals, Spelling::Property(name)) {
                Some(mechanism) => {
                    let construct = format!("{label} property {name}");
                    let mechanism = mechanism.to_owned();
                    cx.diags
                        .error(DEFERRED_MECHANISM, site, &[&construct, &mechanism]);
                }
                None => cx.malformed(site, &label, &format!("unknown property {name}")),
            }
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
