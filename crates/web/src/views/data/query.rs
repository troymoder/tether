pub(super) const TABLES: &str =
    "SELECT name FROM sqlite_master WHERE type IN ('table', 'view') ORDER BY name";

/// Quotes a table name for SQL, since a name cannot be bound as a value is.
pub(super) fn sql_name(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(super) fn first_rows(table: &str) -> String {
    format!("SELECT * FROM {} LIMIT 100", sql_name(table))
}

/// One query that counts the rows of every table in a single round trip.
pub(super) fn counting(tables: &[String]) -> String {
    tables
        .iter()
        .map(|name| {
            format!(
                "SELECT '{}', count(*) FROM {}",
                name.replace('\'', "''"),
                sql_name(name),
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_a_table_name() {
        assert_eq!(sql_name("my \"table\""), "\"my \"\"table\"\"\"");
    }

    #[test]
    fn counts_every_table_in_one_query() {
        let tables = ["a".to_owned(), "it's".to_owned()];

        assert_eq!(
            counting(&tables),
            "SELECT 'a', count(*) FROM \"a\" UNION ALL SELECT 'it''s', count(*) FROM \"it's\""
        );
    }
}
