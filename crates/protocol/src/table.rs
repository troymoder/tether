/// Tabs, line breaks and backslashes inside a cell are escaped as `\t`, `\n`
/// and `\\`.
pub fn row<S: AsRef<str>>(cells: impl IntoIterator<Item = S>) -> String {
    let cells: Vec<String> = cells
        .into_iter()
        .map(|cell| {
            cell.as_ref()
                .replace('\\', "\\\\")
                .replace('\t', "\\t")
                .replace('\n', "\\n")
        })
        .collect();

    cells.join("\t")
}

pub fn cells(row: &str) -> Vec<String> {
    row.split('\t').map(unescape).collect()
}

fn unescape(cell: &str) -> String {
    let mut plain = String::with_capacity(cell.len());
    let mut letters = cell.chars();

    while let Some(letter) = letters.next() {
        plain.push(match letter {
            '\\' => match letters.next() {
                Some('t') => '\t',
                Some('n') => '\n',
                Some(other) => other,
                None => break,
            },
            other => other,
        });
    }

    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_keeps_what_is_in_its_cells() {
        let told = ["plain", "a\tb", "two\nlines", "back\\slash \\n", ""];
        let written = row(told);

        assert_eq!(written.lines().count(), 1);
        assert_eq!(written.matches('\t').count(), 4);
        assert_eq!(cells(&written), told);
    }
}
