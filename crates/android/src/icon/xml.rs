/// Values are kept as `aapt2 dump xmltree` prints them: a reference as
/// `@0x7f080028`, a colour as `#ff000000`, and an enum as its number.
#[derive(Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    /// `aapt2 dump xmltree` prints an element as `E: path (line=7)`, with its
    /// attributes indented below it:
    /// `A: http://schemas.android.com/apk/res/android:fillColor(0x01010404)=#ff000000`.
    pub fn parse(tree: &str) -> Option<Element> {
        let mut open: Vec<(usize, Element)> = Vec::new();

        for line in tree.lines() {
            let depth = line.len() - line.trim_start().len();
            let line = line.trim();

            if let Some(name) = line.strip_prefix("E: ") {
                while open.last().is_some_and(|(at, _)| *at >= depth) {
                    close(&mut open);
                }

                open.push((
                    depth,
                    Element {
                        name: name.split(' ').next().unwrap_or_default().to_owned(),
                        ..Element::default()
                    },
                ));
            } else if let Some(attribute) = line.strip_prefix("A: ")
                && let Some((_, element)) = open.last_mut()
                && let Some(attribute) = self::attribute(attribute)
            {
                element.attributes.push(attribute);
            }
        }

        loop {
            if let Some(root) = close(&mut open) {
                return Some(root);
            }

            if open.is_empty() {
                return None;
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let (_, value) = self.attributes.iter().find(|(known, _)| known == name)?;

        Some(value)
    }

    pub fn number(&self, name: &str) -> Option<f32> {
        self.get(name)?.parse().ok()
    }
}

fn close(open: &mut Vec<(usize, Element)>) -> Option<Element> {
    let (_, done) = open.pop()?;

    match open.last_mut() {
        Some((_, parent)) => {
            parent.children.push(done);

            None
        }
        None => Some(done),
    }
}

fn attribute(line: &str) -> Option<(String, String)> {
    let (name, value) = line.split_once('=')?;
    // The name comes with its namespace and resource id, which are dropped.
    let name = name.split('(').next().unwrap_or_default();
    let name = name.rsplit(':').next().unwrap_or_default();
    // Text is quoted and followed by its raw form, which is dropped.
    let value = match value.strip_prefix('"') {
        Some(text) => text.split("\" (Raw: ").next().unwrap_or_default(),
        None => value,
    };

    Some((name.to_owned(), value.to_owned()))
}
