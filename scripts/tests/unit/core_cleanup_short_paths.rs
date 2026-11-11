use super::*;

#[cfg(windows)]
#[test]
#[ignore = "explicit read-only project smoke; requires BFC_CLEANUP_PREVIEW_ROOT and BFC_CLEANUP_PREVIEW_KEEPS"]
fn cleanup_external_permission_preview() {
    let root = PathBuf::from(std::env::var_os("BFC_CLEANUP_PREVIEW_ROOT").expect("explicit preview root"));
    let keeps = std::env::var_os("BFC_CLEANUP_PREVIEW_KEEPS").expect("explicit keep-list input");
    let keep: Vec<String> = serde_json::from_slice(&fs::read(keeps).unwrap()).unwrap();
    let report = Plan::capture_with_build(&root, BuildPolicy { keep, ..BuildPolicy::default() }).unwrap().preview();
    assert!(report.read_only);
    assert_eq!(report.deleted_files + report.deleted_directories + report.deleted_junctions, 0);
    println!("BFC_PERMISSION_PREVIEW={}", serde_json::to_string(&report).unwrap());
}

#[cfg(windows)]
#[test]
fn cleanup_short_alias_keeps_the_long_named_product_directory() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    fn short(path: &Path) -> PathBuf {
        let input = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut output = vec![0u16; 32768];
        let length =
            unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), output.len() as u32) };
        assert!(length > 0 && (length as usize) < output.len());
        PathBuf::from(std::ffi::OsString::from_wide(&output[..length as usize]))
    }
    let f = Fixture::new();
    f.put(".build/application-bundle/current.bin", b"keep");
    f.put(".build/cache/old.obj", b"remove");
    let relative = short(&f.0.join(".build/application-bundle"))
        .strip_prefix(short(&f.0))
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    assert_eq!(windows::metadata_product_path(&f.0.join(&relative)).unwrap(), f.0.join(".build/application-bundle"));
    let report = Plan::capture_with_build(&f.0, keeping(&[&relative]))
        .unwrap()
        .execute();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 1);
    assert_eq!(
        fs::read(f.0.join(".build/application-bundle/current.bin")).unwrap(),
        b"keep"
    );
}
