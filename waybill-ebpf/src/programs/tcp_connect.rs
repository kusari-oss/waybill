//! #1252 — outbound connections, observed at the syscall boundary.
//!
//! ## Why this is not a kprobe any more
//!
//! This used to be a kprobe on `tcp_v4_connect` that read the destination
//! address out of `struct sock` at hardcoded offsets 4 and 12, with a
//! comment admitting the offsets "vary by kernel version". That is two
//! unstable dependencies in one probe — an internal symbol and an internal
//! struct layout — and it had a third problem: `tcp_v4_connect` is reached
//! only by IPv4 connections, so a build that connected over IPv6 was
//! recorded as having made no connection at all, which is
//! indistinguishable from an offline build.
//!
//! `sys_enter_connect` fixes all three. The destination comes from the
//! `struct sockaddr` the process itself passed, whose layout is UAPI, and
//! the address family is a field in it rather than something inferred from
//! which kernel function was reached.
//!
//! ## Why connects keep attempt semantics while opens do not
//!
//! #1252 narrowed *opens* to those that succeeded, by joining the request
//! to its result in the kernel. Connects deliberately keep the old
//! behaviour: this program records the attempt and there is no
//! `sys_exit_connect`.
//!
//! That is not an oversight, and it should not be "finished" later.
//! `connect(2)` on a non-blocking socket returns `-EINPROGRESS` — a
//! connection in flight, not a failure — so a success filter modelled on
//! the open join would discard exactly the asynchronous connects a build's
//! HTTP client makes. The address is also available at entry, where the
//! user pages are mapped, so the exit tracepoint would buy nothing.

use aya_ebpf::{
    helpers::{bpf_ktime_get_ns, bpf_probe_read_user},
    macros::tracepoint,
    programs::TracePointContext,
};

use waybill_common::events::{NetworkEvent, NetworkEventType};
use waybill_common::ip::IpAddr;

use crate::helpers::{current_comm, current_pid, current_tid, increment_drop_counter, should_trace};
use crate::maps::{CONN_INFO, NETWORK_EVENTS, NETWORK_EVENT_DROPS};

/// `sys_enter_connect(fd, uservaddr, addrlen)` — argument 1 at offset 24.
const ARG1: usize = 24;

const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;

/// `struct sockaddr_in`:  `sa_family` u16, `sin_port` be16 at 2,
/// `sin_addr` be32 at 4.
/// `struct sockaddr_in6`: `sa_family` u16, `sin6_port` be16 at 2,
/// `sin6_flowinfo` at 4, `sin6_addr` 16 bytes at 8.
/// Both are UAPI (`<netinet/in.h>`), which is the whole point of reading
/// them instead of `struct sock`.
#[inline(always)]
unsafe fn read_destination(addr_ptr: u64) -> Option<(IpAddr, u16)> {
    if addr_ptr == 0 {
        return None;
    }
    let family: u16 = bpf_probe_read_user(addr_ptr as *const u16).ok()?;
    let port_be: u16 = bpf_probe_read_user((addr_ptr + 2) as *const u16).ok()?;
    let port = u16::from_be(port_be);
    match family {
        AF_INET => {
            let raw: u32 = bpf_probe_read_user((addr_ptr + 4) as *const u32).ok()?;
            // Network byte order: the first octet is the low byte on a
            // little-endian host once read as a u32.
            let o = raw.to_ne_bytes();
            Some((IpAddr::new_v4(o[0], o[1], o[2], o[3]), port))
        }
        AF_INET6 => {
            let octets: [u8; 16] = bpf_probe_read_user((addr_ptr + 8) as *const [u8; 16]).ok()?;
            Some((IpAddr::new_v6(octets), port))
        }
        // AF_UNIX and the rest: not an outbound network connection, so
        // not this capture's business. Silently ignored rather than
        // recorded as an unknown destination.
        _ => None,
    }
}

#[tracepoint]
pub fn sys_enter_connect(ctx: TracePointContext) -> u32 {
    if !should_trace() {
        return 0;
    }
    let addr_ptr = match unsafe { ctx.read_at::<u64>(ARG1) } {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let Some((dst_addr, dst_port)) = (unsafe { read_destination(addr_ptr) }) else {
        return 0;
    };

    let pid = current_pid();
    let tid = current_tid();
    let comm = current_comm();
    let timestamp = unsafe { bpf_ktime_get_ns() };

    // The connection id stays what it has always been — the timestamp —
    // because the OpenSSL uprobes correlate captured plaintext to a
    // destination through `CONN_INFO` keyed on it. Changing the key here
    // would leave connections recorded but no longer joined to their
    // payloads, which fails quietly.
    let conn_id = timestamp;
    let conn_info = waybill_common::maps::ConnInfo {
        pid,
        tid,
        dst_addr,
        dst_port,
        _padding: [0; 6],
        established_ns: timestamp,
    };
    if CONN_INFO.insert(&conn_id, &conn_info, 0).is_err() {
        return 0;
    }

    if let Some(mut buf) = NETWORK_EVENTS.reserve::<NetworkEvent>(0) {
        let event = buf.as_mut_ptr();
        unsafe {
            (*event).event_type = NetworkEventType::ConnEstablished;
            (*event).timestamp_ns = timestamp;
            (*event).pid = pid;
            (*event).tid = tid;
            (*event).comm = comm;
            (*event).conn_id = conn_id;
            (*event).dst_addr = dst_addr;
            (*event).dst_port = dst_port;
            (*event).src_addr = IpAddr::new_v4(0, 0, 0, 0);
            (*event).src_port = 0;
            (*event).payload_size = 0;
            (*event).payload_hash = [0; 32];
            (*event).payload_fragment = [0; 512];
            (*event).payload_truncated = 0;
            (*event)._padding = [0; 3];
        }
        buf.submit(0);
    } else {
        increment_drop_counter(&NETWORK_EVENT_DROPS);
    }

    0
}
