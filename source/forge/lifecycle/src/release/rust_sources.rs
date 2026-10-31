//! Exact files statically reachable from Cargo entries outside their package.
//! All reads use the snapshot provider: historical graphs never consult disk.
use super::*;
use syn::visit::Visit;

pub(super) fn reachable(
    entries: &[String],
    inventory: &BTreeSet<String>,
    inputs: &mut BTreeMap<String, Option<Vec<u8>>>,
    read: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<BTreeSet<String>, String> {
    let _profile = crate::release_metrics::span("source_graph.traversal");
    let mut graph = Graph {
        inventory,
        inputs,
        read,
        files: BTreeSet::new(),
        visited: BTreeSet::new(),
        active: BTreeSet::new(),
    };
    for entry in entries {
        if inventory.contains(entry) {
            graph.source(entry, directory(entry), false)?;
        }
    }
    Ok(graph.files)
}

struct Graph<'a, F> {
    inventory: &'a BTreeSet<String>,
    inputs: &'a mut BTreeMap<String, Option<Vec<u8>>>,
    read: &'a mut F,
    files: BTreeSet<String>,
    visited: BTreeSet<(String, String, bool)>,
    active: BTreeSet<(String, String, bool)>,
}

impl<F: FnMut(&str) -> Result<Option<Vec<u8>>, String>> Graph<'_, F> {
    fn bytes(&mut self, path: &str) -> Result<Vec<u8>, String> {
        if !self.inventory.contains(path) {
            return Err(format!(
                "Rust source graph file is outside repository inventory: {path}"
            ));
        }
        let bytes = (self.read)(path)?
            .ok_or_else(|| format!("Rust source graph file is missing: {path}"))?;
        if let Some(previous) = self.inputs.insert(path.into(), Some(bytes.clone())) {
            if previous.as_ref() != Some(&bytes) {
                return Err(format!("Rust source graph changed while reading: {path}"));
            }
        }
        self.files.insert(path.into());
        Ok(bytes)
    }

    fn source(&mut self, path: &str, module_dir: &str, conditional: bool) -> Result<(), String> {
        let key = (path.to_string(), module_dir.to_string(), conditional);
        if self.active.contains(&key) || self.active.len() >= 256 {
            return Err(format!(
                "cyclic or excessively nested Rust source graph: {path}"
            ));
        }
        if !self.visited.insert(key.clone()) {
            return Ok(());
        }
        self.active.insert(key.clone());
        let bytes = self.bytes(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| format!("Rust source is not UTF-8: {path}: {e}"))?;
        let mut visitor = Walker {
            graph: self,
            file: path.into(),
            module_dir: module_dir.into(),
            path_dir: directory(path).into(),
            conditional,
            error: None,
        };
        let parse_span = crate::release_metrics::span("source_graph.parse");
        let parsed = syn::parse_file(text);
        drop(parse_span);
        if let Ok(file) = parsed {
            visitor.conditional |= file
                .attrs
                .iter()
                .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"));
            visitor.visit_file(&file);
        } else if let Ok(expression) = syn::parse_str::<syn::Expr>(text) {
            visitor.visit_expr(&expression);
        } else {
            return Err(format!(
                "cannot parse statically referenced Rust source: {path}"
            ));
        }
        let result = visitor.error.take().map_or(Ok(()), Err);
        self.active.remove(&key);
        result
    }
}

struct Walker<'a, 'b, F> {
    graph: &'a mut Graph<'b, F>,
    file: String,
    module_dir: String,
    path_dir: String,
    conditional: bool,
    error: Option<String>,
}

fn path_value(meta: &syn::Meta) -> Result<Option<String>, String> {
    if !meta.path().is_ident("path") {
        return Ok(None);
    }
    if let syn::Meta::NameValue(value) = meta {
        if let syn::Expr::Lit(expr) = &value.value {
            if let syn::Lit::Str(value) = &expr.lit {
                return Ok(Some(value.value()));
            }
        }
    }
    Err("Rust module path attribute must be a string literal".into())
}

