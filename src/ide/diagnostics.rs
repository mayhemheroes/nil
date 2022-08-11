use crate::{DefDatabase, Diagnostic, DiagnosticKind, FileId};
use rowan::ast::AstNode;
use syntax::ast;

pub(crate) fn diagnostics(db: &dyn DefDatabase, file: FileId) -> Vec<Diagnostic> {
    let parse = db.parse(file);
    let module = db.module(file);

    let mut diags = Vec::new();

    // If we are not in single-file mode, but this file is not in the reference closure,
    // complain about it.
    let source_closure = db.source_root_closure(db.file_source_root(file));
    if !source_closure.is_empty() && !source_closure.contains(&file) {
        diags.push(Diagnostic::new(
            Default::default(),
            DiagnosticKind::FileNotReferenced,
        ));
    }

    // Parsing.
    diags.extend(parse.errors().iter().map(|&err| Diagnostic::from(err)));

    // Lowering.
    diags.extend(module.diagnostics().iter().cloned());

    // Liveness check.
    let liveness = db.liveness_check(file);
    let source_map = db.source_map(file);
    diags.extend(liveness.unused_name_defs().iter().map(|&def| {
        Diagnostic::new(
            source_map.name_def_node(def).unwrap().text_range(),
            DiagnosticKind::UnusedBinding,
        )
    }));
    diags.extend(liveness.unused_withs().iter().filter_map(|&expr| {
        let ptr = source_map.expr_node(expr)?;
        let node = ast::With::cast(ptr.to_node(&parse.syntax_node()))?;
        let with_token_range = node.with_token()?.text_range();
        let with_header_range = node.semicolon_token().map_or_else(
            || node.syntax().text_range(),
            |tok| tok.text_range().cover(with_token_range),
        );
        Some(Diagnostic::new(
            with_header_range,
            DiagnosticKind::UnusedWith,
        ))
    }));
    diags.extend(liveness.unused_recs().iter().filter_map(|&expr| {
        let ptr = source_map.expr_node(expr)?;
        let node = ast::AttrSet::cast(ptr.to_node(&parse.syntax_node()))?;
        let rec_range = node
            .rec_token()
            .map_or_else(|| node.syntax().text_range(), |tok| tok.text_range());
        Some(Diagnostic::new(rec_range, DiagnosticKind::UnusedRec))
    }));

    diags
}

#[cfg(test)]
mod tests {
    use crate::tests::TestDB;
    use expect_test::{expect, Expect};

    #[track_caller]
    fn check(fixture: &str, expect: Expect) {
        check_file("/default.nix", fixture, expect);
    }

    #[track_caller]
    fn check_file(path: &str, fixture: &str, expect: Expect) {
        let (db, f) = TestDB::from_fixture(fixture).unwrap();
        let diags = super::diagnostics(&db, f[path]);
        assert!(!diags.is_empty());
        let got = diags
            .iter()
            .map(|d| d.debug_to_string() + "\n")
            .collect::<String>();
        expect.assert_eq(&got);
    }

    #[test]
    fn syntax_error() {
        check(
            "1 == 2 == 3",
            expect![[r#"
                7..9: Invalid usage of no-associative operators
            "#]],
        );
    }

    #[test]
    fn lower_error() {
        check(
            "{ a = 1; a = 2; }",
            expect![[r#"
                2..3: Duplicated name definition
                9..10: Duplicated name definition
            "#]],
        );
    }

    #[test]
    fn liveness() {
        check(
            "let a = a; b = 1; in with 1; b + rec { }",
            expect![[r#"
                4..5: Unused binding
                21..28: Unused `with`
                33..36: Unused `rec`
            "#]],
        );
    }

    #[test]
    fn file_references() {
        check_file(
            "/bar.nix",
            "
#- /default.nix
./foo.nix

#- /foo.nix
42

#- /bar.nix
24
            ",
            expect![[r#"
                0..0: File not referenced from entry files via any paths
            "#]],
        );
    }
}
