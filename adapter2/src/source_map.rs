use error::Error;
use globset::Glob;
use regex::Regex;
use std::path::{Component, Path, PathBuf};

pub struct SourceMap {
    pairs: Vec<(Regex, Option<String>)>,
}

impl SourceMap {
    pub fn new<'a>(source_map: impl IntoIterator<Item = &'a (String, Option<String>)>) -> Result<SourceMap, Error> {
        let mut pairs = vec![];
        for (remote, local) in source_map.into_iter() {
            let glob = match Glob::new(&remote) {
                Ok(glob) => glob,
                Err(err) => return Err(Error::UserError(format!("Invalid glob pattern: {}", remote))),
            };
            let regex = Regex::new(&format!("({}).*", &glob.regex()[5..])).unwrap(); // TODO: use ?
            pairs.push((regex, local.clone()));
        }
        Ok(SourceMap { pairs })
    }

    pub fn to_local(&self, path: &str) -> Option<String> {
        let normalized = normalize_path(path);
        for (remote_prefix, local_prefix) in self.pairs.iter() {
            if let Some(captures) = remote_prefix.captures(&normalized) {
                return match local_prefix {
                    Some(prefix) => {
                        let match_len = captures.get(1).unwrap().start();
                        let result = normalize_path(&format!("{}{}", prefix, &normalized[match_len..]));
                        Some(result)
                    }
                    None => None,
                };
            }
        }
        Some(normalized)
    }
}

pub fn normalize_path(path: &str) -> String {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => (),
            Component::Normal(comp) => normalized.push(comp),
            Component::CurDir => (),
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }
    normalized.to_str().unwrap().into()
}

#[test]
fn test_source_map() {
    let source_map = [
        ("/foo/bar/*".to_owned(), Some("/hren".to_owned()))
    ];
    let map = SourceMap::new(&source_map).unwrap();
    assert_eq!(map.to_local("/foo/bar/baz.cpp"), Some("/hren/baz.cpp".to_owned()));
}