fn conditional(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
}

macro_rules! conditional_enum {
    ($visit:ident, $kind:ident, [$($variant:ident),*]) => {
        fn $visit(&mut self, node: &'ast syn::$kind) {
            let attrs = match node { $(syn::$kind::$variant(value) => value.attrs.as_slice(),)* _ => &[] };
            let previous = self.conditional;
            self.conditional |= conditional(attrs);
            syn::visit::$visit(self, node);
            self.conditional = previous;
        }
    };
}

macro_rules! conditional_node {
    ($visit:ident, $kind:ident) => {
        fn $visit(&mut self, node: &'ast syn::$kind) {
            let previous = self.conditional;
            self.conditional |= conditional(&node.attrs);
            syn::visit::$visit(self, node);
            self.conditional = previous;
        }
    };
}

impl<F: FnMut(&str) -> Result<Option<Vec<u8>>, String>> Walker<'_, '_, F> {
    fn module(&mut self, module: &syn::ItemMod) -> Result<(), String> {
        let mut direct = Vec::new();
        let mut alternatives = Vec::new();
        let mut conditional = self.conditional;
        for attribute in &module.attrs {
            if let Some(path) = path_value(&attribute.meta)? {
                direct.push(path);
            }
            if attribute.path().is_ident("cfg") {
                conditional = true;
            }
            if attribute.path().is_ident("cfg_attr") {
                conditional = true;
                let arguments = attribute
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .map_err(|e| format!("cannot parse cfg_attr in {}: {e}", self.file))?;
                for meta in arguments.iter().skip(1) {
                    if meta.path().is_ident("cfg_attr") {
                        return Err(format!(
                            "nested cfg_attr requires explicit source mapping: {}",
                            self.file
                        ));
                    }
                    if let Some(path) = path_value(meta)? {
                        alternatives.push(path);
                    }
                }
            }
        }
        if direct.len() > 1 {
            return Err(format!(
                "multiple unconditional module paths: {}",
                self.file
            ));
        }
        // The custom module traversal must still visit attribute expressions,
        // e.g. #[doc = include_str!("module.md")], in the containing file.
        let previous = self.conditional;
        self.conditional = conditional;
        for attribute in &module.attrs {
            self.visit_attribute(attribute);
        }
        self.conditional = previous;
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        let name = module
            .ident
            .to_string()
            .trim_start_matches("r#")
            .to_string();
        let mut directories = Vec::new();
        if let Some(path) = direct.first() {
            alternatives.push(path.clone());
        }
        if direct.is_empty() {
            directories.push(normalize(&self.module_dir, &name)?);
        }
        if let Some((_, items)) = &module.content {
            for path in alternatives {
                directories.push(normalize(&self.path_dir, &path)?);
            }
            directories.sort();
            directories.dedup();
            for dir in directories {
                let mut child = Walker {
                    graph: self.graph,
                    file: self.file.clone(),
                    module_dir: dir.clone(),
                    path_dir: dir,
                    conditional,
                    error: None,
                };
                for item in items {
                    child.visit_item(item);
                }
                if let Some(error) = child.error {
                    return Err(error);
                }
            }
            return Ok(());
        }
        let mut candidates = BTreeMap::new();
        for path in alternatives {
            let path = normalize(&self.path_dir, &path)?;
            // An explicit #[path] owns its physical directory, including for
            // ordinary submodules in an arbitrarily named source file.
            candidates.insert(path.clone(), directory(&path).to_string());
        }
        if direct.is_empty() {
            let dir = normalize(&self.module_dir, &name)?;
            let regular = format!("{dir}.rs");
            let legacy = format!("{dir}/mod.rs");
            if self.graph.inventory.contains(&regular) && self.graph.inventory.contains(&legacy) {
                return Err(format!(
                    "ambiguous Rust module files: {regular} and {legacy}"
                ));
            }
            for path in [regular, legacy] {
                if self.graph.inventory.contains(&path) {
                    candidates.insert(path, dir.clone());
                }
            }
        }
        candidates.retain(|path, _| self.graph.inventory.contains(path));
        // Native history can retain stale module declarations. Ownership scans
        // follow only files in this snapshot, without validating compilation.
        for (path, dir) in candidates {
            self.graph.source(&path, &dir, conditional)?;
        }
        Ok(())
    }

