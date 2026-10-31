#![no_std]
#![no_main]

mod helpers;
mod maps;
mod programs;

// Re-export all eBPF program entry points so aya can find them.
// Milestone 211 (issue #611): vfs_open_entry retired — see
// programs/file_ops.rs for the bpf_d_path kernel-restriction rationale.
// #1252: the open and connect kprobes were removed. Capture attaches to
// syscall tracepoints in `programs::open_syscalls` and
// `programs::tcp_connect`, which — like `programs::fs_syscalls` and
// `programs::compiler_exec` — need no re-export: aya finds a tracepoint
// program by its ELF section name.
pub use programs::file_ops::{vfs_read_entry, vfs_write_entry};
pub use programs::tls_openssl::{ssl_read_entry, ssl_read_return, ssl_write_entry};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
