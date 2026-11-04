use super::*;
use crate::ProcessOutput;
use std::cell::{Cell, RefCell};

struct Server {
    root: PathBuf,
    plan: Plan,
    commit: String,
    objects: RefCell<BTreeMap<String, Vec<u8>>>,
    published: RefCell<Option<Value>>,
    tag: RefCell<Option<String>>,
    writes: RefCell<Vec<String>>,
    lose_upload: Cell<bool>,
    lose_create: Cell<bool>,
    delay_upload: Cell<bool>,
    corrupt_during_create: Cell<bool>,
    reject_upload: Cell<bool>,
    reject_create: Cell<bool>,
}
impl Server {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.runtime/tests/native-gitlab")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(root.join(".runtime/config")).unwrap();
        fs::create_dir_all(root.join(".build/package")).unwrap();
        fs::write(
            root.join(".runtime/config/bridgeforge-gitlab-auth.json"),
            br#"{"origin":"http://gitlab.example:8095","token":"fixture-test-token"}"#,
        )
        .unwrap();
        fs::write(root.join(".build/package/app.bin"), b"first artifact").unwrap();
        let plan = Plan {
            schema: 1,
            product: "App".into(),
            version: "1.0.0".into(),
            repository: "group/subgroup/project".into(),
            gitlab_origin: Some("http://gitlab.example:8095".into()),
            tag: "v1.0.0".into(),
            title: "App 1.0.0".into(),
            notes: "Release".into(),
            assets: vec![Asset {
                name: "app.bin".into(),
                path: ".build/package/app.bin".into(),
                sha256: raw_sha(b"first artifact"),
                size: 14,
            }],
        };
        Self {
            root,
            plan,
            commit: "a".repeat(40),
            objects: RefCell::new(BTreeMap::new()),
            published: RefCell::new(None),
            tag: RefCell::new(None),
            writes: RefCell::new(vec![]),
            lose_upload: Cell::new(false),
            lose_create: Cell::new(false),
            delay_upload: Cell::new(false),
            corrupt_during_create: Cell::new(false),
            reject_upload: Cell::new(false),
            reject_create: Cell::new(false),
        }
    }
    fn response(status: u16, value: Value) -> ProcessOutput {
        ProcessOutput {
            code: 0,
            stdout: format!("{value}\n{status}").into_bytes(),
            stderr: vec![],
            timed_out: false,
        }
    }
    fn lost() -> ProcessOutput {
        ProcessOutput {
            code: 28,
            stdout: vec![],
            stderr: b"lost response".to_vec(),
            timed_out: true,
        }
    }
}
impl ProcessRunner for Server {
    fn run(&self, q: &ProcessRequest) -> std::io::Result<ProcessOutput> {
        let args: Vec<_> = q
            .args
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(!args.iter().any(|s| s.contains("fixture-test-token")));
        assert!(String::from_utf8_lossy(&q.stdin).contains("fixture-test-token"));
        assert!(!args.iter().any(|s| s == "--location"));
        let field = |s: &str| args[args.iter().position(|a| a == s).unwrap() + 1].clone();
        let method = field("--request");
        let url = field("--url");
        assert!(
            url.starts_with(
                "http://gitlab.example:8095/api/v4/projects/group%2Fsubgroup%2Fproject/"
            )
        );
        if method != "GET" {
            self.writes.borrow_mut().push(method.clone());
        }
        let result = if url.contains("/repository/tags/") {
            match self.tag.borrow().as_ref() {
                Some(sha) => Self::response(200, json!({"commit":{"id":sha}})),
                None => Self::response(404, json!({})),
            }
        } else if url.contains("/packages?") {
            if self.objects.borrow().is_empty() {
                Self::response(200, json!([]))
            } else {
                Self::response(
                    200,
                    json!([{"id":1,"name":PACKAGE,"version":package_version(&self.plan,&self.commit),"package_type":"generic"}]),
                )
            }
        } else if url.contains("/package_files?") {
            Self::response(200,json!(self.objects.borrow().iter().map(|(name,bytes)|json!({"file_name":name,"size":bytes.len(),"file_sha256":&raw_sha(bytes)[7..]})).collect::<Vec<_>>()))
        } else if url.contains("/packages/generic/") && method == "PUT" {
            if self.reject_upload.get() {
                return Ok(Self::response(401, json!({"message":"Unauthorized"})));
            }
            let path = field("--upload-file");
            if self.delay_upload.get() {
                return Ok(Self::lost());
            }
            self.objects
                .borrow_mut()
                .insert("app.bin".into(), fs::read(path).unwrap());
            if self.lose_upload.replace(false) {
                Self::lost()
            } else {
                Self::response(201, json!({"message":"201 Created"}))
            }
        } else if url.ends_with("/releases") && method == "POST" {
            if self.reject_create.get() {
                return Ok(Self::response(403, json!({"message":"Forbidden"})));
            }
            if self.corrupt_during_create.get() {
                self.objects
                    .borrow_mut()
                    .insert("app.bin".into(), b"changed during publication".to_vec());
            }
            *self.tag.borrow_mut() = Some(self.commit.clone());
            *self.published.borrow_mut() = Some(
                json!({"tag_name":self.plan.tag,"commit":{"id":self.commit},"upcoming_release":false,
                "released_at":"2026-11-04T00:00:00Z","description":marker(&self.plan,&self.commit).unwrap(),
                "_links":{"self":"http://gitlab.example:8095/group/subgroup/project/-/releases/v1.0.0"},
                "assets":{"links":[{"name":"app.bin","url":asset_url(&self.plan,&self.commit,"app.bin").unwrap()}]}}),
            );
            if self.lose_create.replace(false) {
                Self::lost()
            } else {
                Self::response(201, self.published.borrow().clone().unwrap())
            }
        } else if url.contains("/releases/") {
            match self.published.borrow().as_ref() {
                Some(r) => Self::response(200, r.clone()),
                None => Self::response(404, json!({})),
            }
        } else {
            panic!("unexpected {method} {url}")
        };
        Ok(result)
    }
}

