//! Source candidates require owner review; syntax is not semantic proof.
use crate::Result;
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use syn::{
    parse::Parser,
    spanned::Spanned,
    visit::{self, Visit},
};

pub(super) fn run(args: &[String], root: &Path) -> Result<()> {
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--report"] => {
            let report = inventory(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        ["--check", ledger] => {
            let path = Path::new(ledger);
            let ledger: Value = serde_json::from_str(&read(path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut report = inventory(root)?;
            report["feature_recipes"] = ledger["feature_recipes"].clone();
            let counts = check(&report, &ledger)?;
            println!(
                "quality: {} open debt, {} retained proofs, {} required changes",
                counts[0], counts[1], counts[2]
            );
            Ok(())
        }
        _ => Err("usage: cargo xtask quality --report | --check LEDGER".into()),
    }
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn fingerprint(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn testing(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|meta| test_meta(&meta)))
    })
}

fn test_meta(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|items| items.iter().any(test_meta)),
        // any(test, feature) can contain production code, so retain it.
        _ => false,
    }
}

fn allowance(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        (attr.path().is_ident("allow") || attr.path().is_ident("expect"))
            && attr
                .meta
                .to_token_stream()
                .to_string()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == "unsafe_code")
    })
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(x) => &x.attrs,
        syn::Item::Enum(x) => &x.attrs,
        syn::Item::ExternCrate(x) => &x.attrs,
        syn::Item::Fn(x) => &x.attrs,
        syn::Item::ForeignMod(x) => &x.attrs,
        syn::Item::Impl(x) => &x.attrs,
        syn::Item::Macro(x) => &x.attrs,
        syn::Item::Mod(x) => &x.attrs,
        syn::Item::Static(x) => &x.attrs,
        syn::Item::Struct(x) => &x.attrs,
        syn::Item::Trait(x) => &x.attrs,
        syn::Item::TraitAlias(x) => &x.attrs,
        syn::Item::Type(x) => &x.attrs,
        syn::Item::Union(x) => &x.attrs,
        syn::Item::Use(x) => &x.attrs,
        _ => &[],
    }
}

fn rust_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Context {
    test: bool,
    allowed: bool,
}

fn inherited(
    items: &[syn::Item],
    directory: &Path,
    context: Context,
    out: &mut BTreeMap<PathBuf, Context>,
) {
    for item in items {
        if let syn::Item::Mod(module) = item {
            let context = Context {
                test: context.test || testing(&module.attrs),
                allowed: context.allowed || allowance(&module.attrs),
            };
            let directory = directory.join(module.ident.to_string());
            if let Some((_, items)) = &module.content {
                inherited(items, &directory, context, out);
            } else {
                let explicit = module.attrs.iter().find_map(|attr| {
                    if let syn::Meta::NameValue(value) = &attr.meta
                        && attr.path().is_ident("path")
                        && let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(path),
                            ..
                        }) = &value.value
                    {
                        return Some(path.value());
                    }
                    None
                });
                let candidates = if let Some(path) = explicit {
                    vec![directory.parent().unwrap_or(directory.as_path()).join(path)]
                } else {
                    vec![directory.with_extension("rs"), directory.join("mod.rs")]
                };
                for path in candidates {
                    let previous = out.entry(path).or_default();
                    previous.test |= context.test;
                    previous.allowed |= context.allowed;
                }
            }
        }
    }
}

struct Scan<'a> {
    path: String,
    source: &'a str,
    context: Context,
    scope: Vec<String>,
    sites: Vec<Value>,
    functions: Vec<Value>,
    excluded: Vec<(usize, usize)>,
    ordinals: BTreeMap<(String, String), usize>,
    calls: Vec<Value>,
    declarations: usize,
    public_nodes: usize,
}

impl<'a> Scan<'a> {
    fn new(path: String, source: &'a str, context: Context) -> Self {
        Self {
            path,
            source,
            context,
            scope: Vec::new(),
            sites: Vec::new(),
            functions: Vec::new(),
            excluded: Vec::new(),
            ordinals: BTreeMap::new(),
            calls: Vec::new(),
            declarations: 0,
            public_nodes: 0,
        }
    }

    fn symbol(&self) -> String {
        self.scope.join("::")
    }

