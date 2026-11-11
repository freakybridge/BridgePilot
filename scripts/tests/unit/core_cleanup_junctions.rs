use super::*;
use std::{os::windows::process::CommandExt, process::Command};

fn junction(link: &Path, target: &Path) {
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    let result = Command::new("cmd")
        .creation_flags(0x08000000)
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn cleanup_junction_preview_and_deletion_never_visit_external_target() {
    let f = Fixture::new();
    let external = Fixture::new();
    external.put(
        "target/keep.bin",
        b"external data must remain byte-identical",
    );
    f.put(".runtime/nested/ordinary.txt", b"remove ordinary content");
    let link = f.0.join(".runtime/nested/node_modules");
    junction(&link, &external.0.join("target"));
    let before = external.tree();
    let local_before = f.tree();
    let plan = Plan::capture(&f.0).unwrap();
    let preview = plan.preview();
    assert_eq!(preview.planned_junctions, 1);
    assert_eq!(preview.planned_files, 1);
    assert_eq!(preview.junctions[0].path, ".runtime/nested/node_modules");
    assert_eq!(preview.junctions[0].action, "remove_link_only");
    assert_eq!(f.tree(), local_before);
    assert_eq!(external.tree(), before);
    let report = plan.execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_junctions, 1);
    assert_eq!(report.deleted_files, 1);
    assert_eq!(report.remaining_entries, Some(0));
    assert_eq!(external.tree(), before);
    assert!(fs::symlink_metadata(link).is_err());
}

#[test]
fn cleanup_keeps_exact_and_parent_protected_junctions_including_dangling_links() {
    let f = Fixture::new();
    let external = Fixture::new();
    external.put("target/keep.bin", b"keep");
    junction(&f.0.join(".build/current"), &external.0.join("target"));
    junction(
        &f.0.join(".runtime/kept/dangling"),
        &external.0.join("missing"),
    );
    junction(&f.0.join(".runtime/config"), &external.0.join("target"));
    junction(&f.0.join(".runtime/download"), &external.0.join("missing"));
    junction(
        &f.0.join(".runtime/protected/link"),
        &external.0.join("target"),
    );
    junction(
        &f.0.join(".runtime/removed-dangling"),
        &external.0.join("missing"),
    );
    let report = Plan::capture_with_build(
        &f.0,
        keeping(&[
            ".build/current",
            ".runtime/kept/dangling",
            ".runtime/protected",
        ]),
    )
    .unwrap()
    .execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_junctions, 1);
    for path in [
        ".build/current",
        ".runtime/kept/dangling",
        ".runtime/config",
        ".runtime/download",
        ".runtime/protected/link",
    ] {
        assert!(fs::symlink_metadata(f.0.join(path)).is_ok(), "{path}");
    }
    assert_eq!(
        fs::read(external.0.join("target/keep.bin")).unwrap(),
        b"keep"
    );
}

#[test]
fn cleanup_junction_target_or_type_replacement_blocks_before_first_delete() {
    for replace_with_directory in [false, true] {
        let f = Fixture::new();
        let external = Fixture::new();
        external.put("first/data", b"first");
        external.put("second/data", b"second");
        f.put(".runtime/a.txt", b"must survive");
        let link = f.0.join(".build/link");
        junction(&link, &external.0.join("first"));
        let plan = Plan::capture(&f.0).unwrap();
        fs::remove_dir(&link).unwrap();
        if replace_with_directory {
            fs::create_dir(&link).unwrap();
        } else {
            junction(&link, &external.0.join("second"));
        }
        let report = plan.execute();
        assert_eq!(report.status, "blocked", "{:?}", report.failures);
        assert_eq!(
            report.deleted_files + report.deleted_directories + report.deleted_junctions,
            0
        );
        assert_eq!(
            fs::read(f.0.join(".runtime/a.txt")).unwrap(),
            b"must survive"
        );
        assert_eq!(fs::read(external.0.join("first/data")).unwrap(), b"first");
        assert_eq!(fs::read(external.0.join("second/data")).unwrap(), b"second");
    }
}

#[test]
fn cleanup_junction_root_and_linked_product_ancestors_remain_blocked() {
    let f = Fixture::new();
    let external = Fixture::new();
    external.put("target/data", b"keep");
    junction(&f.0.join(".runtime"), &external.0.join("target"));
    assert!(Plan::capture(&f.0).is_err());
    assert!(Plan::capture(&f.0.join(".runtime")).is_err());
    fs::remove_dir(f.0.join(".runtime")).unwrap();
    junction(&f.0.join(".build/link"), &external.0.join("target"));
    assert!(Plan::capture_with_build(&f.0, keeping(&[".build/link/data"])).is_err());
    assert_eq!(fs::read(external.0.join("target/data")).unwrap(), b"keep");
}

#[test]
fn cleanup_junction_payload_rejects_symlinks_volume_mounts_and_malformed_records() {
    fn payload(target: &str) -> Vec<u8> {
        let bytes = target
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut data = Vec::new();
        data.extend_from_slice(&0xa0000003u32.to_le_bytes());
        data.extend_from_slice(&((bytes.len() + 8) as u16).to_le_bytes());
        data.extend_from_slice(&[0; 4]); // Reserved, SubstituteNameOffset
        data.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        data.extend_from_slice(&[0; 4]); // PrintNameOffset, PrintNameLength
        data.extend_from_slice(&bytes);
        data
    }
    let valid = payload(r"\??\C:\external\target");
    assert!(windows::junction_target(&valid).is_ok());
    let mut symlink = valid.clone();
    symlink[..4].copy_from_slice(&0xa000000cu32.to_le_bytes());
    assert!(windows::junction_target(&symlink).is_err());
    assert!(
        windows::junction_target(&payload(
            r"\??\Volume{00000000-0000-0000-0000-000000000000}\"
        ))
        .is_err()
    );
    assert!(windows::junction_target(&payload(r"\??\UNC\server\share")).is_err());
    for length in 0..valid.len() {
        assert!(windows::junction_target(&valid[..length]).is_err());
    }
    let mut offset = valid;
    offset[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(windows::junction_target(&offset).is_err());
}
