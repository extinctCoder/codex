use serde::Deserialize;

/// One item in a view's `select:` list. The variant is chosen by which keys are
/// present (`column` vs `sql`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SelectItem {
    Column(SelectColumn),
    Expression(SelectExpression),
}

/// A plain column pulled from a `from:`/`join:` alias.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectColumn {
    pub column: String,
    /// The alias the column comes from; resolved in a later phase.
    #[serde(default)]
    pub from: Option<String>,
    /// Output name; defaults to the column name when absent.
    #[serde(default)]
    pub r#as: Option<String>,
}

/// A raw SQL expression with a declared output name (escape hatch).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectExpression {
    pub sql: String,
    pub r#as: String,
}
