use super::*;
use std::time::Instant;

#[test]
#[ignore = "explicit 20,000-file scan/delete throughput benchmark"]
fn cleanup_scan_and_delete_throughput() {
    let f = Fixture::new();
    f.put(".runtime/config/current", b"config");
    f.put(".deps/current", b"dependency");
    f.put(".build/application/current.exe", b"current product");
    for index in 0..20_000 {
        f.put(&format!(".runtime/tests/{:03}/{index}.obj", index / 200), b"cache");
    }
    let scan = Instant::now();
    let plan = Plan::capture_with_build(&f.0, keeping(&[".build/application/current.exe"])).unwrap();
    let scan_ms = scan.elapsed().as_millis();
    assert_eq!(plan.preview().planned_files, 20_000);
    let execute = Instant::now();
    let report = plan.execute();
    let execute_ms = execute.elapsed().as_millis();
    assert_eq!(report.status, "completed", "{:?}", report.failures);
    assert_eq!(report.deleted_files, 20_000);
    assert_eq!(report.remaining_entries, Some(0));
    for (path, bytes) in [(".runtime/config/current", &b"config"[..]), (".deps/current", &b"dependency"[..]), (".build/application/current.exe", &b"current product"[..])] {
        assert_eq!(fs::read(f.0.join(path)).unwrap(), bytes);
    }
    println!("CLEANUP_THROUGHPUT={{\"files\":20000,\"scan_ms\":{scan_ms},\"execute_ms\":{execute_ms},\"deleted_files\":{},\"deleted_directories\":{}}}", report.deleted_files, report.deleted_directories);
}
