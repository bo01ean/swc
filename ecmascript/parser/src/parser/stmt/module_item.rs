use super::*;

#[parser]
impl<'a, I: Input> Parser<'a, I> {
    fn parse_import(&mut self) -> PResult<'a, ModuleDecl> {
        let start = cur_pos!();
        assert_and_bump!("import");

        // Handle import 'mod.js'
        match *cur!()? {
            Str { .. } => match bump!() {
                Str { value, .. } => {
                    expect!(';');
                    return Ok(ModuleDecl {
                        span: span!(start),
                        node: ModuleDeclKind::Import {
                            src: value,
                            specifiers: vec![],
                        },
                    });
                }
                _ => unreachable!(),
            },
            _ => {}
        }

        let mut specifiers = vec![];

        if is!(BindingIdent) {
            let local = self.parse_imported_default_binding()?;
            //TODO: Better error reporting
            if !is!("from") {
                expect!(',');
            }
            specifiers.push(ImportSpecifier {
                span: local.span,
                local,
                node: ImportSpecifierKind::Default,
            });
        }

        {
            let import_spec_start = cur_pos!();
            if eat!('*') {
                expect!("as");
                let local = self.parse_imported_binding()?;
                specifiers.push(ImportSpecifier {
                    span: span!(import_spec_start),
                    local,
                    node: ImportSpecifierKind::Namespace,
                });
            } else if eat!('{') {
                let mut first = true;
                while !eof!() && !is!('}') {
                    if first {
                        first = false;
                    } else {
                        if eat!(',') {
                            if is!('}') {
                                break;
                            }
                        }
                    }

                    specifiers.push(self.parse_import_specifier()?);
                }
                expect!('}');
            }
        }

        let src = self.parse_from_clause_and_semi()?;

        Ok(ModuleDecl {
            span: span!(start),
            node: ModuleDeclKind::Import { specifiers, src },
        })
    }

    /// Parse `foo`, `foo2 as bar` in `import { foo, foo2 as bar }`
    fn parse_import_specifier(&mut self) -> PResult<'a, ImportSpecifier> {
        let start = cur_pos!();
        match *cur!()? {
            Word(..) => {
                let orig_name = self.parse_ident_name()?;

                if eat!("as") {
                    let local = self.parse_binding_ident()?;
                    return Ok(ImportSpecifier {
                        span: Span::new(start, local.span.hi(), Default::default()),
                        local,
                        node: ImportSpecifierKind::Specific {
                            imported: Some(orig_name),
                        },
                    });
                }

                // Handle difference between
                //
                // 'ImportedBinding'
                // 'IdentifierName' as 'ImportedBinding'
                if self.ctx().is_reserved_word(&orig_name.sym) {
                    syntax_error!(orig_name.span, SyntaxError::Unexpected)
                }

                let local = orig_name;
                return Ok(ImportSpecifier {
                    span: span!(start),
                    local,
                    node: ImportSpecifierKind::Specific { imported: None },
                });
            }
            _ => unexpected!(),
        }
    }

    fn parse_imported_default_binding(&mut self) -> PResult<'a, Ident> {
        self.parse_imported_binding()
    }

    fn parse_imported_binding(&mut self) -> PResult<'a, Ident> {
        let ctx = Context {
            in_async: false,
            in_generator: false,
            ..self.ctx()
        };
        self.with_ctx(ctx).parse_binding_ident()
    }

    fn parse_export(&mut self) -> PResult<'a, ModuleDecl> {
        let start = cur_pos!();
        assert_and_bump!("export");

        if eat!('*') {
            let src = self.parse_from_clause_and_semi()?;
            return Ok(ModuleDecl {
                span: span!(start),
                node: ModuleDeclKind::ExportAll { src },
            });
        }

        if eat!("default") {
            let decl = if is!("class") {
                self.parse_default_class()?
            } else if is!("async") && peeked_is!("function")
                && !self.input.has_linebreak_between_cur_and_peeked()
            {
                self.parse_default_async_fn()?
            } else if is!("function") {
                self.parse_default_fn()?
            } else {
                let expr = self.include_in_expr(true).parse_assignment_expr()?;
                expect!(';');
                return Ok(ModuleDecl {
                    span: span!(start),
                    node: ModuleDeclKind::ExportDefaultExpr(expr),
                });
            };

            return Ok(ModuleDecl {
                span: span!(start),
                node: ModuleDeclKind::ExportDefaultDecl(decl),
            });
        }

        let decl = if is!("class") {
            self.parse_class_decl()?
        } else if is!("async") && peeked_is!("function")
            && !self.input.has_linebreak_between_cur_and_peeked()
        {
            self.parse_async_fn_decl()?
        } else if is!("function") {
            self.parse_fn_decl()?
        } else if is!("var") || is!("const")
            || (is!("let")
                && peek!()
                    .map(|t| {
                        // module code is always in strict mode.
                        t.follows_keyword_let(true)
                    })
                    .unwrap_or(false))
        {
            self.parse_var_stmt(false).map(Decl::Var)?
        } else {
            // export {};
            // export {} from '';

            expect!('{');
            let mut specifiers = vec![];
            let mut first = true;
            while is_one_of!(',', IdentName) {
                if first {
                    first = false;
                } else {
                    if eat!(',') {
                        if is!('}') {
                            break;
                        }
                    }
                }

                specifiers.push(self.parse_export_specifier()?);
            }
            expect!('}');

            let src = if is!("from") {
                Some(self.parse_from_clause_and_semi()?)
            } else {
                None
            };
            return Ok(ModuleDecl {
                span: span!(start),
                node: ModuleDeclKind::ExportNamed { specifiers, src },
            });
        };

        return Ok(ModuleDecl {
            span: span!(start),
            node: ModuleDeclKind::ExportDecl(decl),
        });
    }

    fn parse_export_specifier(&mut self) -> PResult<'a, ExportSpecifier> {
        let orig = self.parse_ident_name()?;

        let exported = if eat!("as") {
            Some(self.parse_ident_name()?)
        } else {
            None
        };
        Ok(ExportSpecifier { orig, exported })
    }

    fn parse_from_clause_and_semi(&mut self) -> PResult<'a, String> {
        expect!("from");
        match *cur!()? {
            Str { .. } => match bump!() {
                Str { value, .. } => {
                    expect!(';');
                    Ok(value)
                }
                _ => unreachable!(),
            },
            _ => unexpected!(),
        }
    }
}

impl IsDirective for ModuleItem {
    fn as_ref(&self) -> Option<&StmtKind> {
        match *self {
            ModuleItem::Stmt(ref s) => Some(&s.node),
            _ => None,
        }
    }
}

#[parser]
impl<'a, I: Input> StmtLikeParser<'a, ModuleItem> for Parser<'a, I> {
    fn accept_import_export() -> bool {
        true
    }

    fn handle_import_export(&mut self, top_level: bool) -> PResult<'a, ModuleItem> {
        if !top_level {
            syntax_error!(SyntaxError::NonTopLevelImportExport);
        }

        let start = cur_pos!();
        let decl = if is!("import") {
            self.parse_import()?
        } else if is!("export") {
            self.parse_export()?
        } else {
            unreachable!(
                "handle_import_export should not be called if current token isn't import nor \
                 export"
            )
        };

        Ok(ModuleItem::ModuleDecl(decl))
    }
}