    fn included(&mut self, mac: &syn::Macro) -> Result<(), String> {
        let segments = mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>();
        let name = match segments.as_slice() {
            [name] => name.as_str(),
            [namespace, name] if matches!(namespace.as_str(), "std" | "core") => name.as_str(),
            _ => return Ok(()),
        };
        if !matches!(name, "include" | "include_str" | "include_bytes") {
            return Ok(());
        }
        // Like other macro expansion, nonliteral paths are opaque. In
        // particular never consult host env or run a build script to guess
        // historical consumers; dynamic sharing requires shared_inputs.
        let Ok(arguments) = mac.parse_body_with(
            syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated,
        ) else {
            return Ok(());
        };
        if arguments.len() != 1 {
            return Err(format!(
                "Rust {name}! requires one static path: {}",
                self.file
            ));
        }
        let path = normalize(directory(&self.file), &arguments.first().unwrap().value())?;
        if !self.graph.inventory.contains(&path) {
            return Ok(());
        }
        if name == "include" {
            self.graph.source(&path, directory(&path), self.conditional)
        } else {
            self.graph.bytes(&path).map(|_| ())
        }
    }
}

impl<'ast, F: FnMut(&str) -> Result<Option<Vec<u8>>, String>> Visit<'ast> for Walker<'_, '_, F> {
    conditional_enum!(
        visit_item,
        Item,
        [
            Const,
            Enum,
            ExternCrate,
            Fn,
            ForeignMod,
            Impl,
            Macro,
            Mod,
            Static,
            Struct,
            Trait,
            TraitAlias,
            Type,
            Union,
            Use
        ]
    );
    conditional_enum!(
        visit_expr,
        Expr,
        [
            Array, Assign, Async, Await, Binary, Block, Break, Call, Cast, Closure, Const,
            Continue, Field, ForLoop, Group, If, Index, Infer, Let, Lit, Loop, Macro, Match,
            MethodCall, Paren, Path, Range, RawAddr, Reference, Repeat, Return, Struct, Try,
            TryBlock, Tuple, Unary, Unsafe, While, Yield
        ]
    );
    conditional_enum!(visit_impl_item, ImplItem, [Const, Fn, Type, Macro]);
    conditional_enum!(visit_trait_item, TraitItem, [Const, Fn, Type, Macro]);
    conditional_enum!(visit_foreign_item, ForeignItem, [Fn, Static, Type, Macro]);
    conditional_node!(visit_local, Local);
    conditional_node!(visit_arm, Arm);
    conditional_node!(visit_field, Field);
    conditional_node!(visit_variant, Variant);
    conditional_node!(visit_stmt_macro, StmtMacro);
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if !node.path().is_ident("cfg_attr") {
            syn::visit::visit_attribute(self, node);
            return;
        }
        if self.error.is_some() {
            return;
        }
        let arguments = match node.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        ) {
            Ok(arguments) => arguments,
            Err(error) => {
                self.error = Some(format!("cannot parse cfg_attr in {}: {error}", self.file));
                return;
            }
        };
        let previous = self.conditional;
        self.conditional = true;
        for meta in arguments.iter().skip(1) {
            if meta.path().is_ident("cfg_attr") {
                self.error = Some(format!(
                    "nested cfg_attr requires explicit source mapping: {}",
                    self.file
                ));
                break;
            }
            self.visit_meta(meta);
        }
        self.conditional = previous;
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if self.error.is_none() {
            if let Err(error) = self.module(node) {
                self.error = Some(error);
            }
        }
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.error.is_none() {
            if let Err(error) = self.included(node) {
                self.error = Some(error);
            }
        }
    }
}