    fn emit(&mut self, kind: &str, span: Span, tokens: impl ToTokens, detail: Value) {
        if self.context.test && kind != "timing" {
            return;
        }
        let symbol = self.symbol();
        let ordinal = self
            .ordinals
            .entry((symbol.clone(), kind.into()))
            .or_default();
        *ordinal += 1;
        self.sites.push(json!({"path":self.path, "symbol":symbol, "kind":kind, "ordinal":ordinal,
            "fingerprint":fingerprint(&tokens.to_token_stream().to_string()), "line":span.start().line,
            "test":self.context.test, "detail":detail}));
    }

    fn function(
        &mut self,
        sig: &syn::Signature,
        block: Option<&syn::Block>,
        attrs: &[syn::Attribute],
        public: bool,
    ) {
        let context = self.context;
        self.context.test |= testing(attrs);
        self.context.allowed |= allowance(attrs);
        self.scope.push(sig.ident.to_string());
        if public && !self.context.test {
            self.public_nodes += 1;
            self.emit(
                "public_syntax",
                sig.span(),
                sig,
                json!({"candidate":"function visibility"}),
            );
        }
        self.emit(
            "declaration",
            sig.span(),
            quote::quote!(#sig #block),
            json!({"type":"function"}),
        );
        if let Some(block) = block {
            let end = block.span().end().line;
            let lines = end - sig.span().start().line + 1;
            self.functions.push(json!({"path":self.path,"symbol":self.symbol(),"line":sig.span().start().line,"end":end,"lines":lines,"test":self.context.test,"fingerprint":fingerprint(&quote::quote!(#sig #block).to_string())}));
            if lines > 80 {
                self.emit(
                    "function_size",
                    sig.span(),
                    quote::quote!(#sig #block),
                    json!({"lines":lines}),
                );
            }
        }
        if sig.asyncness.is_some() {
            self.emit(
                "async_chain",
                sig.span(),
                quote::quote!(#sig #block),
                json!({"candidate":"review callees, abort, blocking work and joins"}),
            );
        }
        if sig.unsafety.is_some() {
            self.emit(
                "unsafe_function",
                sig.span(),
                sig,
                json!({"allowed":self.context.allowed}),
            );
        }
        if sig.output.to_token_stream().to_string().contains("Result") {
            self.emit(
                "fallible_api",
                sig.span(),
                quote::quote!(#sig #(#attrs)*),
                json!({"public":public,"has_errors_heading":attrs.iter().any(|attr| attr.to_token_stream().to_string().contains("# Errors")),"candidate":"review errors, uncertainty and examples"}),
            );
        }
        let mut previous = None;
        for arg in &sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                if bool_type(&arg.ty) {
                    if let Some(first) = previous {
                        self.emit("bool_parameters", arg.span(), sig, json!({"parameters":[first,arg.pat.to_token_stream().to_string()],"callers":"candidate name matches, not resolved Rust calls"}));
                    }
                    previous = Some(arg.pat.to_token_stream().to_string());
                } else {
                    previous = None;
                }
            } else {
                previous = None;
            }
        }
        visit::visit_signature(self, sig);
        let first_site = self.sites.len();
        if let Some(block) = block {
            self.visit_block(block);
            let locks = self.sites[first_site..]
                .iter()
                .filter(|site| site["kind"] == "lock_edge")
                .count();
            if locks > 1 {
                self.emit("nested_locks", sig.span(), block, json!({"candidate":"multiple acquisitions; nesting and Drop order require review","acquisitions":locks}));
            }
        }
        self.scope.pop();
        self.context = context;
    }

    fn tokens(&mut self, tokens: TokenStream) {
        if let Ok(expressions) =
            syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                .parse2(tokens.clone())
        {
            for expr in expressions {
                self.visit_expr(&expr);
            }
            return;
        }
        for token in tokens.clone() {
            if let TokenTree::Ident(ident) = token
                && [
                    "sleep",
                    "sleep_until",
                    "timeout",
                    "timeout_at",
                    "elapsed",
                    "now",
                ]
                .contains(&ident.to_string().as_str())
            {
                self.emit(
                    "timing",
                    ident.span(),
                    &tokens,
                    json!({"candidate":"timing name in opaque macro syntax"}),
                );
            }
        }
        let mut chunk = TokenStream::new();
        for token in tokens {
            if matches!(&token, TokenTree::Punct(p) if [',',':',';'].contains(&p.as_char())) {
                self.part(std::mem::take(&mut chunk));
            } else {
                chunk.extend([token]);
            }
        }
        self.part(chunk);
    }

    fn part(&mut self, tokens: TokenStream) {
        if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expr);
        } else {
            for token in tokens {
                if let TokenTree::Group(group) = token {
                    self.tokens(group.stream());
                }
            }
        }
    }

    fn call(&mut self, name: String, span: Span, tokens: impl ToTokens) {
        self.calls.push(json!({"path":self.path,"symbol":self.symbol(),"name":name,"line":span.start().line,"test":self.context.test}));
        if [
            "sleep",
            "sleep_until",
            "timeout",
            "timeout_at",
            "elapsed",
            "now",
            "duration_since",
            "modified",
            "set_modified",
            "from_secs",
            "from_millis",
            "from_micros",
            "from_nanos",
        ]
        .iter()
        .any(|word| name.replace(' ', "").rsplit("::").next() == Some(word))
        {
            self.emit("timing", span, &tokens, json!({"call":name}));
        }
        if name.contains("std :: fs")
            || name.contains("fs ::")
            || name.contains("Command ::")
            || name.ends_with("spawn_blocking")
        {
            self.emit(
                "blocking_call",
                span,
                &tokens,
                json!({"call":name,"candidate":"async reachability requires review"}),
            );
        }
        if ["write", "append", "open", "create", "stdout", "stderr"]
            .iter()
            .any(|word| name.ends_with(word))
        {
            self.emit(
                "log_destination",
                span,
                tokens,
                json!({"call":name,"candidate":"may be storage rather than a log"}),
            );
        }
    }
}

fn bool_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(path) if path.qself.is_none() && (path.path.is_ident("bool") || ["std :: primitive :: bool", "core :: primitive :: bool"].contains(&path.path.to_token_stream().to_string().as_str())))
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let context = self.context;
        let attrs = item_attrs(item);
        self.context.test |= testing(attrs);
        self.context.allowed |= allowance(attrs);
        if self.context.test && !context.test {
            self.excluded
                .push((item.span().start().line, item.span().end().line));
        }
        if !self.context.test && !matches!(item, syn::Item::Use(_) | syn::Item::Impl(_)) {
            self.declarations += 1;
        }
        if allowance(attrs) {
            self.emit(
                "unsafe_allowance",
                item.span(),
                item,
                json!({"candidate":"review exception scope and reason"}),
            );
        }
        visit::visit_item(self, item);
        self.context = context;
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"module"}));
        visit::visit_item_mod(self, item);
        self.scope.pop();
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"struct"}));
        visit::visit_item_struct(self, item);
        self.scope.pop();
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"enum"}));
        visit::visit_item_enum(self, item);
        self.scope.pop();
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        self.scope.push(variant.ident.to_string());
        visit::visit_variant(self, variant);
        self.scope.pop();
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        self.scope.push(item.self_ty.to_token_stream().to_string());
        if item.unsafety.is_some() {
            self.emit(
                "unsafe_impl",
                item.span(),
                item,
                json!({"allowed":self.context.allowed}),
            );
        }
        if item.trait_.as_ref().is_some_and(|(_, path, _)| {
            path.segments
                .last()
                .is_some_and(|segment| segment.ident == "Drop")
        }) {
            self.emit(
                "drop_chain",
                item.span(),
                item,
                json!({"candidate":"review transitive blocking, locks and ownership"}),
            );
        }
        visit::visit_item_impl(self, item);
        self.scope.pop();
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => &[][..],
        };
        let context = self.context;
        self.context.test |= testing(attrs);
        if self.context.test && !context.test {
            self.excluded
                .push((item.span().start().line, item.span().end().line));
        }
        visit::visit_impl_item(self, item);
        self.context = context;
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attrs = match item {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => &[][..],
        };
        let context = self.context;
        self.context.test |= testing(attrs);
        if self.context.test && !context.test {
            self.excluded
                .push((item.span().start().line, item.span().end().line));
        }
        visit::visit_trait_item(self, item);
        self.context = context;
    }

    fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
        self.scope.push(item.abi.to_token_stream().to_string());
        self.emit(
            "declaration",
            item.span(),
            item,
            json!({"type":"extern block"}),
        );
        if item.unsafety.is_some() {
            self.emit(
                "unsafe_extern",
                item.span(),
                item,
                json!({"allowed":self.context.allowed}),
            );
        }
        visit::visit_item_foreign_mod(self, item);
        self.scope.pop();
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        let (attrs, name) = match item {
            syn::ForeignItem::Fn(item) => (&item.attrs[..], item.sig.ident.to_string()),
            syn::ForeignItem::Static(item) => (&item.attrs[..], item.ident.to_string()),
            syn::ForeignItem::Type(item) => (&item.attrs[..], item.ident.to_string()),
            syn::ForeignItem::Macro(item) => {
                (&item.attrs[..], item.mac.path.to_token_stream().to_string())
            }
            _ => (&[][..], String::new()),
        };
        let context = self.context;
        self.context.test |= testing(attrs);
        self.context.allowed |= allowance(attrs);
        self.scope.push(name);
        if self.context.test && !context.test {
            self.excluded
                .push((item.span().start().line, item.span().end().line));
        }
        if !self.context.test {
            self.declarations += 1;
        }
        self.emit(
            "declaration",
            item.span(),
            item,
            json!({"type":"foreign","allowed":self.context.allowed}),
        );
        if allowance(attrs) {
            self.emit(
                "unsafe_allowance",
                item.span(),
                item,
                json!({"candidate":"review exception scope and reason"}),
            );
        }
        visit::visit_foreign_item(self, item);
        self.scope.pop();
        self.context = context;
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"trait"}));
        visit::visit_item_trait(self, item);
        self.scope.pop();
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(
            &item.sig,
            Some(&item.block),
            &item.attrs,
            matches!(item.vis, syn::Visibility::Public(_)),
        );
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !self.context.test && !testing(&item.attrs) {
            self.declarations += 1;
        }
        self.function(
            &item.sig,
            Some(&item.block),
            &item.attrs,
            matches!(item.vis, syn::Visibility::Public(_)),
        );
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if !self.context.test && !testing(&item.attrs) {
            self.declarations += 1;
        }
        self.function(&item.sig, item.default.as_ref(), &item.attrs, false);
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        if bool_type(&field.ty) {
            self.emit(
                "bool_field",
                field.span(),
                field,
                json!({"field":field.ident.as_ref().map(ToString::to_string)}),
            );
        }
        self.emit(
            "boundary_field",
            field.span(),
            field,
            json!({"candidate":"classify wire, durable, internal or opaque extension role"}),
        );
        visit::visit_field(self, field);
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"const"}));
        visit::visit_item_const(self, item);
        self.scope.pop();
    }

    fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
        self.scope.push(item.ident.to_string());
        if !self.context.test {
            self.declarations += 1;
        }
        self.emit(
            "declaration",
            item.span(),
            item,
            json!({"type":"associated const"}),
        );
        visit::visit_impl_item_const(self, item);
        self.scope.pop();
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if !matches!(item.vis, syn::Visibility::Inherited) {
            self.emit(
                "reexport",
                item.span(),
                item,
                json!({"candidate":"facade declaration, not an import"}),
            );
        }
        visit::visit_item_use(self, item);
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        if literal
            .value()
            .split_whitespace()
            .any(|word| word == "sleep" || word == "timeout")
        {
            self.emit(
                "timing",
                literal.span(),
                literal,
                json!({"candidate":"embedded shell timing or text"}),
            );
        }
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"static"}));
        visit::visit_item_static(self, item);
        self.scope.pop();
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.scope.push(item.ident.to_string());
        self.emit("declaration", item.span(), item, json!({"type":"alias"}));
        visit::visit_item_type(self, item);
        self.scope.pop();
    }

    fn visit_trait_item_type(&mut self, item: &'ast syn::TraitItemType) {
        self.scope.push(item.ident.to_string());
        if !self.context.test {
            self.declarations += 1;
        }
        self.emit(
            "declaration",
            item.span(),
            item,
            json!({"type":"associated type"}),
        );
        visit::visit_trait_item_type(self, item);
        self.scope.pop();
    }

    fn visit_impl_item_type(&mut self, item: &'ast syn::ImplItemType) {
        self.scope.push(item.ident.to_string());
        if !self.context.test {
            self.declarations += 1;
        }
        self.emit(
            "declaration",
            item.span(),
            item,
            json!({"type":"associated type"}),
        );
        visit::visit_impl_item_type(self, item);
        self.scope.pop();
    }

    fn visit_expr_match(&mut self, expr: &'ast syn::ExprMatch) {
        self.emit(
            "match",
            expr.span(),
            expr,
            json!({"candidate":"review exhaustive or fallback policy"}),
        );
        visit::visit_expr_match(self, expr);
    }

    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if ty
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Value")
        {
            self.emit("json_role", ty.span(), ty, json!({"candidate":"decode, encode, presentation or extension; name may be unrelated"}));
        }
        visit::visit_type_path(self, ty);
    }

    fn visit_visibility(&mut self, visibility: &'ast syn::Visibility) {
        if matches!(visibility, syn::Visibility::Public(_)) && !self.context.test {
            self.public_nodes += 1;
            self.emit(
                "public_syntax",
                visibility.span(),
                visibility,
                json!({"candidate":"syntax node, not an independent API declaration"}),
            );
        }
    }

    fn visit_expr_method_call(&mut self, expr: &'ast syn::ExprMethodCall) {
        let name = expr.method.to_string();
        if [
            "unwrap",
            "expect",
            "ok",
            "unwrap_or",
            "unwrap_or_else",
            "unwrap_or_default",
            "map_or",
            "map_or_else",
            "lock",
            "try_lock",
            "read",
            "write",
        ]
        .contains(&name.as_str())
        {
            let kind = match name.as_str() {
                "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" | "map_or" | "map_or_else" => {
                    "fallback"
                }
                "lock" | "try_lock" | "read" | "write" => "lock_edge",
                other => other,
            };
            self.emit(
                kind,
                expr.span(),
                expr,
                json!({"method":name,"candidate":true}),
            );
        }
        self.call(name, expr.span(), expr);
        visit::visit_expr_method_call(self, expr);
    }

    fn visit_expr_call(&mut self, expr: &'ast syn::ExprCall) {
        self.call(expr.func.to_token_stream().to_string(), expr.span(), expr);
        visit::visit_expr_call(self, expr);
    }

    fn visit_expr_index(&mut self, expr: &'ast syn::ExprIndex) {
        let role = if matches!(
            expr.index.as_ref(),
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(_),
                ..
            })
        ) {
            "json_key_candidate"
        } else {
            "collection_or_json_candidate"
        };
        self.emit("index", expr.span(), expr, json!({"role":role}));
        visit::visit_expr_index(self, expr);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if matches!(local.pat, syn::Pat::Wild(_)) {
            self.emit(
                "discard",
                local.span(),
                local,
                json!({"candidate":"result type and outcome require review"}),
            );
        }
        visit::visit_local(self, local);
    }

    fn visit_expr_unsafe(&mut self, expr: &'ast syn::ExprUnsafe) {
        let line = expr.span().start().line;
        let safety = self
            .source
            .lines()
            .skip(line.saturating_sub(9))
            .take(9)
            .any(|line| line.contains("SAFETY:"));
        self.emit(
            "unsafe_block",
            expr.span(),
            expr,
            json!({"allowed":self.context.allowed,"nearby_safety":safety}),
        );
        visit::visit_expr_unsafe(self, expr);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac.path.to_token_stream().to_string();
        if name.contains("assert")
            && mac
                .tokens
                .to_string()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| {
                    [
                        "elapsed", "after", "waited", "duration", "age", "deadline", "PROMPTLY",
                    ]
                    .contains(&word)
                })
        {
            self.emit(
                "timing",
                mac.span(),
                mac,
                json!({"candidate":"assertion with clock or elapsed-value vocabulary"}),
            );
        }
        let kind = match name.as_str() {
            "unreachable" | "panic" | "todo" | "unimplemented" => name.as_str(),
            "eprintln" | "println" | "eprint" | "print" => "log_destination",
            _ => "macro",
        };
        self.emit(
            kind,
            mac.span(),
            mac,
            json!({"name":name,"candidate":"macro expansion is not resolved"}),
        );
        self.tokens(mac.tokens.clone());
    }
}

