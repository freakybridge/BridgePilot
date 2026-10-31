//! EOL equivalence is Git-proven; transaction inputs always retain their bytes.
use super::*;
use std::cell::RefCell;

const ATTRS: [&str; 6] = [
    "text",
    "eol",
    "crlf",
    "filter",
    "working-tree-encoding",
    "ident",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    attributes: BTreeMap<String, Vec<String>>,
    config: String,
}

fn query(
    root: &Path,
    args: &[&str],
    stdin: Vec<u8>,
    runner: &dyn ProcessRunner,
    missing: bool,
) -> Result<Vec<u8>, String> {
    let mut request = ProcessRequest::new("git", root);
    request.args = args.iter().map(Into::into).collect();
    request.stdin = stdin;
    request.timeout = Duration::from_secs(45);
    let output = InspectionRunner(runner)
        .run(&request)
        .map_err(|e| e.to_string())?;
    if output.timed_out
        || (output.code != 0 && !(missing && output.code == 1 && output.stdout.is_empty()))
    {
        return Err("cannot inspect Git content normalization".into());
    }
    Ok(output.stdout)
}

impl Rules {
    pub(super) fn capture(
        root: &Path,
        paths: impl IntoIterator<Item = String>,
        runner: &dyn ProcessRunner,
    ) -> Result<Self, String> {
        let driver_keys = query(
            root,
            &[
                "config",
                "--null",
                "--name-only",
                "--get-regexp",
                "^filter\\..*\\.(clean|process)$",
            ],
            Vec::new(),
            runner,
            true,
        )?;
        let driver_keys = std::str::from_utf8(&driver_keys).map_err(|e| e.to_string())?;
        let drivers = driver_keys
            .split('\0')
            .filter(|key| !key.is_empty())
            .map(|key| {
                key.strip_prefix("filter.")
                    .and_then(|key| {
                        key.strip_suffix(".clean")
                            .or_else(|| key.strip_suffix(".process"))
                    })
                    .ok_or("invalid Git filter driver name")
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let paths = paths.into_iter().collect::<BTreeSet<_>>();
        let mut args = vec!["check-attr", "-z", "--stdin"];
        args.extend(ATTRS);
        let input = paths.iter().flat_map(|p| p.bytes().chain([0])).collect();
        let bytes = query(root, &args, input, runner, false)?;
        let fields = bytes.split(|b| *b == 0).collect::<Vec<_>>();
        if fields.last() != Some(&&b""[..]) || fields.len() != paths.len() * ATTRS.len() * 3 + 1 {
            return Err("invalid Git normalization attribute framing".into());
        }
        let mut attributes = BTreeMap::new();
        let mut offset = 0;
        for path in paths {
            let mut values = Vec::new();
            for attr in ATTRS {
                if fields[offset] != path.as_bytes() || fields[offset + 1] != attr.as_bytes() {
                    return Err("unexpected Git normalization attribute".into());
                }
                let value = std::str::from_utf8(fields[offset + 2])
                    .map_err(|e| e.to_string())?
                    .to_string();
                if attr == "filter"
                    && (drivers.contains(value.as_str())
                        || !matches!(value.as_str(), "unspecified" | "unset"))
                {
                    return Err(format!(
                        "external Git filter is not supported for validation input: {path}"
                    ));
                }
                values.push(value);
                offset += 3;
            }
            attributes.insert(path, values);
        }
        let config = String::from_utf8(query(
            root,
            &[
                "config",
                "--null",
                "--get-regexp",
                "^core\\.(autocrlf|eol|safecrlf)$",
            ],
            Vec::new(),
            runner,
            true,
        )?)
        .map_err(|e| e.to_string())?;
        Ok(Self { attributes, config })
    }

    pub(super) fn verify(&self, root: &Path, runner: &dyn ProcessRunner) -> Result<(), String> {
        if Self::capture(root, self.attributes.keys().cloned(), runner)? != *self {
            return Err("Git normalization rules changed during validation; no apply".into());
        }
        Ok(())
    }

    pub(super) fn planning_digest(&self) -> Result<String, String> {
        Ok(digest(
            &serde_json::to_vec(&json!({"attributes":self.attributes,"config":self.config}))
                .map_err(|e| e.to_string())?,
        ))
    }

    pub(super) fn covers(&self, earlier: &Self) -> bool {
        self.config == earlier.config
            && earlier
                .attributes
                .iter()
                .all(|(path, attributes)| self.attributes.get(path) == Some(attributes))
    }
}

pub(super) struct Identity<'a> {
    root: &'a Path,
    runner: &'a dyn ProcessRunner,
    pub rules: Rules,
    cache: RefCell<BTreeMap<(String, String), Vec<u8>>>,
}

impl<'a> Identity<'a> {
    pub(super) fn new(root: &'a Path, rules: Rules, runner: &'a dyn ProcessRunner) -> Self {
        Self {
            root,
            runner,
            rules,
            cache: RefCell::new(BTreeMap::new()),
        }
    }