#[test]
fn gitlab_validates_host_port_subgroups_and_rejects_credentials_or_other_remote() {
    assert!(
        match_remote(
            "http://gitlab.example:8095/g/sub/p.git",
            "g/sub/p",
            "http://gitlab.example:8095"
        )
        .is_ok()
    );
    assert!(
        match_remote(
            "https://other/g/sub/p.git",
            "g/sub/p",
            "http://gitlab.example:8095"
        )
        .is_err()
    );
    for origin in [
        "https://user:secret@host",
        "https://host/path",
        "https://host?x=1",
        "http://host:0",
        "http://host\nheader:x",
    ] {
        assert!(
            bridgeforge_core::native_release::validate_repository("g/p", Some(origin)).is_err()
        );
    }
}

#[test]
fn gitlab_uploads_before_publishing_and_reconciles_lost_responses_without_repeating_writes() {
    let s = Server::new();
    s.lose_upload.set(true);
    s.lose_create.set(true);
    let result = publish(&s.root, &s, &s.plan, &s.commit).unwrap();
    assert_eq!(result["status"], "released");
    assert_eq!(result["publication_verified"], true);
    assert_eq!(*s.writes.borrow(), vec!["PUT", "POST"]);
    let reused = verify_existing(&s.root, &s, &s.plan, &s.commit).unwrap();
    assert_eq!(reused["publication_reused"], true);
    assert_eq!(s.writes.borrow().len(), 2);
}

#[test]
fn gitlab_rejects_existing_corrupt_bytes_without_upload_or_overwrite() {
    let s = Server::new();
    s.objects
        .borrow_mut()
        .insert("app.bin".into(), b"foreign".to_vec());
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert!(s.writes.borrow().is_empty());
}

#[test]
fn gitlab_rejects_tag_drift_and_foreign_release_links() {
    let s = Server::new();
    *s.tag.borrow_mut() = Some("b".repeat(40));
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert!(s.writes.borrow().is_empty());
    *s.tag.borrow_mut() = None;
    publish(&s.root, &s, &s.plan, &s.commit).unwrap();
    s.published.borrow_mut().as_mut().unwrap()["assets"]["links"][0]["url"] =
        json!("http://other/file");
    assert!(verify_existing(&s.root, &s, &s.plan, &s.commit).is_err());
    assert_eq!(s.writes.borrow().len(), 2);
}

#[test]
fn old_github_identity_remains_byte_compatible_and_gitlab_identity_is_bound() {
    let old = json!({"product":"App","repository":"owner/repo","version":"1.0.0","tag_prefix":"v","paths":["VERSION"]});
    let identity: SourceIdentity = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(&identity).unwrap(), old);
    let s = Server::new();
    let mut changed = s.plan.clone();
    changed.gitlab_origin = Some("http://other".into());
    assert_ne!(
        marker(&s.plan, &s.commit).unwrap(),
        marker(&changed, &s.commit).unwrap()
    );
}

#[test]
fn unknown_upload_is_persisted_until_late_visibility_without_duplicate_put() {
    let s = Server::new();
    s.delay_upload.set(true);
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert_eq!(*s.writes.borrow(), vec!["PUT"]);
    s.objects
        .borrow_mut()
        .insert("app.bin".into(), b"first artifact".to_vec());
    let result = publish(&s.root, &s, &s.plan, &s.commit).unwrap();
    assert_eq!(result["publication_verified"], true);
    assert_eq!(*s.writes.borrow(), vec!["PUT", "POST"]);
}

#[test]
fn creation_time_asset_change_cannot_be_reported_as_verified() {
    let s = Server::new();
    s.corrupt_during_create.set(true);
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert!(verify_existing(&s.root, &s, &s.plan, &s.commit).is_err());
}

#[test]
fn definitive_upload_and_creation_rejections_resume_after_conditions_are_fixed() {
    let s = Server::new();
    s.reject_upload.set(true);
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    s.reject_upload.set(false);
    s.reject_create.set(true);
    assert!(publish(&s.root, &s, &s.plan, &s.commit).is_err());
    assert_eq!(*s.writes.borrow(), vec!["PUT", "PUT", "POST"]);
    s.reject_create.set(false);
    assert_eq!(
        publish(&s.root, &s, &s.plan, &s.commit).unwrap()["publication_verified"],
        true
    );
    assert_eq!(*s.writes.borrow(), vec!["PUT", "PUT", "POST", "POST"]);
}