fn module_contexts(
    sources: &[(PathBuf, String, syn::File)],
    root: &Path,
) -> BTreeMap<PathBuf, Context> {
    let mut contexts = BTreeMap::<PathBuf, Context>::new();
    loop {
        let previous = contexts.clone();
        for (path, _, parsed) in sources {
            let context = contexts.get(path).copied().unwrap_or_default();
            let context = Context {
                test: context.test || testing(&parsed.attrs),
                allowed: context.allowed || allowance(&parsed.attrs),
            };
            let directory = if ["mod", "lib", "main"]
                .iter()
                .any(|stem| path.file_stem().is_some_and(|s| s == *stem))
            {
                path.parent().unwrap_or(root).to_owned()
            } else {
                path.with_extension("")
            };
            inherited(&parsed.items, &directory, context, &mut contexts);
        }
        if contexts == previous {
            break;
        }
    }
    contexts
}

fn inventory(root: &Path) -> Result<Value> {
    let mut paths = Vec::new();
    rust_files(&root.join("src"), &mut paths)?;
    rust_files(&root.join("xtask/src"), &mut paths)?;
    paths.sort();
    let sources = paths
        .iter()
        .map(|path| {
            let source = read(path)?;
            let parsed =
                syn::parse_file(&source).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((path.clone(), source, parsed))
        })
        .collect::<Result<Vec<_>>>()?;
    let contexts = module_contexts(&sources, root);
    let mut files = Vec::new();
    let mut sites = Vec::new();
    let mut functions = Vec::new();
    let mut calls = Vec::new();
    for (path, source, parsed) in sources {
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let mut context = contexts.get(&path).copied().unwrap_or_default();
        context.test |= relative
            .split('/')
            .any(|part| part == "tests" || part == "test_support")
            || relative.ends_with("tests.rs")
            || testing(&parsed.attrs);
        context.allowed |= allowance(&parsed.attrs);
        let mut scan = Scan::new(relative.clone(), &source, context);
        if allowance(&parsed.attrs) {
            scan.emit(
                "unsafe_allowance",
                parsed.span(),
                &parsed,
                json!({"candidate":"file unsafe exception"}),
            );
        }
        scan.visit_file(&parsed);
        let lines = source.lines().collect::<Vec<_>>();
        let retained = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                !scan
                    .excluded
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(&(index + 1)))
            })
            .map(|(_, line)| *line)
            .collect::<Vec<_>>();
        let production = if context.test { 0 } else { retained.len() };
        if production > 800 {
            scan.emit(
                "file_size",
                Span::call_site(),
                TokenStream::new(),
                json!({"lines":production}),
            );
            if let Some(site) = scan.sites.last_mut() {
                site["fingerprint"] = fingerprint(&retained.join("\n")).into();
            }
        }
        files.push(json!({"path":relative,"physical":lines.len(),"production":production,"declarations":scan.declarations,"public_syntax_nodes":scan.public_nodes}));
        sites.extend(scan.sites);
        functions.extend(scan.functions);
        calls.extend(scan.calls);
    }
    for site in &mut sites {
        if site["kind"] == "bool_parameters" {
            let name = site["symbol"]
                .as_str()
                .unwrap_or("")
                .rsplit("::")
                .next()
                .unwrap_or("");
            site["detail"]["callers"] = calls
                .iter()
                .filter(|call| {
                    call["name"].as_str().is_some_and(|called| {
                        called.replace(' ', "").rsplit("::").next() == Some(name)
                    })
                })
                .cloned()
                .collect::<Vec<_>>()
                .into();
        }
    }
    let external_timing = external_timing(root)?;
    let mut ordinals = BTreeMap::<String, usize>::new();
    for timing in &external_timing {
        let path = timing["path"]
            .as_str()
            .ok_or("missing external timing path")?;
        let ordinal = ordinals.entry(path.into()).or_default();
        *ordinal += 1;
        sites.push(json!({"path":path,"symbol":"external test timing","kind":"timing","ordinal":ordinal,
            "fingerprint":timing["fingerprint"],"line":timing["line"],"test":true,"detail":{"candidate":timing["candidate"]}}));
    }
    Ok(
        json!({"files":files,"functions":functions,"sites":sites,"external_timing":external_timing,
        "limits":["Inactive cfg branches are parsed; macros are inspected but not expanded.","Callers are name candidates; imports, aliasing, dispatch, inferred types and transitive call graphs need review.","Index, boundary, fallback, lock and log roles are candidates, not semantic proofs.","External timing is a textual candidate inventory, not an ordering proof."]}),
    )
}

