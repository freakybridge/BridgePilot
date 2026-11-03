#[allow(dead_code)]
#[path = "../support/product_manifest.rs"]
mod fixture_product;

use super::*;
use std::{
    fs,
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "child process fixture for replacing its own loaded executable"]
fn loaded_image_child() {
    let root = PathBuf::from(std::env::var_os("BFC_IMAGE_ROOT").unwrap());
    let path = std::env::current_exe().unwrap();
    let before = fs::read(&path).unwrap();
    let mut after = before.clone();
    after.extend_from_slice(b"new-image-overlay");
    write_binary(&root, &path, &after).unwrap();
    assert_eq!(fs::read(&path).unwrap(), after);
    fs::write(root.join("ready"), b"ready").unwrap();
    std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    write_binary(&root, &path, &before).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn generated_binary_can_replace_its_loaded_image_and_restore() {
    let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "runtime").join(format!(
        "bfc-image-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(if cfg!(windows) {
        "loaded.exe"
    } else {
        "loaded"
    });
    let before = fs::read(std::env::current_exe().unwrap()).unwrap();
    write_binary(&root, &path, &before).unwrap();
    let mut command = Command::new(&path);
    command
        .args([
            "--exact",
            "runtime::tests::loaded_image_child",
            "--ignored",
            "--nocapture",
        ])
        .env("BFC_IMAGE_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("ready").exists()
        && Instant::now() < deadline
        && child.try_wait().unwrap().is_none()
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !root.join("ready").exists() {
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        let _ = fs::remove_dir_all(&root);
        panic!(
            "child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut expected = before.clone();
    expected.extend_from_slice(b"new-image-overlay");
    assert_eq!(fs::read(&path).unwrap(), expected);
    assert!(
        child.try_wait().unwrap().is_none(),
        "original process must stay alive after replacement"
    );
    child.stdin.take().unwrap().write_all(b"x").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    cleanup_images(&root).unwrap();
    assert_eq!(
        fs::read_dir(image_directory(&root).unwrap())
            .unwrap()
            .count(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "parent process fixture for managed peer replacement"]
fn loaded_peer_parent() {
    let root = PathBuf::from(std::env::var_os("BFC_PEER_ROOT").unwrap());
    let path = std::env::current_exe().unwrap();
    let before = fs::read(&path).unwrap();
    let mut request = crate::ProcessRequest::new(
        root.join(".build/bridgeforge/bin/bridgeforge-factory.exe")
            .into_os_string(),
        &root,
    );
    request.args = [
        "--exact",
        "runtime::tests::loaded_peer_child",
        "--ignored",
        "--nocapture",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    request
        .env
        .insert("BFC_PEER_ROOT".into(), root.clone().into_os_string());
    request
        .env
        .insert("BFC_PEER_TARGET".into(), path.clone().into_os_string());
    request.env.insert(
        "BFC_PEER_ALLOWED".into(),
        std::env::var_os("BFC_PEER_ALLOWED").unwrap(),
    );
    request.timeout = Duration::from_secs(45);
    let result = crate::SystemProcessRunner.run(&request).unwrap();
    assert!(!result.timed_out);
    assert_eq!(
        result.code,
        0,
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(
        root.join("parent-continued"),
        b"original parent continued after child exit",
    )
    .unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "factory child process fixture for replacing its active parent"]
fn loaded_peer_child() {
    let root = PathBuf::from(std::env::var_os("BFC_PEER_ROOT").unwrap());
    let target = PathBuf::from(std::env::var_os("BFC_PEER_TARGET").unwrap());
    assert_eq!(
        std::env::current_exe().unwrap().file_name().unwrap(),
        "bridgeforge-factory.exe"
    );
    let allowed = std::env::var("BFC_PEER_ALLOWED").unwrap() == "1";
    let before = fs::read(&target).unwrap();
    let mut after = before.clone();
    after.extend_from_slice(b"managed-peer-new-image-overlay");
    let result = write_binary(&root, &target, &after);
    if allowed {
        result.unwrap();
        assert_eq!(fs::read(&target).unwrap(), after);
        let images = image_directory(&root).unwrap();
        let retained = fs::read_dir(&images)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read(retained[0].path()).unwrap(), before);
        cleanup_images(&root).unwrap();
        assert_eq!(
            fs::read_dir(&images).unwrap().count(),
            1,
            "cleanup must preserve the still-loaded parent image"
        );
    } else {
        assert!(
            result.is_err(),
            "unmanaged loaded peer must retain its original failure semantics"
        );
        assert_eq!(fs::read(&target).unwrap(), before);
        assert!(!root.join(".build/bridgeforge/git-sync-images").exists());
    }
    fs::write(root.join("peer-ready"), b"ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("peer-continue").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        root.join("peer-continue").exists(),
        "outer fixture did not release the child"
    );
    if allowed {
        write_binary(&root, &target, &before).unwrap();
    }
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[cfg(windows)]
#[test]
fn managed_factory_child_replaces_loaded_parent_and_rejects_unmanaged_peers() {
    use std::os::windows::process::CommandExt;
    for (relative, allowed) in [
        (".build/bridgeforge/bin/bridgeforge.exe", true),
        (".build/bridgeforge/bin/project_demo.exe", false),
        ("outside/bridgeforge.exe", false),
    ] {
        let root = test_artifacts::root(env!("CARGO_MANIFEST_DIR"), "runtime").join(format!(
            "bfc-peer-image-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".build/bridgeforge/bin")).unwrap();
        fs::create_dir_all(root.join("templates")).unwrap();
        fixture_product::write(root.join("source/forge/bridgeforge-manifest.json"), b"{}").unwrap();
        let parent = root.join(relative);
        fs::create_dir_all(parent.parent().unwrap()).unwrap();
        let before = fs::read(std::env::current_exe().unwrap()).unwrap();
        write_binary(&root, &parent, &before).unwrap();
        write_binary(
            &root,
            &root.join(".build/bridgeforge/bin/bridgeforge-factory.exe"),
            &before,
        )
        .unwrap();
        let mut process = Command::new(&parent)
            .args([
                "--exact",
                "runtime::tests::loaded_peer_parent",
                "--ignored",
                "--nocapture",
            ])
            .env("BFC_PEER_ROOT", &root)
            .env("BFC_PEER_ALLOWED", if allowed { "1" } else { "0" })
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(25);
        while !root.join("peer-ready").exists()
            && Instant::now() < deadline
            && process.try_wait().unwrap().is_none()
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ready = root.join("peer-ready").exists();
        if ready {
            let mut expected = before.clone();
            if allowed {
                expected.extend_from_slice(b"managed-peer-new-image-overlay");
            }
            assert_eq!(fs::read(&parent).unwrap(), expected);
            assert!(
                process.try_wait().unwrap().is_none(),
                "original parent must remain alive throughout child replacement"
            );
        }
        // Always release the fixture normally, including diagnostics on failure.
        fs::write(root.join("peer-continue"), b"continue").unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(
            ready && output.status.success(),
            "{relative}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(root.join("parent-continued").exists());
        assert_eq!(fs::read(&parent).unwrap(), before);
        cleanup_images(&root).unwrap();
        assert_eq!(
            fs::read_dir(image_directory(&root).unwrap())
                .unwrap()
                .count(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[allow(dead_code)]
#[path = "../support/artifact_paths.rs"]
mod test_artifacts;
