#[test]
#[ignore = "requires virtme-ng, qemu, rdma-core, sshd, and kernel rdma_rxe support"]
fn validates_rdma_fast_path_with_rxe_vm() {
    let status = std::process::Command::new("bash")
        .arg("scripts/validate-rdma-rxe-vm.sh")
        .status()
        .expect("run RDMA RXE validation script");
    assert!(status.success(), "RDMA RXE validation script failed");
}
