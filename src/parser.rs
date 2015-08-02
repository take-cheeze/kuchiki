use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::option;
use std::path::Path;
use html5ever::{self, Attribute};
use html5ever::tree_builder::{TreeSink, NodeOrText, QuirksMode};
use string_cache::QualName;
use tendril::{StrTendril, ReadExt, Tendril};

use tree::NodeRef;

/// The HTML parser.
pub struct Html {
    opts: ParseOpts,
    data: option::IntoIter<StrTendril>,
}

impl Html  {
    /// Parse from a single string in memory.
    #[inline]
    pub fn from_string<S: Into<StrTendril>>(string: S) -> Html {
        Html {
            opts: ParseOpts::default(),
            data: Some(string.into()).into_iter(),
        }
    }

    /// Parse from reading a file.
    #[inline]
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Html, Error> {
        Html::from_stream(&mut try!(File::open(&path)))
    }

    /// Parse from reading a stream of bytes.
    #[inline]
    pub fn from_stream<S: Read>(stream: &mut S) -> Result<Html, Error> {
        let mut buf = Tendril::new();
        try!(stream.read_to_tendril(&mut buf));
        Ok(Html {
            opts: ParseOpts::default(),
            // FIXME: Make UTF-8 decoding lossy, but try to minimize copying.
            data: Some(try!(buf.try_reinterpret().map_err(|_| {
                Error::new(ErrorKind::Other, "Invalid UTF-8.")
            }))).into_iter(),
        })
    }

    /// Run the parser and return a reference to the document node, the root of the tree.
    #[inline]
    pub fn parse(self) -> NodeRef {
        let parser = Parser {
            document_node: NodeRef::new_document(),
            on_parse_error: self.opts.on_parse_error,
        };
        let html5opts = html5ever::ParseOpts {
            tokenizer: self.opts.tokenizer,
            tree_builder: self.opts.tree_builder,
        };
        let parser = html5ever::parse_to(parser, self.data, html5opts);
        parser.document_node
    }

}

/// Options for the HTML parser.
#[derive(Default)]
pub struct ParseOpts {
    /// Options for the HTML tokenizer.
    pub tokenizer: html5ever::tokenizer::TokenizerOpts,

    /// Options for the HTML tree builder.
    pub tree_builder: html5ever::tree_builder::TreeBuilderOpts,

    /// A callback for HTML parse errors (which are never fatal).
    pub on_parse_error: Option<Box<FnMut(Cow<'static, str>)>>,
}


struct Parser {
    document_node: NodeRef,
    on_parse_error: Option<Box<FnMut(Cow<'static, str>)>>,
}


impl TreeSink for Parser {
    type Handle = NodeRef;

    #[inline]
    fn parse_error(&mut self, message: Cow<'static, str>) {
        if let Some(ref mut handler) = self.on_parse_error {
            handler(message)
        }
    }

    #[inline]
    fn get_document(&mut self) -> NodeRef {
        self.document_node.clone()
    }

    #[inline]
    fn set_quirks_mode(&mut self, mode: QuirksMode) {
        self.document_node.as_document().unwrap()._quirks_mode.set(mode)
    }

    #[inline]
    fn same_node(&self, x: NodeRef, y: NodeRef) -> bool {
        x == y
    }

    #[inline]
    fn elem_name(&self, target: NodeRef) -> QualName {
        target.as_element().unwrap().name.clone()
    }

    #[inline]
    fn create_element(&mut self, name: QualName, attrs: Vec<Attribute>) -> NodeRef {
        let attrs = attrs.into_iter().map(|Attribute { name, value }| (name, value.into()));
        NodeRef::new_element(name, attrs)
    }

    #[inline]
    fn create_comment(&mut self, text: StrTendril) -> NodeRef {
        NodeRef::new_comment(text)
    }

    #[inline]
    fn append(&mut self, parent: NodeRef, child: NodeOrText<NodeRef>) {
        match child {
            NodeOrText::AppendNode(node) => parent.append(node),
            NodeOrText::AppendText(text) => {
                if let Some(last_child) = parent.last_child() {
                    if let Some(existing) = last_child.as_text() {
                        existing.borrow_mut().push_str(&text);
                        return
                    }
                }
                parent.append(NodeRef::new_text(text))
            }
        }
    }

    #[inline]
    fn append_before_sibling(&mut self, sibling: NodeRef, child: NodeOrText<NodeRef>)
                             -> Result<(), NodeOrText<NodeRef>> {
        if sibling.parent().is_none() {
            return Err(child)
        }
        match child {
            NodeOrText::AppendNode(node) => sibling.insert_before(node),
            NodeOrText::AppendText(text) => {
                if let Some(previous_sibling) = sibling.previous_sibling() {
                    if let Some(existing) = previous_sibling.as_text() {
                        existing.borrow_mut().push_str(&text);
                        return Ok(())
                    }
                }
                sibling.insert_before(NodeRef::new_text(text))
            }
        }
        Ok(())
    }

    #[inline]
    fn append_doctype_to_document(&mut self, name: StrTendril, public_id: StrTendril,
                                  system_id: StrTendril) {
        self.document_node.append(NodeRef::new_doctype(name, public_id, system_id))
    }

    #[inline]
    fn add_attrs_if_missing(&mut self, target: NodeRef, attrs: Vec<Attribute>) {
        // FIXME: https://github.com/servo/html5ever/issues/121
        if let Some(element) = target.as_element() {
            let mut attributes = element.attributes.borrow_mut();
            for Attribute { name, value } in attrs {
                use std::collections::hash_map::Entry;
                match attributes.entry(name) {
                    Entry::Vacant(entry) => {
                        entry.insert(value.into());
                    }
                    Entry::Occupied(mut entry) => {
                        *entry.get_mut() = value.into();
                    }
                }
            }
        }
    }

    #[inline]
    fn remove_from_parent(&mut self, target: NodeRef) {
        target.detach()
    }

    #[inline]
    fn reparent_children(&mut self, node: NodeRef, new_parent: NodeRef) {
        // FIXME: Can this be done more effciently in rctree,
        // by moving the whole linked list of children at once?
        for child in node.children() {
            new_parent.append(child)
        }
    }

    #[inline]
    fn mark_script_already_started(&mut self, _node: NodeRef) {
        // FIXME: Is this useful outside of a browser?
    }
}
