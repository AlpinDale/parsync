use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_contains_expected_flags() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-v, --verbose"))
        .stdout(predicate::str::contains("-r, --recursive"))
        .stdout(predicate::str::contains("-P"))
        .stdout(predicate::str::contains("-l, --links"))
        .stdout(predicate::str::contains("-u, --update"));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn help_omits_rdma_flags() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--rdma").not())
        .stdout(predicate::str::contains("--no-rdma").not());
}

#[cfg(not(target_os = "linux"))]
#[test]
fn rdma_flag_is_rejected() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.args(["--rdma=require", "host:/src", "/tmp/dst"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--rdma'"));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn internal_rdma_helper_is_rejected() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.arg("--internal-rdma-send");
    cmd.assert().failure().stderr(predicate::str::contains(
        "unexpected argument '--internal-rdma-send'",
    ));
}

#[test]
fn missing_local_source_fails() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.args(["-r", "invalid-spec", "/tmp/dst"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("local source path not found"));
}

#[test]
fn invalid_remote_spec_fails() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("parsync"));
    cmd.args(["-r", "host:", "/tmp/dst"]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "remote must include non-empty host and path",
    ));
}
