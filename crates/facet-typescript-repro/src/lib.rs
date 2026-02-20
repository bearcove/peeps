#[cfg(test)]
mod tests {
    use facet::Facet;
    use facet_typescript::TypeScriptGenerator;

    #[derive(Facet)]
    struct EnvelopeNonTransparent {
        backtrace: BacktraceIdNonTransparent,
    }

    #[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    struct BacktraceIdNonTransparent(u64);

    #[derive(Facet)]
    struct EnvelopeTransparent {
        backtrace: BacktraceIdTransparent,
    }

    #[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[facet(transparent)]
    struct BacktraceIdTransparent(u64);

    #[test]
    fn transparent_newtype_is_scalar_alias() {
        let mut gen = TypeScriptGenerator::new();
        gen.add_type::<EnvelopeTransparent>();
        let out = gen.finish();
        assert!(
            out.contains("export type BacktraceIdTransparent = number;"),
            "expected transparent newtype to generate scalar alias, got:\n{out}"
        );
    }

    #[test]
    fn non_transparent_newtype_is_not_scalar_alias() {
        let mut gen = TypeScriptGenerator::new();
        gen.add_type::<EnvelopeNonTransparent>();
        let out = gen.finish();
        assert!(
            !out.contains("export type BacktraceIdNonTransparent = number;"),
            "bug: non-transparent tuple newtype generated scalar alias:\n{out}"
        );
    }
}