    fn eligible(&self, path: &str, bytes: &[u8]) -> Result<bool, String> {
        let attrs = self
            .rules
            .attributes
            .get(path)
            .ok_or_else(|| format!("normalization input was not captured: {path}"))?;
        Ok(attrs[0] != "unset"
            && matches!(attrs[4].as_str(), "unspecified" | "unset")
            && matches!(attrs[5].as_str(), "unspecified" | "unset")
            && !bytes.contains(&0)
            && bytes.windows(2).any(|p| p == b"\r\n"))
    }

    // With external filters, encoding and ident excluded, Git's only possible
    // content conversion is EOL. Compare filtered/unfiltered OIDs in two batches,
    // retaining SHA-256 over LF bytes for compatibility with old LF certificates.
    pub(super) fn prepare(
        &self,
        inputs: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) -> Result<(), String> {
        let mut paths = Vec::new();
        let mut input = Vec::new();
        for (path, bytes) in inputs {
            let Some(bytes) = bytes else {
                continue;
            };
            let Ok(relative) = path.strip_prefix(self.root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if !self.eligible(&relative, bytes)? {
                continue;
            }
            input.extend_from_slice(
                serde_json::to_string(&relative)
                    .map_err(|e| e.to_string())?
                    .as_bytes(),
            );
            input.push(b'\n');
            paths.push((relative, bytes));
        }
        if paths.is_empty() {
            return Ok(());
        }
        let canonical = query(
            self.root,
            &["hash-object", "--stdin-paths"],
            input.clone(),
            self.runner,
            false,
        )?;
        let raw = query(
            self.root,
            &["hash-object", "--stdin-paths", "--no-filters"],
            input,
            self.runner,
            false,
        )?;
        let lines = |bytes: Vec<u8>| -> Result<Vec<String>, String> {
            let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            let result = text.lines().map(str::to_string).collect::<Vec<_>>();
            if result.len() != paths.len()
                || result.iter().any(|s| {
                    !matches!(s.len(), 40 | 64) || !s.bytes().all(|b| b.is_ascii_hexdigit())
                })
            {
                return Err("invalid Git normalization object ids".into());
            }
            Ok(result)
        };
        let canonical = lines(canonical)?;
        let raw = lines(raw)?;
        for ((path, bytes), (raw, canonical)) in paths.into_iter().zip(raw.iter().zip(&canonical)) {
            self.cache.borrow_mut().insert(
                (path, digest(bytes)),
                if raw != canonical {
                    lf(bytes)
                } else {
                    bytes.clone()
                },
            );
        }
        Ok(())
    }

    pub(super) fn bytes(&self, path: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        // This contract only equates EOL representation. Encoding/ident transforms
        // and byte-sensitive/binary inputs retain their original SHA-256 identity.
        if !self.eligible(path, bytes)? {
            return Ok(bytes.to_vec());
        }
        let key = (path.to_string(), digest(bytes));
        if let Some(value) = self.cache.borrow().get(&key) {
            return Ok(value.clone());
        }
        let lf = lf(bytes);
        let canonical = query(
            self.root,
            &["hash-object", "--stdin", "--path", path],
            bytes.to_vec(),
            self.runner,
            false,
        )?;
        let candidate = query(
            self.root,
            &["hash-object", "--stdin", "--no-filters"],
            lf.clone(),
            self.runner,
            false,
        )?;
        let result = if canonical == candidate {
            lf
        } else {
            bytes.to_vec()
        };
        self.cache.borrow_mut().insert(key, result.clone());
        Ok(result)
    }

    pub(super) fn hash(&self, path: &str, bytes: Option<&[u8]>) -> Result<Option<String>, String> {
        bytes
            .map(|b| self.bytes(path, b).map(|v| digest(&v)))
            .transpose()
    }
}

fn lf(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if *b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                None
            } else {
                Some(*b)
            }
        })
        .collect()
}