fn external_timing(root: &Path) -> Result<Vec<Value>> {
    fn walk(root: &Path, directory: &Path, out: &mut Vec<Value>) -> Result<()> {
        for entry in fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                walk(root, &path, out)?;
            } else if path.extension().is_some_and(|ext| {
                ["rs", "sh", "mjs", "js"]
                    .iter()
                    .any(|candidate| ext == *candidate)
            }) {
                for (index, line) in read(&path)?.lines().enumerate() {
                    if [
                        "sleep",
                        "timeout",
                        "elapsed",
                        "Instant::now",
                        "SystemTime::now",
                    ]
                    .iter()
                    .any(|word| line.contains(word))
                    {
                        out.push(json!({"path":path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy(),"line":index+1,"fingerprint":fingerprint(line),"candidate":line.trim()}));
                    }
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for directory in ["tests", "scripts"] {
        walk(root, &root.join(directory), &mut out)?;
    }
    out.sort_by_key(|row| {
        (
            row["path"].as_str().unwrap_or("").to_owned(),
            row["line"].as_u64(),
        )
    });
    Ok(out)
}

fn key(site: &Value) -> Result<String> {
    let string = |field| {
        site[field]
            .as_str()
            .ok_or_else(|| format!("missing site {field}"))
    };
    let ordinal = site["ordinal"].as_u64().ok_or("missing site ordinal")?;
    Ok(format!(
        "{}|{}|{}|{ordinal}",
        string("path")?,
        string("symbol")?,
        string("kind")?
    ))
}

fn check(report: &Value, ledger: &Value) -> Result<[usize; 3]> {
    let rows = |value: &Value, name: &str| {
        value[name]
            .as_array()
            .cloned()
            .ok_or_else(|| format!("missing {name} array"))
    };
    let sites = rows(report, "sites")?;
    let entries = rows(ledger, "dispositions")?;
    let mut remaining = BTreeMap::new();
    for site in &sites {
        if remaining.insert(key(site)?, site).is_some() {
            return Err("duplicate inventory site".into());
        }
    }
    let mut counts = [0; 3];
    for entry in entries {
        let id = key(&entry)?;
        let site = remaining
            .remove(&id)
            .ok_or_else(|| format!("vanished or duplicate disposition: {id}"))?;
        if entry["fingerprint"] != site["fingerprint"] {
            return Err(format!("stale disposition: {id}"));
        }
        for field in ["owner", "reason"] {
            let text = entry[field]
                .as_str()
                .ok_or_else(|| format!("missing {field}: {id}"))?;
            if text.trim().is_empty()
                || text.contains('*')
                || ["all", "any", "baseline", "allowed", "reviewed"].contains(&text.trim())
            {
                return Err(format!("blanket or empty {field}: {id}"));
            }
        }
        match entry["status"].as_str() {
            Some("open_debt") => counts[0] += 1,
            Some("retained_proof") => {
                if entry["proof"]
                    .as_str()
                    .is_none_or(|proof| proof.trim().is_empty() || proof.contains('*'))
                {
                    return Err(format!("missing local proof: {id}"));
                }
                counts[1] += 1;
            }
            Some("required_change") => counts[2] += 1,
            _ => return Err(format!("unknown disposition status: {id}")),
        }
    }
    if let Some((id, _)) = remaining.first_key_value() {
        return Err(format!("unclassified site: {id}"));
    }
    let recipes = rows(ledger, "feature_recipes")?;
    let names = rows(ledger, "recipe_names")?;
    let mut actual = recipes
        .iter()
        .map(|recipe| recipe["name"].clone())
        .collect::<Vec<_>>();
    actual.sort_by_key(Value::to_string);
    let mut expected = names;
    expected.sort_by_key(Value::to_string);
    if actual != expected || actual.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("missing or duplicate feature recipe".into());
    }
    for recipe in recipes {
        let named = rows(&recipe, "sites")?;
        if named.is_empty() || recipe["count"].as_u64() != Some(named.len() as u64) {
            return Err("feature recipe has a missing site".into());
        }
        let mut unique = BTreeMap::new();
        for anchor in named {
            let id = key(&anchor)?;
            if unique.insert(id, ()).is_some() {
                return Err("duplicate feature site".into());
            }
            if !sites.iter().any(|site| {
                key(site).ok() == key(&anchor).ok() && site["fingerprint"] == anchor["fingerprint"]
            }) {
                return Err(format!("missing or stale feature site: {}", recipe["name"]));
            }
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests;
