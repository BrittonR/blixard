//! Transport benchmarks

#[cfg(all(test, not(madsim)))]
pub mod raft_transport_bench;