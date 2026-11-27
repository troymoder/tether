mod sqlite;

use adb::Device;
use adb::libadb::Session;
use anyhow::{Result, ensure};

use crate::files;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Database,
    Preferences,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    pub kind: Kind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Preference {
    pub name: String,
    /// The element name Android writes it as, such as `string`, `int`,
    /// `boolean` or `set`.
    pub kind: String,
    pub value: String,
}

/// True if `name` is a plain file name that cannot leave the app's folder.
fn valid_file(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\0', '\n']) && name != ".."
}

pub async fn stores<S>(device: &Device<S>, rooted: bool, id: &str) -> Result<Vec<Store>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    ensure!(files::is_package(id), "invalid package name");

    let mut stores = Vec::new();

    for (kind, folder) in [
        (Kind::Database, "databases"),
        (Kind::Preferences, "shared_prefs"),
    ] {
        let listed = match files::list(device, rooted, &format!("/data/data/{id}/{folder}")).await {
            Ok(listed) => listed,
            // An app with no stores of this kind has no folder for them.
            Err(error) if error.to_string().contains("No such file") => continue,
            Err(error) => return Err(error),
        };

        for name in listed
            .iter()
            .filter(|entry| !entry.folder)
            .map(|entry| entry.name.as_str())
        {
            // SQLite's journal and write-ahead files sit beside a database
            // and are not databases themselves.
            let aside = ["-wal", "-shm", "-journal"]
                .iter()
                .any(|end| name.ends_with(end));
            let kept = match kind {
                Kind::Database => !aside,
                Kind::Preferences => name.ends_with(".xml"),
            };

            if kept && valid_file(name) {
                stores.push(Store {
                    kind,
                    name: name.to_owned(),
                });
            }
        }
    }

    Ok(stores)
}

// shortcut: read-only, because a write would have to reach the device while
// the app holds the database open. Stop the app first if writes are needed.
pub async fn query<S>(
    device: &Device<S>,
    rooted: bool,
    id: &str,
    database: &str,
    sql: &str,
) -> Result<Vec<Vec<String>>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    ensure!(files::is_package(id), "invalid package name");
    ensure!(valid_file(database), "invalid database name");

    let path = format!("/data/data/{id}/databases/{database}");
    let file = files::read(device, rooted, &path).await?;
    // The write-ahead log holds changes not yet checkpointed into the file.
    let wal = files::read(device, rooted, &format!("{path}-wal"))
        .await
        .unwrap_or_default();

    sqlite::ask(sqlite::with_wal(file, &wal), sql)
}

pub async fn preferences<S>(
    device: &Device<S>,
    rooted: bool,
    id: &str,
    file: &str,
) -> Result<Vec<Preference>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    ensure!(files::is_package(id), "invalid package name");
    ensure!(valid_file(file), "invalid preferences file name");

    let path = format!("/data/data/{id}/shared_prefs/{file}");
    let xml = files::read(device, rooted, &path).await?;

    read(&String::from_utf8_lossy(&xml))
}

/// Android writes each entry as an element named after its type, with the
/// value in a `value` attribute, or as text for a string. A set has a string
/// element for each of its values.
fn read(xml: &str) -> Result<Vec<Preference>> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| anyhow::anyhow!("preferences failed: {error}"))?;
    let text = |node: roxmltree::Node| node.text().unwrap_or_default().to_owned();

    Ok(document
        .root_element()
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|element| {
            let kind = element.tag_name().name();
            let value = match (kind, element.attribute("value")) {
                ("set", _) => element
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .map(text)
                    .collect::<Vec<_>>()
                    .join(", "),
                (_, Some(value)) => value.to_owned(),
                (_, None) => text(element),
            };

            Preference {
                name: element.attribute("name").unwrap_or_default().to_owned(),
                kind: kind.to_owned(),
                value,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_preferences() {
        let xml = "<?xml version='1.0' encoding='utf-8' standalone='yes' ?>\n<map>\n    \
            <string name=\"greeting\">a &lt;b&gt; &amp; c&#10;d</string>\n    \
            <int name=\"count\" value=\"3\" />\n    \
            <boolean name=\"on\" value=\"true\" />\n    \
            <set name=\"tags\">\n        <string>one</string>\n        <string>two</string>\n    </set>\n    \
            <set name=\"none\" />\n    \
            <string name=\"empty\"></string>\n</map>\n";

        assert_eq!(
            read(xml)
                .unwrap()
                .into_iter()
                .map(|found| [found.name, found.kind, found.value].join("|"))
                .collect::<Vec<_>>(),
            [
                "greeting|string|a <b> & c\nd",
                "count|int|3",
                "on|boolean|true",
                "tags|set|one, two",
                "none|set|",
                "empty|string|",
            ]
        );

        assert!(read("<map><string").is_err());
    }
}
